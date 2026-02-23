//! Shared output formatting helpers for CLI commands.
//!
//! This module consolidates output formatting logic to avoid duplication
//! across command modules. It provides helpers for JSON/YAML serialization,
//! TTY detection, and color output.

use serde::Serialize;
use std::io::{self, IsTerminal};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use crate::error::FactbaseError;

/// Global flag for --no-color CLI option.
static NO_COLOR_FLAG: OnceLock<AtomicBool> = OnceLock::new();

/// Set the global no-color flag. Call this from main() when --no-color is passed.
pub fn set_no_color(value: bool) {
    let flag = NO_COLOR_FLAG.get_or_init(|| AtomicBool::new(false));
    flag.store(value, Ordering::SeqCst);
}

/// Check if the --no-color flag was set.
fn is_no_color_flag_set() -> bool {
    NO_COLOR_FLAG
        .get()
        .is_some_and(|f| f.load(Ordering::SeqCst))
}

/// Format data as pretty-printed JSON string.
pub fn format_json<T: Serialize>(data: &T) -> Result<String, FactbaseError> {
    Ok(serde_json::to_string_pretty(data)?)
}

/// Format data as YAML string.
pub fn format_yaml<T: Serialize>(data: &T) -> Result<String, FactbaseError> {
    Ok(serde_yaml_ng::to_string(data)?)
}

/// Check if stdout is a terminal (TTY).
pub fn is_tty() -> bool {
    io::stdout().is_terminal()
}

/// Check if color output should be used.
///
/// Returns false if:
/// - --no-color CLI flag was passed
/// - NO_COLOR environment variable is set (<https://no-color.org/>)
/// - stdout is not a terminal (piped output)
///
/// Returns true otherwise.
pub fn should_use_color() -> bool {
    // Check --no-color CLI flag
    if is_no_color_flag_set() {
        return false;
    }
    // Check NO_COLOR environment variable (https://no-color.org/)
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }
    // Check if stdout is a terminal
    is_tty()
}

/// Determine if highlighting should be used based on explicit flag and environment.
///
/// Priority:
/// 1. Explicit flag value (if Some)
/// 2. NO_COLOR environment variable
/// 3. TTY detection
pub fn should_highlight(explicit_flag: Option<bool>) -> bool {
    match explicit_flag {
        Some(value) => value,
        None => should_use_color(),
    }
}

/// Format byte count as human-readable string (B, KB, MB, GB).
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// ANSI escape codes for terminal colors.
pub mod ansi {
    /// Bold red text (for highlighting matches).
    pub const BOLD_RED: &str = "\x1b[1;31m";
    /// Reset all formatting.
    pub const RESET: &str = "\x1b[0m";
    /// Clear screen.
    pub const CLEAR_SCREEN: &str = "\x1b[2J\x1b[H";
}

/// Highlight all occurrences of a pattern in text using ANSI colors.
pub fn highlight_text(text: &str, pattern: &str) -> String {
    if pattern.is_empty() {
        return text.to_string();
    }
    // Case-insensitive replacement
    let Ok(regex) = regex::RegexBuilder::new(&regex::escape(pattern))
        .case_insensitive(true)
        .build()
    else {
        return text.to_string();
    };
    regex
        .replace_all(text, |caps: &regex::Captures| {
            format!("{}{}{}", ansi::BOLD_RED, &caps[0], ansi::RESET)
        })
        .to_string()
}

/// Truncate text at word boundary with "..." suffix
pub(crate) fn truncate_at_word_boundary(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    // Find a valid char boundary at or before max_len
    let mut end = max_len;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let truncated = &text[..end];
    if let Some(last_space) = truncated.rfind(' ') {
        format!("{}...", &truncated[..last_space])
    } else {
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct TestData {
        name: String,
        value: i32,
    }

    #[test]
    fn test_format_json() {
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };
        let result = format_json(&data).unwrap();
        assert!(result.contains("\"name\": \"test\""));
        assert!(result.contains("\"value\": 42"));
    }

    #[test]
    fn test_format_yaml() {
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };
        let result = format_yaml(&data).unwrap();
        assert!(result.contains("name: test"));
        assert!(result.contains("value: 42"));
    }

    #[test]
    fn test_should_highlight_explicit_true() {
        assert!(should_highlight(Some(true)));
    }

    #[test]
    fn test_should_highlight_explicit_false() {
        assert!(!should_highlight(Some(false)));
    }

    #[test]
    fn test_highlight_text_basic() {
        let result = highlight_text("hello world", "world");
        assert!(result.contains(ansi::BOLD_RED));
        assert!(result.contains(ansi::RESET));
        assert!(result.contains("world"));
    }

    #[test]
    fn test_highlight_text_case_insensitive() {
        let result = highlight_text("Hello World", "world");
        assert!(result.contains(ansi::BOLD_RED));
        assert!(result.contains("World"));
    }

    #[test]
    fn test_highlight_text_empty_pattern() {
        let result = highlight_text("hello world", "");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_highlight_text_no_match() {
        let result = highlight_text("hello world", "xyz");
        assert_eq!(result, "hello world");
        assert!(!result.contains(ansi::BOLD_RED));
    }

    #[test]
    fn test_highlight_text_multiple_matches() {
        let result = highlight_text("hello hello hello", "hello");
        // Should have 3 highlighted occurrences
        assert_eq!(result.matches(ansi::BOLD_RED).count(), 3);
        assert_eq!(result.matches(ansi::RESET).count(), 3);
    }

    #[test]
    fn test_format_bytes_small() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn test_format_bytes_kilobytes() {
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(10240), "10.0 KB");
    }

    #[test]
    fn test_format_bytes_megabytes() {
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 5), "5.0 MB");
        assert_eq!(format_bytes(1024 * 1024 + 512 * 1024), "1.5 MB");
    }

    #[test]
    fn test_format_bytes_gigabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 2), "2.0 GB");
    }

    #[test]
    fn test_set_no_color_disables_color() {
        // Note: This test modifies global state, so it may affect other tests
        // if run in parallel. The OnceLock ensures the flag is only set once.
        set_no_color(true);
        assert!(is_no_color_flag_set());
    }

    #[test]
    fn test_truncate_at_word_boundary_short() {
        assert_eq!(truncate_at_word_boundary("Short text", 100), "Short text");
    }

    #[test]
    fn test_truncate_at_word_boundary_long() {
        let result = truncate_at_word_boundary("This is a longer text that needs truncation", 20);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 23);
    }

    #[test]
    fn test_truncate_at_word_boundary_no_space() {
        assert_eq!(
            truncate_at_word_boundary("Verylongwordwithoutspaces", 10),
            "Verylongwo..."
        );
    }

    #[test]
    fn test_truncate_at_word_boundary_exact_length() {
        assert_eq!(truncate_at_word_boundary("Hello world", 11), "Hello world");
    }

    #[test]
    fn test_truncate_at_word_boundary_trailing_space() {
        assert_eq!(truncate_at_word_boundary("Hello world ", 6), "Hello...");
    }
}
