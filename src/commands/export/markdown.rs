//! Markdown export format handler.

use factbase::{Database, Document};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Build YAML frontmatter for a document.
pub fn build_yaml_frontmatter(
    id: &str,
    title: &str,
    doc_type: Option<&str>,
    links_to: &[&str],
    linked_from: &[&str],
) -> String {
    let mut fm = String::from("---\n");
    fm.push_str(&format!("id: {}\n", id));
    fm.push_str(&format!("title: {}\n", title));
    if let Some(t) = doc_type {
        fm.push_str(&format!("type: {}\n", t));
    }
    if !links_to.is_empty() {
        fm.push_str(&format!("links_to: [{}]\n", links_to.join(", ")));
    }
    if !linked_from.is_empty() {
        fm.push_str(&format!("linked_from: [{}]\n", linked_from.join(", ")));
    }
    fm.push_str("---\n\n");
    fm
}

/// Build a single document's markdown output with optional frontmatter.
pub fn build_document_markdown(content: &str, frontmatter: Option<&str>) -> String {
    match frontmatter {
        Some(fm) => format!("{}{}", fm, content),
        None => content.to_string(),
    }
}

/// Build markdown content with optional YAML frontmatter metadata.
fn build_markdown_content(
    docs: &[Document],
    db: &Database,
    with_metadata: bool,
) -> anyhow::Result<String> {
    let mut content = String::new();
    for (i, doc) in docs.iter().enumerate() {
        if i > 0 {
            content.push_str("\n\n---\n\n");
        }
        let frontmatter = if with_metadata {
            let links_from = db.get_links_from(&doc.id)?;
            let links_to = db.get_links_to(&doc.id)?;
            let links_to_ids: Vec<&str> = links_from.iter().map(|l| l.target_id.as_str()).collect();
            let linked_from_ids: Vec<&str> =
                links_to.iter().map(|l| l.source_id.as_str()).collect();
            Some(build_yaml_frontmatter(
                &doc.id,
                &doc.title,
                doc.doc_type.as_deref(),
                &links_to_ids,
                &linked_from_ids,
            ))
        } else {
            None
        };
        content.push_str(&build_document_markdown(
            &doc.content,
            frontmatter.as_deref(),
        ));
    }
    Ok(content)
}

/// Export documents as markdown to stdout.
pub fn export_markdown_stdout(
    docs: &[Document],
    db: &Database,
    with_metadata: bool,
) -> anyhow::Result<()> {
    let content = build_markdown_content(docs, db, with_metadata)?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{}", content)?;
    Ok(())
}

/// Export documents as a single markdown file.
pub fn export_markdown_single_file(
    docs: &[Document],
    db: &Database,
    output: &Path,
    with_metadata: bool,
    compress: bool,
) -> anyhow::Result<()> {
    let content = build_markdown_content(docs, db, with_metadata)?;

    if compress {
        #[cfg(feature = "compression")]
        {
            let compressed = zstd::encode_all(content.as_bytes(), 3)?;
            fs::write(output, compressed)?;
        }
        #[cfg(not(feature = "compression"))]
        unreachable!("compression check at start should have caught this");
    } else {
        fs::write(output, content)?;
    }
    println!(
        "Exported {} documents to {}{}",
        docs.len(),
        output.display(),
        if compress { " (compressed)" } else { "" }
    );
    Ok(())
}

