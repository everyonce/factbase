//! Document format layer — applies format config when writing documents.
//!
//! Handles frontmatter generation, ID placement, and link formatting
//! based on the resolved format configuration.

use crate::models::format::{IdPlacement, LinkStyle, ResolvedFormat};

/// Build a document header (ID + title) according to format config.
///
/// For `IdPlacement::Comment`: `<!-- factbase:id -->\n# Title\n\n`
/// For `IdPlacement::Frontmatter`: `---\nfactbase_id: id\ntype: ...\n---\n# Title\n\n`
pub fn build_document_header(
    id: &str,
    title: &str,
    doc_type: Option<&str>,
    format: &ResolvedFormat,
) -> String {
    match format.id_placement {
        IdPlacement::Comment => {
            if format.frontmatter {
                // Frontmatter without ID (ID stays in comment)
                let mut fm = String::from("<!-- factbase:");
                fm.push_str(id);
                fm.push_str(" -->\n---\n");
                if let Some(t) = doc_type {
                    fm.push_str("type: ");
                    fm.push_str(t);
                    fm.push('\n');
                }
                fm.push_str("---\n# ");
                fm.push_str(title);
                fm.push_str("\n\n");
                fm
            } else {
                format!("<!-- factbase:{id} -->\n# {title}\n\n")
            }
        }
        IdPlacement::Frontmatter => {
            let mut fm = String::from("---\nfactbase_id: ");
            fm.push_str(id);
            fm.push('\n');
            if let Some(t) = doc_type {
                fm.push_str("type: ");
                fm.push_str(t);
                fm.push('\n');
            }
            fm.push_str("---\n# ");
            fm.push_str(title);
            fm.push_str("\n\n");
            fm
        }
    }
}

/// Strip `.md` extension from a file path for wikilink targets.
pub fn wikilink_path(file_path: &str) -> &str {
    file_path.strip_suffix(".md").unwrap_or(file_path)
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

    #[test]
    fn test_build_header_default() {
        let fmt = ResolvedFormat::default();
        let h = build_document_header("abc123", "Test Title", None, &fmt);
        assert_eq!(h, "<!-- factbase:abc123 -->\n# Test Title\n\n");
    }

    #[test]
    fn test_build_header_frontmatter_id() {
        let fmt = ResolvedFormat {
            id_placement: IdPlacement::Frontmatter,
            frontmatter: true,
            ..Default::default()
        };
        let h = build_document_header("abc123", "Test Title", Some("person"), &fmt);
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
        let h = build_document_header("abc123", "Test", None, &fmt);
        assert!(h.contains("factbase_id: abc123\n"));
        assert!(!h.contains("type:"));
    }

    #[test]
    fn test_build_header_comment_with_frontmatter() {
        let fmt = ResolvedFormat {
            id_placement: IdPlacement::Comment,
            frontmatter: true,
            ..Default::default()
        };
        let h = build_document_header("abc123", "Test", Some("note"), &fmt);
        assert!(h.starts_with("<!-- factbase:abc123 -->\n---\n"));
        assert!(h.contains("type: note\n"));
        assert!(h.contains("---\n# Test\n\n"));
    }

    #[test]
    fn test_format_link_factbase() {
        assert_eq!(format_link("abc123", None, None, LinkStyle::Factbase), "[[abc123]]");
        assert_eq!(format_link("abc123", Some("John"), None, LinkStyle::Factbase), "[[abc123]]");
    }

    #[test]
    fn test_format_link_wikilink_no_path() {
        assert_eq!(format_link("abc123", Some("John Doe"), None, LinkStyle::Wikilink), "[[John Doe]]");
        assert_eq!(format_link("abc123", None, None, LinkStyle::Wikilink), "[[abc123]]");
    }

    #[test]
    fn test_format_link_wikilink_with_path() {
        assert_eq!(
            format_link("abc123", Some("Tim Leidig"), Some("people/tim-leidig.md"), LinkStyle::Wikilink),
            "[[people/tim-leidig|Tim Leidig]]"
        );
    }

    #[test]
    fn test_format_link_wikilink_path_disambiguates() {
        let person = format_link("aaa111", Some("Joshua"), Some("people/joshua.md"), LinkStyle::Wikilink);
        let book = format_link("bbb222", Some("Joshua"), Some("books/joshua.md"), LinkStyle::Wikilink);
        assert_eq!(person, "[[people/joshua|Joshua]]");
        assert_eq!(book, "[[books/joshua|Joshua]]");
        assert_ne!(person, book);
    }

    #[test]
    fn test_format_link_wikilink_root_file() {
        assert_eq!(
            format_link("abc123", Some("Notes"), Some("notes.md"), LinkStyle::Wikilink),
            "[[notes|Notes]]"
        );
    }

    #[test]
    fn test_format_link_markdown() {
        assert_eq!(format_link("abc123", Some("John"), None, LinkStyle::Markdown), "[John](abc123)");
        assert_eq!(format_link("abc123", None, None, LinkStyle::Markdown), "[abc123](abc123)");
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
        assert_eq!(line, "References: [[people/john|John]] [[companies/acme-corp|Acme Corp]]");
    }

    #[test]
    fn test_format_references_line_wikilink_no_paths() {
        let ids = vec![("abc123", Some("John"), None), ("def456", Some("Acme Corp"), None)];
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
}
