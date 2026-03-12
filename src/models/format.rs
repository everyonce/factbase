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
    /// `<!-- factbase:hex_id -->` HTML comment (default)
    #[default]
    Comment,
    /// `factbase_id: hex_id` in YAML frontmatter
    Frontmatter,
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
            frontmatter: false,
            inline_links: false,
            id_placement: IdPlacement::Comment,
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
            reviewed_in_frontmatter: self.reviewed_in_frontmatter.unwrap_or(base.reviewed_in_frontmatter),
        }
    }
}

/// CSS content for the Obsidian snippet file.
pub const OBSIDIAN_CSS_SNIPPET: &str = r#"/* Factbase custom styles — auto-generated, do not edit */

/* Review Queue callout: amber colour + clipboard-check icon */
.callout[data-callout="review"] {
    --callout-color: 245, 158, 11;
    --callout-icon: lucide-clipboard-check;
}

/* Temporal tag (@t[...]) pill styling for inline code */
.markdown-rendered code,
.cm-s-obsidian .cm-inline-code {
    border-radius: 4px;
    padding: 1px 5px;
}
"#;

/// Write `.obsidian/snippets/factbase.css` under `repo_path` if the repo uses
/// the obsidian preset.  Creates the directory if needed.  Idempotent.
pub fn write_obsidian_css_snippet(repo_path: &std::path::Path) -> std::io::Result<()> {
    let snippets_dir = repo_path.join(".obsidian").join("snippets");
    std::fs::create_dir_all(&snippets_dir)?;
    std::fs::write(snippets_dir.join("factbase.css"), OBSIDIAN_CSS_SNIPPET)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_format() {
        let cfg = FormatConfig::default();
        let r = cfg.resolve();
        assert_eq!(r.link_style, LinkStyle::Factbase);
        assert!(!r.frontmatter);
        assert!(!r.inline_links);
        assert_eq!(r.id_placement, IdPlacement::Comment);
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
        assert_eq!(r.id_placement, IdPlacement::Comment); // default
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
        let css_path = tmp.path().join(".obsidian").join("snippets").join("factbase.css");
        assert!(css_path.exists());
        let content = std::fs::read_to_string(&css_path).unwrap();
        assert!(content.contains("[data-callout=\"review\"]"));
        assert!(content.contains("245, 158, 11")); // amber
        assert!(content.contains("lucide-clipboard-check"));
    }

    #[test]
    fn test_write_obsidian_css_snippet_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_obsidian_css_snippet(tmp.path()).unwrap();
        write_obsidian_css_snippet(tmp.path()).unwrap(); // second call should not error
        let css_path = tmp.path().join(".obsidian").join("snippets").join("factbase.css");
        assert!(css_path.exists());
    }
}
