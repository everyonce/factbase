//! Format configuration for document output.
//!
//! Controls how factbase writes documents — link style, frontmatter, ID placement.
//! Configured via `format:` section in `perspective.yaml`.

use serde::{Deserialize, Serialize};

/// Link style for document references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkStyle {
    /// `[[hex_id]]` — factbase default
    #[default]
    Factbase,
    /// `[[Entity Name]]` — Obsidian-compatible wikilinks
    Wikilink,
    /// `[Entity Name](hex_id)` — standard markdown links
    Markdown,
}

/// Where to place the factbase document ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdPlacement {
    /// `factbase_id: hex_id` in YAML frontmatter (default)
    #[default]
    Frontmatter,
    /// Deprecated: treated as `Frontmatter`. Kept for config backward compatibility.
    Comment,
}

/// User-facing format config from `perspective.yaml`.
/// All fields optional — unset fields inherit from preset or defaults.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FormatConfig {
    /// Preset name: "obsidian" applies all obsidian-friendly defaults
    pub preset: Option<String>,
    /// Link style for references
    pub link_style: Option<LinkStyle>,
    /// Emit YAML frontmatter with type, tags, dates
    pub frontmatter: Option<bool>,
    /// Embed `[[Entity Name]]` wikilinks in body text
    pub inline_links: Option<bool>,
    /// Where to place the factbase document ID
    pub id_placement: Option<IdPlacement>,
    /// Wrap review queue in a collapsed callout (for Obsidian rendering)
    pub review_callout: Option<bool>,
    /// Store reviewed dates in YAML frontmatter instead of inline HTML comments
    pub reviewed_in_frontmatter: Option<bool>,
}

/// Fully resolved format settings (no Option fields).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedFormat {
    pub link_style: LinkStyle,
    pub frontmatter: bool,
    pub inline_links: bool,
    pub id_placement: IdPlacement,
    /// Wrap review queue in a collapsed Obsidian callout.
    pub review_callout: bool,
    /// Store reviewed dates in YAML frontmatter instead of inline comments.
    pub reviewed_in_frontmatter: bool,
}

impl Default for ResolvedFormat {
    fn default() -> Self {
        Self {
            link_style: LinkStyle::Factbase,
            frontmatter: true,
            inline_links: false,
            id_placement: IdPlacement::Frontmatter,
            review_callout: false,
            reviewed_in_frontmatter: false,
        }
    }
}

/// Obsidian preset defaults.
const OBSIDIAN: ResolvedFormat = ResolvedFormat {
    link_style: LinkStyle::Wikilink,
    frontmatter: true,
    inline_links: true,
    id_placement: IdPlacement::Frontmatter,
    review_callout: true,
    reviewed_in_frontmatter: true,
};

impl FormatConfig {
    /// Resolve to concrete settings by applying preset defaults then overrides.
    pub fn resolve(&self) -> ResolvedFormat {
        let base = match self.preset.as_deref() {
            Some("obsidian") => OBSIDIAN,
            _ => ResolvedFormat::default(),
        };
        ResolvedFormat {
            link_style: self.link_style.unwrap_or(base.link_style),
            frontmatter: self.frontmatter.unwrap_or(base.frontmatter),
            inline_links: self.inline_links.unwrap_or(base.inline_links),
            id_placement: self.id_placement.unwrap_or(base.id_placement),
            review_callout: self.review_callout.unwrap_or(base.review_callout),
            reviewed_in_frontmatter: self
                .reviewed_in_frontmatter
                .unwrap_or(base.reviewed_in_frontmatter),
        }
    }
}

/// CSS content for the Obsidian snippet file.
pub const OBSIDIAN_CSS_SNIPPET: &str = r#"/* Factbase custom styles — auto-generated, do not edit */

/* ── Review Queue callout ─────────────────────────────────────────── */
.callout[data-callout="review"] {
    --callout-color: 245, 158, 11;
    --callout-icon: lucide-clipboard-check;
}
/* Indent each review question with a left accent bar */
.callout[data-callout="review"] .callout-content li {
    border-left: 2px solid rgba(245, 158, 11, 0.45);
    padding-left: 8px;
    margin-bottom: 4px;
}

