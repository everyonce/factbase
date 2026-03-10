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
}

/// Fully resolved format settings (no Option fields).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedFormat {
    pub link_style: LinkStyle,
    pub frontmatter: bool,
    pub inline_links: bool,
    pub id_placement: IdPlacement,
}

impl Default for ResolvedFormat {
    fn default() -> Self {
        Self {
            link_style: LinkStyle::Factbase,
            frontmatter: false,
            inline_links: false,
            id_placement: IdPlacement::Comment,
        }
    }
}

/// Obsidian preset defaults.
const OBSIDIAN: ResolvedFormat = ResolvedFormat {
    link_style: LinkStyle::Wikilink,
    frontmatter: true,
    inline_links: true,
    id_placement: IdPlacement::Frontmatter,
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
        }
    }
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
}
