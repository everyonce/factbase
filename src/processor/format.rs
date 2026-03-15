//! Document format layer — applies format config when writing documents.
//!
//! Handles frontmatter generation, ID placement, and link formatting
//! based on the resolved format configuration.

use crate::models::format::{LinkStyle, ResolvedFormat};

/// YAML frontmatter field keys managed by factbase — these are written by
/// `build_document_header` and must not be duplicated from extra fields.
const MANAGED_FIELDS: &[&str] = &["factbase_id", "type"];

/// Derive tags from a relative file path using directory components.
///
/// Rule: use all directory components; if there are multiple, skip the first
/// (top-level category folder) since it's too broad to be useful as a tag.
///
/// Examples:
/// - `customers/acme/people/alice-chen.md` → `["acme", "people"]`
/// - `services/amazon-aurora.md` → `["services"]`
/// - `doc.md` → `[]`
pub fn tags_from_path(relative_path: &std::path::Path) -> Vec<String> {
    let dirs: Vec<String> = relative_path
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| match c {
                    std::path::Component::Normal(s) => s.to_str().map(|s| s.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    if dirs.len() > 1 {
        dirs[1..].to_vec()
    } else {
        dirs
    }
}

/// Parse a YAML `tags:` line into a list of tag strings.
///
/// Handles flow style `tags: [a, b]` and scalar `tags: a`.
fn parse_tags_line(line: &str) -> Vec<String> {
    let value = match line.split_once(':').map(|x| x.1) {
        Some(v) => v.trim(),
        None => return Vec::new(),
    };
    if value.starts_with('[') && value.ends_with(']') {
        let inner = &value[1..value.len() - 1];
        inner
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else if !value.is_empty() {
        vec![value.to_string()]
    } else {
        Vec::new()
    }
}

/// Merge path-derived tags into extra frontmatter fields.
///
/// Path tags come first; existing user-added tags are appended if not already
/// present (preserving user tags while ensuring path tags are always included).
/// Does nothing when `path_tags` is empty.
pub fn merge_path_tags(extra: &mut Vec<String>, path_tags: &[String]) {
    if path_tags.is_empty() {
        return;
    }

    let existing: Vec<String> = extra
        .iter()
        .find(|l| l.trim_start().starts_with("tags:"))
        .map(|l| parse_tags_line(l))
        .unwrap_or_default();

    let mut merged = path_tags.to_vec();
    for tag in &existing {
        if !merged.contains(tag) {
            merged.push(tag.clone());
        }
    }

    let tags_line = format!("tags: [{}]", merged.join(", "));

    if let Some(pos) = extra
        .iter()
        .position(|l| l.trim_start().starts_with("tags:"))
    {
        extra[pos] = tags_line;
    } else {
        extra.push(tags_line);
    }
}

/// Extract extra (non-managed) YAML frontmatter fields from document content.
///
/// Returns raw YAML lines (e.g. `"reviewed: 2026-02-21"`) for all fields that
/// are NOT managed by factbase (`factbase_id`, `type`).  Returns an empty vec
/// when the content has no frontmatter block.
pub fn extract_extra_frontmatter(content: &str) -> Vec<String> {
    let mut lines = content.lines();

    let first = match lines.next() {
        Some(l) => l,
        None => return Vec::new(),
    };

    if first.trim() != "---" {
        return Vec::new();
    }

    let mut extra = Vec::new();
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        let key = line.split(':').next().unwrap_or("").trim();
        if !MANAGED_FIELDS.contains(&key) && !key.is_empty() {
            extra.push(line.to_string());
        }
    }
    extra
}

/// Build a document header (ID + title) according to format config.
///
/// `extra_fields` are raw YAML lines (e.g. `"reviewed: 2026-02-21"`) that will
/// be included in the frontmatter block.
///
/// Always generates YAML frontmatter: `---\nfactbase_id: id\ntype: ...\n---\n# Title\n\n`
pub fn build_document_header(
    id: &str,
    title: &str,
    doc_type: Option<&str>,
    format: &ResolvedFormat,
    extra_fields: &[String],
) -> String {
    // Both Comment and Frontmatter placement now use YAML frontmatter
    let _ = format.id_placement; // kept for API compatibility
    let mut fm = String::from("---\nfactbase_id: ");
    fm.push_str(id);
    fm.push('\n');
    if let Some(t) = doc_type {
        fm.push_str("type: ");
        fm.push_str(t);
        fm.push('\n');
    }
    for field in extra_fields {
        fm.push_str(field);
        fm.push('\n');
    }
    fm.push_str("---\n# ");
    fm.push_str(title);
    fm.push_str("\n\n");
    fm
}

/// Strip `.md` extension from a file path for wikilink targets.
pub fn wikilink_path(file_path: &str) -> &str {
    file_path.strip_suffix(".md").unwrap_or(file_path)
}

/// Update (or insert) the `type:` field in YAML frontmatter.
///
/// If the content has a frontmatter block, the `type:` field is updated or
/// added.  If there is no frontmatter, the content is returned unchanged.
pub fn update_frontmatter_type(content: &str, doc_type: &str) -> String {
    if !content.starts_with("---\n") {
        return content.to_string();
    }
    let fm_end = match content.find("\n---\n") {
        Some(pos) => pos,
        None => return content.to_string(),
    };

    let fm_text = &content[4..fm_end]; // skip leading "---\n"
    let after = &content[fm_end + 5..]; // skip "\n---\n"

    let type_line = format!("type: {doc_type}");
    let new_fm = if fm_text.lines().any(|l| l.trim_start().starts_with("type:")) {
        fm_text
            .lines()
            .map(|l| {
                if l.trim_start().starts_with("type:") {
                    type_line.as_str()
                } else {
                    l
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        let mut lines: Vec<&str> = fm_text.lines().collect();
        let insert_pos = lines
            .iter()
            .position(|l| l.trim_start().starts_with("factbase_id:"))
            .map(|i| i + 1)
            .unwrap_or(lines.len());
        lines.insert(insert_pos, &type_line);
        lines.join("\n")
    };

    format!("---\n{new_fm}\n---\n{after}")
}

/// Format a link reference according to link style.
///
/// - `Factbase`: `[[hex_id]]`
/// - `Wikilink`: `[[folder/filename|Display Title]]` when `file_path` is available,
///   otherwise `[[entity_name]]`
/// - `Markdown`: `[entity_name](hex_id)`
///
/// `file_path` is the document's relative path (e.g. `people/tim-leidig.md`).
pub fn format_link(
    id: &str,
    name: Option<&str>,
    file_path: Option<&str>,
    style: LinkStyle,
) -> String {
    match style {
        LinkStyle::Factbase => format!("[[{id}]]"),
        LinkStyle::Wikilink => {
            if let Some(fp) = file_path {
                let target = wikilink_path(fp);
                let display = name.unwrap_or(id);
                format!("[[{target}|{display}]]")
            } else {
                let display = name.unwrap_or(id);
                format!("[[{display}]]")
            }
        }
        LinkStyle::Markdown => {
            let display = name.unwrap_or(id);
            format!("[{display}]({id})")
        }
    }
}

/// Format a References: line with the given IDs and optional names/paths.
///
/// `id_names` is a slice of `(id, Option<name>, Option<file_path>)` tuples.
pub fn format_references_line(
    id_names: &[(&str, Option<&str>, Option<&str>)],
    style: LinkStyle,
) -> String {
    let links: Vec<String> = id_names
        .iter()
        .map(|(id, name, fp)| format_link(id, *name, *fp, style))
        .collect();
    format!("References: {}", links.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::format::IdPlacement;

    #[test]
    fn test_build_header_default() {
        let fmt = ResolvedFormat::default();
        let h = build_document_header("abc123", "Test Title", None, &fmt, &[]);
        assert_eq!(h, "---\nfactbase_id: abc123\n---\n# Test Title\n\n");
    }

    #[test]
    fn test_build_header_frontmatter_id() {
        let fmt = ResolvedFormat {
            id_placement: IdPlacement::Frontmatter,
            frontmatter: true,
            ..Default::default()
        };
        let h = build_document_header("abc123", "Test Title", Some("person"), &fmt, &[]);
        assert!(h.starts_with("---\n"));
        assert!(h.contains("factbase_id: abc123\n"));
        assert!(h.contains("type: person\n"));
        assert!(h.contains("---\n# Test Title\n\n"));
        assert!(!h.contains("<!-- factbase:"));
    }

    #[test]
    fn test_build_header_frontmatter_id_no_type() {
        let fmt = ResolvedFormat {
            id_placement: IdPlacement::Frontmatter,
            ..Default::default()
        };
        let h = build_document_header("abc123", "Test", None, &fmt, &[]);
        assert!(h.contains("factbase_id: abc123\n"));
        assert!(!h.contains("type:"));
    }

    #[test]
    fn test_build_header_comment_with_frontmatter() {
        // Comment placement now behaves same as Frontmatter
        let fmt = ResolvedFormat {
            id_placement: crate::models::format::IdPlacement::Comment,
            frontmatter: true,
            ..Default::default()
        };
        let h = build_document_header("abc123", "Test", Some("note"), &fmt, &[]);
        assert!(h.starts_with("---\nfactbase_id: abc123\n"));
        assert!(h.contains("type: note\n"));
        assert!(h.contains("---\n# Test\n\n"));
    }

    #[test]
    fn test_format_link_factbase() {
        assert_eq!(
            format_link("abc123", None, None, LinkStyle::Factbase),
            "[[abc123]]"
        );
        assert_eq!(
            format_link("abc123", Some("John"), None, LinkStyle::Factbase),
            "[[abc123]]"
        );
    }

    #[test]
    fn test_format_link_wikilink_no_path() {
        assert_eq!(
            format_link("abc123", Some("John Doe"), None, LinkStyle::Wikilink),
            "[[John Doe]]"
        );
        assert_eq!(
            format_link("abc123", None, None, LinkStyle::Wikilink),
            "[[abc123]]"
        );
    }

    #[test]
    fn test_format_link_wikilink_with_path() {
        assert_eq!(
            format_link(
                "abc123",
                Some("Tim Leidig"),
                Some("people/tim-leidig.md"),
                LinkStyle::Wikilink
            ),
            "[[people/tim-leidig|Tim Leidig]]"
        );
    }

    #[test]
    fn test_format_link_wikilink_path_disambiguates() {
        let person = format_link(
            "aaa111",
            Some("Joshua"),
            Some("people/joshua.md"),
            LinkStyle::Wikilink,
        );
        let book = format_link(
            "bbb222",
            Some("Joshua"),
            Some("books/joshua.md"),
            LinkStyle::Wikilink,
        );
        assert_eq!(person, "[[people/joshua|Joshua]]");
        assert_eq!(book, "[[books/joshua|Joshua]]");
        assert_ne!(person, book);
    }

    #[test]
    fn test_format_link_wikilink_root_file() {
        assert_eq!(
            format_link(
                "abc123",
                Some("Notes"),
                Some("notes.md"),
                LinkStyle::Wikilink
            ),
            "[[notes|Notes]]"
        );
    }

    #[test]
    fn test_format_link_markdown() {
        assert_eq!(
            format_link("abc123", Some("John"), None, LinkStyle::Markdown),
            "[John](abc123)"
        );
        assert_eq!(
            format_link("abc123", None, None, LinkStyle::Markdown),
            "[abc123](abc123)"
        );
    }

    #[test]
    fn test_format_references_line_factbase() {
        let ids = vec![("abc123", None, None), ("def456", None, None)];
        let line = format_references_line(&ids, LinkStyle::Factbase);
        assert_eq!(line, "References: [[abc123]] [[def456]]");
    }

    #[test]
    fn test_format_references_line_wikilink_with_paths() {
        let ids = vec![
            ("abc123", Some("John"), Some("people/john.md")),
            ("def456", Some("Acme Corp"), Some("companies/acme-corp.md")),
        ];
        let line = format_references_line(&ids, LinkStyle::Wikilink);
        assert_eq!(
            line,
            "References: [[people/john|John]] [[companies/acme-corp|Acme Corp]]"
        );
    }

    #[test]
    fn test_format_references_line_wikilink_no_paths() {
        let ids = vec![
            ("abc123", Some("John"), None),
            ("def456", Some("Acme Corp"), None),
        ];
        let line = format_references_line(&ids, LinkStyle::Wikilink);
        assert_eq!(line, "References: [[John]] [[Acme Corp]]");
    }

    #[test]
    fn test_format_references_line_markdown() {
        let ids = vec![("abc123", Some("John"), None)];
        let line = format_references_line(&ids, LinkStyle::Markdown);
        assert_eq!(line, "References: [John](abc123)");
    }

    #[test]
    fn test_wikilink_path_strips_md() {
        assert_eq!(wikilink_path("people/john.md"), "people/john");
        assert_eq!(wikilink_path("notes.md"), "notes");
        assert_eq!(wikilink_path("no-extension"), "no-extension");
    }

    // --- extract_extra_frontmatter tests ---

    #[test]
    fn test_extract_extra_frontmatter_no_frontmatter() {
        let content = "# Title\n\nBody";
        assert!(extract_extra_frontmatter(content).is_empty());
    }

    #[test]
    fn test_extract_extra_frontmatter_only_managed_fields() {
        let content = "---\nfactbase_id: abc123\ntype: person\n---\n# Title\n";
        assert!(extract_extra_frontmatter(content).is_empty());
    }

    #[test]
    fn test_extract_extra_frontmatter_preserves_extra_fields() {
        let content = "---\nfactbase_id: abc123\nreviewed: 2026-02-21\ntype: person\ntags: important\n---\n# Title\n";
        let extra = extract_extra_frontmatter(content);
        assert_eq!(extra, vec!["reviewed: 2026-02-21", "tags: important"]);
    }

    #[test]
    fn test_extract_extra_frontmatter_with_comment_header() {
        let content =
            "---\nfactbase_id: abc123\ntype: person\nreviewed: 2026-03-06\n---\n# Title\n";
        let extra = extract_extra_frontmatter(content);
        assert_eq!(extra, vec!["reviewed: 2026-03-06"]);
    }

    #[test]
    fn test_extract_extra_frontmatter_empty_content() {
        assert!(extract_extra_frontmatter("").is_empty());
    }

    // --- build_document_header with extra_fields tests ---

    #[test]
    fn test_build_header_frontmatter_with_extra_fields() {
        let fmt = ResolvedFormat {
            id_placement: IdPlacement::Frontmatter,
            frontmatter: true,
            ..Default::default()
        };
        let extra = vec!["reviewed: 2026-02-21".to_string()];
        let h = build_document_header("abc123", "Test", Some("person"), &fmt, &extra);
        assert!(h.contains("factbase_id: abc123\n"));
        assert!(h.contains("type: person\n"));
        assert!(h.contains("reviewed: 2026-02-21\n"));
        assert!(h.contains("---\n# Test\n\n"));
    }

    #[test]
    fn test_build_header_comment_frontmatter_with_extra_fields() {
        // Comment placement now behaves same as Frontmatter
        let fmt = ResolvedFormat {
            id_placement: crate::models::format::IdPlacement::Comment,
            frontmatter: true,
            ..Default::default()
        };
        let extra = vec![
            "reviewed: 2026-02-21".to_string(),
            "tags: important".to_string(),
        ];
        let h = build_document_header("abc123", "Test", Some("note"), &fmt, &extra);
        assert!(h.contains("type: note\n"));
        assert!(h.contains("reviewed: 2026-02-21\n"));
        assert!(h.contains("tags: important\n"));
    }

    #[test]
    fn test_build_header_default_includes_extra_fields() {
        let fmt = ResolvedFormat::default();
        let extra = vec!["reviewed: 2026-02-21".to_string()];
        let h = build_document_header("abc123", "Test", None, &fmt, &extra);
        assert!(h.starts_with("---\nfactbase_id: abc123\n"));
        assert!(h.contains("reviewed: 2026-02-21\n"));
    }

    // --- tags_from_path tests ---

    #[test]
    fn test_tags_from_path_deep() {
        use std::path::Path;
        let tags = tags_from_path(Path::new("customers/acme/people/alice-chen.md"));
        assert_eq!(tags, vec!["acme", "people"]);
    }

    #[test]
    fn test_tags_from_path_single_dir() {
        use std::path::Path;
        let tags = tags_from_path(Path::new("services/amazon-aurora.md"));
        assert_eq!(tags, vec!["services"]);
    }

    #[test]
    fn test_tags_from_path_root_file() {
        use std::path::Path;
        let tags = tags_from_path(Path::new("doc.md"));
        assert!(tags.is_empty());
    }

    #[test]
    fn test_tags_from_path_two_dirs() {
        use std::path::Path;
        let tags = tags_from_path(Path::new("a/b/file.md"));
        assert_eq!(tags, vec!["b"]);
    }

    // --- merge_path_tags tests ---

    #[test]
    fn test_merge_path_tags_no_existing() {
        let mut extra: Vec<String> = vec![];
        merge_path_tags(&mut extra, &["acme".into(), "people".into()]);
        assert_eq!(extra, vec!["tags: [acme, people]"]);
    }

    #[test]
    fn test_merge_path_tags_preserves_user_tags() {
        let mut extra = vec!["tags: [important]".to_string()];
        merge_path_tags(&mut extra, &["acme".into(), "people".into()]);
        assert_eq!(extra, vec!["tags: [acme, people, important]"]);
    }

    #[test]
    fn test_merge_path_tags_no_duplicates() {
        let mut extra = vec!["tags: [people, vip]".to_string()];
        merge_path_tags(&mut extra, &["acme".into(), "people".into()]);
        assert_eq!(extra, vec!["tags: [acme, people, vip]"]);
    }

    #[test]
    fn test_merge_path_tags_empty_path_tags_noop() {
        let mut extra = vec!["tags: [important]".to_string()];
        merge_path_tags(&mut extra, &[]);
        assert_eq!(extra, vec!["tags: [important]"]);
    }

    #[test]
    fn test_merge_path_tags_preserves_other_fields() {
        let mut extra = vec!["reviewed: 2026-01-01".to_string()];
        merge_path_tags(&mut extra, &["services".into()]);
        assert!(extra.contains(&"reviewed: 2026-01-01".to_string()));
        assert!(extra.contains(&"tags: [services]".to_string()));
    }

    // --- update_frontmatter_type tests ---

    #[test]
    fn test_update_frontmatter_type_updates_existing() {
        let content = "---\nfactbase_id: abc123\ntype: old_type\n---\n# Title\n";
        let result = update_frontmatter_type(content, "person");
        assert!(result.contains("type: person\n"));
        assert!(!result.contains("type: old_type"));
    }

    #[test]
    fn test_update_frontmatter_type_inserts_after_factbase_id() {
        let content = "---\nfactbase_id: abc123\nreviewed: 2026-01-01\n---\n# Title\n";
        let result = update_frontmatter_type(content, "person");
        assert!(result.contains("type: person\n"));
        // type should appear after factbase_id
        let id_pos = result.find("factbase_id:").unwrap();
        let type_pos = result.find("type:").unwrap();
        assert!(type_pos > id_pos);
    }

    #[test]
    fn test_update_frontmatter_type_no_frontmatter_unchanged() {
        let content = "# Title\n\nSome content without frontmatter\n";
        let result = update_frontmatter_type(content, "person");
        assert_eq!(result, content);
    }

    #[test]
    fn test_update_frontmatter_type_preserves_other_fields() {
        let content =
            "---\nfactbase_id: abc123\nreviewed: 2026-03-10\ntags: [people]\n---\n# Title\n";
        let result = update_frontmatter_type(content, "person");
        assert!(result.contains("reviewed: 2026-03-10\n"));
        assert!(result.contains("tags: [people]\n"));
        assert!(result.contains("type: person\n"));
    }
}