/// Export documents as individual markdown files to a directory.
pub fn export_markdown_directory(
    docs: &[Document],
    db: &Database,
    output: &Path,
    repo_path: &Path,
    with_metadata: bool,
) -> anyhow::Result<()> {
    fs::create_dir_all(output)?;

    for doc in docs {
        let rel_path = Path::new(&doc.file_path)
            .strip_prefix(repo_path)
            .unwrap_or(Path::new(&doc.file_path));
        let out_path = output.join(rel_path);

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&out_path, &doc.content)?;
        println!("Exported: {}", rel_path.display());
    }

    if with_metadata {
        let mut metadata: Vec<serde_json::Value> = Vec::with_capacity(docs.len());
        for doc in docs {
            let links_from = db.get_links_from(&doc.id)?;
            let links_to = db.get_links_to(&doc.id)?;
            metadata.push(serde_json::json!({
                "id": doc.id,
                "title": doc.title,
                "type": doc.doc_type,
                "file_path": doc.file_path,
                "links_to": links_from.iter().map(|l| &l.target_id).collect::<Vec<_>>(),
                "linked_from": links_to.iter().map(|l| &l.source_id).collect::<Vec<_>>(),
            }));
        }
        let meta_path = output.join("_metadata.json");
        fs::write(&meta_path, serde_json::to_string_pretty(&metadata)?)?;
        println!("Exported: _metadata.json");
    }

    println!(
        "\nExported {} documents to {}",
        docs.len(),
        output.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_yaml_frontmatter_basic() {
        let fm = build_yaml_frontmatter("abc123", "Test Title", Some("note"), &[], &[]);
        assert!(fm.starts_with("---\n"));
        assert!(fm.ends_with("---\n\n"));
        assert!(fm.contains("id: abc123\n"));
        assert!(fm.contains("title: Test Title\n"));
        assert!(fm.contains("type: note\n"));
        assert!(!fm.contains("links_to:"));
        assert!(!fm.contains("linked_from:"));
    }

    #[test]
    fn test_build_yaml_frontmatter_no_type() {
        let fm = build_yaml_frontmatter("abc123", "Test", None, &[], &[]);
        assert!(fm.contains("id: abc123\n"));
        assert!(fm.contains("title: Test\n"));
        assert!(!fm.contains("type:"));
    }

    #[test]
    fn test_build_yaml_frontmatter_with_links() {
        let fm = build_yaml_frontmatter(
            "abc123",
            "Test",
            Some("person"),
            &["def456", "ghi789"],
            &["xyz000"],
        );
        assert!(fm.contains("links_to: [def456, ghi789]\n"));
        assert!(fm.contains("linked_from: [xyz000]\n"));
    }

    #[test]
    fn test_build_document_markdown_with_frontmatter() {
        let fm = "---\nid: abc123\n---\n\n";
        let content = "# Title\n\nBody text";
        let result = build_document_markdown(content, Some(fm));
        assert_eq!(result, "---\nid: abc123\n---\n\n# Title\n\nBody text");
    }

    #[test]
    fn test_build_document_markdown_without_frontmatter() {
        let content = "# Title\n\nBody text";
        let result = build_document_markdown(content, None);
        assert_eq!(result, content);
    }

    #[test]
    fn test_build_yaml_frontmatter_special_characters() {
        // YAML frontmatter doesn't escape - values are plain text
        let fm = build_yaml_frontmatter("abc123", "Test: \"quoted\"", Some("note"), &[], &[]);
        assert!(fm.contains("title: Test: \"quoted\"\n"));
    }

    #[test]
    fn test_build_yaml_frontmatter_empty_title() {
        let fm = build_yaml_frontmatter("abc123", "", Some("note"), &[], &[]);
        assert!(fm.contains("id: abc123\n"));
        assert!(fm.contains("title: \n"));
    }

    #[test]
    fn test_build_document_markdown_empty_content() {
        let result = build_document_markdown("", None);
        assert_eq!(result, "");

        let fm = "---\nid: abc123\n---\n\n";
        let result = build_document_markdown("", Some(fm));
        assert_eq!(result, fm);
    }

    #[test]
    fn test_build_yaml_frontmatter_single_link() {
        let fm = build_yaml_frontmatter("abc123", "Test", None, &["def456"], &[]);
        assert!(fm.contains("links_to: [def456]\n"));
        assert!(!fm.contains("linked_from:"));
    }
}