/* ── Temporal tag pills (@t[...] rendered as inline code) ─────────── */
/* Amber pill — visually distinct from prose, suggests time/date data  */
.markdown-rendered code,
.cm-s-obsidian .cm-inline-code {
    background-color: rgba(245, 158, 11, 0.12);
    border: 1px solid rgba(245, 158, 11, 0.30);
    border-radius: 4px;
    padding: 1px 5px;
    font-size: 0.85em;
}

/* ── Source footnotes ─────────────────────────────────────────────── */
/* [^n] superscript reference — indigo pill */
.footnote-ref a {
    background-color: rgba(99, 102, 241, 0.12);
    border: 1px solid rgba(99, 102, 241, 0.28);
    border-radius: 3px;
    padding: 0 3px;
    font-size: 0.75em;
    text-decoration: none;
    color: var(--link-color);
}
.footnote-ref a:hover {
    background-color: rgba(99, 102, 241, 0.25);
}
/* Footnote definitions block at bottom of document */
.footnotes {
    border-top: 1px solid var(--background-modifier-border);
    margin-top: 2em;
    padding-top: 0.5em;
    font-size: 0.85em;
    color: var(--text-muted);
}

/* ── Entity type badge (frontmatter 'type' property) ─────────────── */
/* Show type as a green badge; hide internal-only fields */
.metadata-property[data-property-key="factbase_id"],
.metadata-property[data-property-key="reviewed"] {
    display: none;
}
.metadata-property[data-property-key="type"] .metadata-property-value {
    background-color: rgba(16, 185, 129, 0.14);
    border: 1px solid rgba(16, 185, 129, 0.35);
    border-radius: 12px;
    padding: 1px 10px;
    font-size: 0.8em;
    font-weight: 600;
    color: rgb(16, 185, 129);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    display: inline-block;
}

/* ── Wikilink / cross-reference styling ──────────────────────────── */
/* Dashed underline distinguishes [[links]] from plain text */
.internal-link {
    text-decoration: none;
    border-bottom: 1px dashed var(--link-color);
    padding-bottom: 1px;
}
.internal-link:hover {
    border-bottom-style: solid;
}
/* Unresolved (broken) links — muted to signal missing target */
.internal-link.is-unresolved {
    color: var(--text-muted);
    border-bottom-color: var(--text-muted);
    opacity: 0.7;
}

/* ── Hide internal fields ─────────────────────────────────────────── */
/* Hide filename slug — the # heading is the entity name */
.inline-title { display: none; }
"#;

/// Write `.obsidian/snippets/factbase.css` under `repo_path` if the repo uses
/// the obsidian preset.  Creates the directory if needed.  Idempotent.
/// If `.factbase/obsidian.css` exists under `repo_path`, its content is used
/// instead of the compiled-in default.
pub fn write_obsidian_css_snippet(repo_path: &std::path::Path) -> std::io::Result<()> {
    let css = crate::load_file_override(repo_path, "obsidian.css")
        .unwrap_or_else(|| OBSIDIAN_CSS_SNIPPET.to_string());
    let snippets_dir = repo_path.join(".obsidian").join("snippets");
    std::fs::create_dir_all(&snippets_dir)?;
    std::fs::write(snippets_dir.join("factbase.css"), css)
}

/// Write `.obsidian/app.json` with `enabledCssSnippets: ["factbase"]` so the
/// snippet is pre-enabled when Obsidian opens the vault on a fresh clone.
/// Only writes the `enabledCssSnippets` key — safe to commit.  Idempotent.
pub fn write_obsidian_app_json(repo_path: &std::path::Path) -> std::io::Result<()> {
    let obsidian_dir = repo_path.join(".obsidian");
    std::fs::create_dir_all(&obsidian_dir)?;
    std::fs::write(
        obsidian_dir.join("app.json"),
        r#"{"enabledCssSnippets":["factbase"]}"#,
    )
}

