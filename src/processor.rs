use crate::database::Database;
use rand::Rng;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

static ID_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<!-- factbase:([a-f0-9]{6}) -->").unwrap());

pub struct DocumentProcessor;

impl DocumentProcessor {
    pub fn new() -> Self {
        Self
    }

    pub fn extract_id(&self, content: &str) -> Option<String> {
        let first_line = content.lines().next()?;
        ID_REGEX.captures(first_line).map(|c| c[1].to_string())
    }

    pub fn generate_id(&self) -> String {
        let bytes: [u8; 3] = rand::thread_rng().gen();
        hex::encode(bytes)
    }

    pub fn is_id_unique(&self, id: &str, db: &Database) -> bool {
        db.get_document(id).map(|d| d.is_none()).unwrap_or(true)
    }

    pub fn generate_unique_id(&self, db: &Database) -> String {
        for _ in 0..100 {
            let id = self.generate_id();
            if self.is_id_unique(&id, db) {
                return id;
            }
        }
        self.generate_id() // fallback
    }

    pub fn inject_header(&self, content: &str, id: &str) -> String {
        format!("<!-- factbase:{} -->\n{}", id, content)
    }

    pub fn extract_title(&self, content: &str, path: &Path) -> String {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("<!-- factbase:") {
                continue;
            }
            if let Some(title) = trimmed.strip_prefix("# ") {
                return title.trim().to_string();
            }
            if !trimmed.is_empty() {
                break;
            }
        }
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_string()
    }

    pub fn derive_type(&self, path: &Path, repo_root: &Path) -> String {
        let relative = path.strip_prefix(repo_root).unwrap_or(path);
        if let Some(parent) = relative.parent() {
            if let Some(folder) = parent.file_name().and_then(|s| s.to_str()) {
                if !folder.is_empty() {
                    return self.singularize(folder);
                }
            }
        }
        "document".to_string()
    }

    fn singularize(&self, word: &str) -> String {
        if word.ends_with('s') && word.len() > 1 {
            word[..word.len() - 1].to_string()
        } else {
            word.to_string()
        }
    }
}

impl Default for DocumentProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_id_valid() {
        let p = DocumentProcessor::new();
        assert_eq!(
            p.extract_id("<!-- factbase:a1cb2b -->\n# Title"),
            Some("a1cb2b".into())
        );
    }

    #[test]
    fn test_extract_id_none() {
        let p = DocumentProcessor::new();
        assert_eq!(p.extract_id("# Title\nContent"), None);
    }

    #[test]
    fn test_extract_id_malformed() {
        let p = DocumentProcessor::new();
        assert_eq!(p.extract_id("<!-- factbase:abc -->"), None); // too short
        assert_eq!(p.extract_id("<!-- factbase:ABCDEF -->"), None); // uppercase
    }

    #[test]
    fn test_generate_id_format() {
        let p = DocumentProcessor::new();
        let id = p.generate_id();
        assert_eq!(id.len(), 6);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_extract_title_h1() {
        let p = DocumentProcessor::new();
        assert_eq!(
            p.extract_title(
                "<!-- factbase:abc123 -->\n# My Title\nContent",
                Path::new("test.md")
            ),
            "My Title"
        );
    }

    #[test]
    fn test_extract_title_fallback() {
        let p = DocumentProcessor::new();
        assert_eq!(
            p.extract_title("No heading here", Path::new("my-doc.md")),
            "my-doc"
        );
    }

    #[test]
    fn test_derive_type() {
        let p = DocumentProcessor::new();
        assert_eq!(
            p.derive_type(Path::new("/repo/people/john.md"), Path::new("/repo")),
            "people"
        );
        assert_eq!(
            p.derive_type(Path::new("/repo/projects/foo.md"), Path::new("/repo")),
            "project"
        );
        assert_eq!(
            p.derive_type(Path::new("/repo/notes.md"), Path::new("/repo")),
            "document"
        );
    }
}