/// Factbase KB gitignore block — regenerable artifacts, OS noise, editor noise.
const KB_GITIGNORE_BLOCK: &str = "\
# factbase regenerable artifacts — do not commit
.factbase/factbase.db
.factbase/*.lock
.fastembed_cache/

# OS artifacts
.DS_Store
Thumbs.db
desktop.ini

# Editor artifacts
.vscode/
.idea/
*.swp
*.swo
*~

# Temp / scratch files
*.tmp
*.bak
";

/// Ensure `.gitignore` contains the factbase KB entries.
/// Creates the file if it doesn't exist, or appends the block if the key entry
/// (`.factbase/factbase.db`) is not already present.
/// Returns `true` if the file was created or modified.
pub fn ensure_kb_gitignore(repo_root: &std::path::Path) -> std::io::Result<bool> {
    let path = repo_root.join(".gitignore");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    // Use the key entry as a sentinel — if it's present, the block is already there.
    if existing
        .lines()
        .any(|l| l.trim() == ".factbase/factbase.db")
    {
        return Ok(false);
    }
    let mut content = String::new();
    if !existing.is_empty() && !existing.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(KB_GITIGNORE_BLOCK);
    if existing.is_empty() {
        std::fs::write(&path, content)?;
    } else {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&path)?;
        f.write_all(content.as_bytes())?;
    }
    Ok(true)
}

/// Obsidian-specific gitignore entries: track snippets while ignoring the rest
/// of `.obsidian/`.
const OBSIDIAN_GITIGNORE_ENTRIES: &[&str] = &[
    ".obsidian/",
    "!.obsidian/snippets/",
    "!.obsidian/snippets/factbase.css",
    "!.obsidian/app.json",
];

/// Ensure `.gitignore` contains the Obsidian-specific entries that allow
/// `.obsidian/snippets/factbase.css` and `.obsidian/app.json` to be tracked
/// while keeping the rest of `.obsidian/` gitignored.
/// Returns the list of entries that were added (empty if all already present).
pub fn ensure_obsidian_gitignore(
    repo_root: &std::path::Path,
) -> std::io::Result<Vec<&'static str>> {
    let path = repo_root.join(".gitignore");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let lines: std::collections::HashSet<&str> = existing.lines().map(|l| l.trim()).collect();
    let missing: Vec<&str> = OBSIDIAN_GITIGNORE_ENTRIES
        .iter()
        .copied()
        .filter(|e| !lines.contains(e))
        .collect();
    if missing.is_empty() {
        return Ok(vec![]);
    }
    let mut append = String::new();
    if !existing.is_empty() && !existing.ends_with('\n') {
        append.push('\n');
    }
    for entry in &missing {
        append.push_str(entry);
        append.push('\n');
    }
    if existing.is_empty() {
        std::fs::write(&path, append)?;
    } else {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&path)?;
        f.write_all(append.as_bytes())?;
    }
    Ok(missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_format() {
        let cfg = FormatConfig::default();
        let r = cfg.resolve();
        assert_eq!(r.link_style, LinkStyle::Factbase);
        assert!(r.frontmatter);
        assert!(!r.inline_links);
        assert_eq!(r.id_placement, IdPlacement::Frontmatter);
        assert!(!r.review_callout);
    }

    #[test]
    fn test_obsidian_preset() {
        let cfg = FormatConfig {
            preset: Some("obsidian".into()),
            ..Default::default()
        };
        let r = cfg.resolve();
        assert_eq!(r.link_style, LinkStyle::Wikilink);
        assert!(r.frontmatter);
        assert!(r.inline_links);
        assert_eq!(r.id_placement, IdPlacement::Frontmatter);
        assert!(r.review_callout);
    }

    #[test]
    fn test_obsidian_preset_with_override() {
        let cfg = FormatConfig {
            preset: Some("obsidian".into()),
            inline_links: Some(false),
            ..Default::default()
        };
        let r = cfg.resolve();
        assert_eq!(r.link_style, LinkStyle::Wikilink);
        assert!(r.frontmatter);
        assert!(!r.inline_links); // overridden
        assert_eq!(r.id_placement, IdPlacement::Frontmatter);
    }

    #[test]
    fn test_explicit_fields_no_preset() {
        let cfg = FormatConfig {
            link_style: Some(LinkStyle::Wikilink),
            frontmatter: Some(true),
            ..Default::default()
        };
        let r = cfg.resolve();
        assert_eq!(r.link_style, LinkStyle::Wikilink);
        assert!(r.frontmatter);
        assert!(!r.inline_links); // default
        assert_eq!(r.id_placement, IdPlacement::Frontmatter); // default
    }

    #[test]
    fn test_unknown_preset_uses_defaults() {
        let cfg = FormatConfig {
            preset: Some("unknown".into()),
            ..Default::default()
        };
        let r = cfg.resolve();
        assert_eq!(r, ResolvedFormat::default());
    }

    #[test]
    fn test_serde_roundtrip() {
        let yaml = "preset: obsidian\nlink_style: wikilink\nfrontmatter: true\ninline_links: true\nid_placement: frontmatter\n";
        let cfg: FormatConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(cfg.preset.as_deref(), Some("obsidian"));
        assert_eq!(cfg.link_style, Some(LinkStyle::Wikilink));
        assert_eq!(cfg.frontmatter, Some(true));
        assert_eq!(cfg.inline_links, Some(true));
        assert_eq!(cfg.id_placement, Some(IdPlacement::Frontmatter));
    }

    #[test]
    fn test_serde_partial() {
        let yaml = "link_style: markdown\n";
        let cfg: FormatConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(cfg.link_style, Some(LinkStyle::Markdown));
        assert!(cfg.preset.is_none());
        assert!(cfg.frontmatter.is_none());
    }

    #[test]
    fn test_obsidian_preset_reviewed_in_frontmatter() {
        let cfg = FormatConfig {
            preset: Some("obsidian".into()),
            ..Default::default()
        };
        let r = cfg.resolve();
        assert!(r.reviewed_in_frontmatter);
    }

    #[test]
    fn test_default_format_no_reviewed_in_frontmatter() {
        let cfg = FormatConfig::default();
        let r = cfg.resolve();
        assert!(!r.reviewed_in_frontmatter);
    }

    #[test]
    fn test_reviewed_in_frontmatter_explicit_override() {
        let cfg = FormatConfig {
            preset: Some("obsidian".into()),
            reviewed_in_frontmatter: Some(false),
            ..Default::default()
        };
        let r = cfg.resolve();
        assert!(!r.reviewed_in_frontmatter); // overridden
    }

    #[test]
    fn test_write_obsidian_css_snippet_creates_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_obsidian_css_snippet(tmp.path()).unwrap();
        let css_path = tmp
            .path()
            .join(".obsidian")
            .join("snippets")
            .join("factbase.css");
        assert!(css_path.exists());
        let content = std::fs::read_to_string(&css_path).unwrap();
        assert!(content.contains("[data-callout=\"review\"]"));
        assert!(content.contains("245, 158, 11")); // amber
        assert!(content.contains("lucide-clipboard-check"));
        assert!(content.contains(".footnote-ref")); // source footnotes
        assert!(content.contains(".internal-link")); // wikilinks
        assert!(content.contains("data-property-key=\"type\"")); // type badge
    }

    #[test]
    fn test_write_obsidian_css_snippet_uses_file_override() {
        let tmp = tempfile::TempDir::new().unwrap();
        let factbase_dir = tmp.path().join(".factbase");
        std::fs::create_dir_all(&factbase_dir).unwrap();
        std::fs::write(factbase_dir.join("obsidian.css"), "/* custom override */").unwrap();
        write_obsidian_css_snippet(tmp.path()).unwrap();
        let css_path = tmp
            .path()
            .join(".obsidian")
            .join("snippets")
            .join("factbase.css");
        let content = std::fs::read_to_string(&css_path).unwrap();
        assert_eq!(content, "/* custom override */");
        assert!(!content.contains("245, 158, 11")); // default not used
    }

    #[test]
    fn test_obsidian_css_snippet_hides_inline_title() {
        assert!(OBSIDIAN_CSS_SNIPPET.contains(".inline-title"));
        assert!(OBSIDIAN_CSS_SNIPPET.contains("display: none"));
    }

    #[test]
    fn test_obsidian_css_snippet_hides_internal_properties() {
        assert!(OBSIDIAN_CSS_SNIPPET.contains("factbase_id"));
        assert!(OBSIDIAN_CSS_SNIPPET.contains("display: none"));
    }

    #[test]
    fn test_write_obsidian_css_snippet_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_obsidian_css_snippet(tmp.path()).unwrap();
        write_obsidian_css_snippet(tmp.path()).unwrap(); // second call should not error
        let css_path = tmp
            .path()
            .join(".obsidian")
            .join("snippets")
            .join("factbase.css");
        assert!(css_path.exists());
    }

    #[test]
    fn test_write_obsidian_app_json_creates_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_obsidian_app_json(tmp.path()).unwrap();
        let app_path = tmp.path().join(".obsidian").join("app.json");
        assert!(app_path.exists());
        let content = std::fs::read_to_string(&app_path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        let snippets = v["enabledCssSnippets"].as_array().unwrap();
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0], "factbase");
    }

    #[test]
    fn test_write_obsidian_app_json_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_obsidian_app_json(tmp.path()).unwrap();
        write_obsidian_app_json(tmp.path()).unwrap();
        let app_path = tmp.path().join(".obsidian").join("app.json");
        assert!(app_path.exists());
    }

    #[test]
    fn test_ensure_kb_gitignore_creates_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let added = ensure_kb_gitignore(tmp.path()).unwrap();
        assert!(added, "should create .gitignore");
        let content = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains(".factbase/factbase.db"));
        assert!(content.contains(".factbase/*.lock"));
        assert!(content.contains(".fastembed_cache/"));
        assert!(content.contains(".DS_Store"));
        assert!(content.contains(".vscode/"));
        assert!(content.contains("*.tmp"));
    }

    #[test]
    fn test_ensure_kb_gitignore_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        ensure_kb_gitignore(tmp.path()).unwrap();
        let added = ensure_kb_gitignore(tmp.path()).unwrap();
        assert!(!added, "second call should add nothing");
    }

    #[test]
    fn test_ensure_kb_gitignore_appends_to_existing() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".gitignore"), "*.log\n").unwrap();
        let added = ensure_kb_gitignore(tmp.path()).unwrap();
        assert!(added);
        let content = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.starts_with("*.log\n"));
        assert!(content.contains(".factbase/factbase.db"));
    }

    #[test]
    fn test_ensure_kb_gitignore_skips_if_entry_present() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".gitignore"),
            "*.log\n.factbase/factbase.db\n",
        )
        .unwrap();
        let added = ensure_kb_gitignore(tmp.path()).unwrap();
        assert!(!added, "should not add if key entry already present");
    }

    #[test]
    fn test_ensure_obsidian_gitignore_creates_entries() {
        let tmp = tempfile::TempDir::new().unwrap();
        let added = ensure_obsidian_gitignore(tmp.path()).unwrap();
        assert_eq!(added.len(), OBSIDIAN_GITIGNORE_ENTRIES.len());
        let content = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains(".obsidian/"));
        assert!(content.contains("!.obsidian/snippets/"));
        assert!(content.contains("!.obsidian/snippets/factbase.css"));
        assert!(content.contains("!.obsidian/app.json"));
    }

    #[test]
    fn test_ensure_obsidian_gitignore_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        ensure_obsidian_gitignore(tmp.path()).unwrap();
        let added = ensure_obsidian_gitignore(tmp.path()).unwrap();
        assert!(added.is_empty(), "second call should add nothing");
    }

    #[test]
    fn test_ensure_obsidian_gitignore_appends_to_existing() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".gitignore"), "*.log\n").unwrap();
        let added = ensure_obsidian_gitignore(tmp.path()).unwrap();
        assert_eq!(added.len(), OBSIDIAN_GITIGNORE_ENTRIES.len());
        let content = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.starts_with("*.log\n"));
        assert!(content.contains(".obsidian/"));
    }

    #[test]
    fn test_ensure_obsidian_gitignore_partial_existing() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Pre-populate with just the first entry
        std::fs::write(tmp.path().join(".gitignore"), ".obsidian/\n").unwrap();
        let added = ensure_obsidian_gitignore(tmp.path()).unwrap();
        // Should add the 3 missing entries
        assert_eq!(added.len(), OBSIDIAN_GITIGNORE_ENTRIES.len() - 1);
    }
}
