//! JSON export format handler.

use factbase::{Database, Document};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Build a JSON value for a single document export.
pub fn build_export_json(
    id: &str,
    title: &str,
    doc_type: Option<&str>,
    content: &str,
    links_to: &[String],
    linked_from: &[String],
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "title": title,
        "type": doc_type,
        "content": content,
        "links_to": links_to,
        "linked_from": linked_from,
    })
}

/// Export documents as JSON format.
pub fn export_json(
    docs: &[Document],
    db: &Database,
    output: &Path,
    compress: bool,
    to_stdout: bool,
) -> anyhow::Result<()> {
    let mut export_data: Vec<serde_json::Value> = Vec::with_capacity(docs.len());
    for doc in docs {
        let links_from = db.get_links_from(&doc.id)?;
        let links_to = db.get_links_to(&doc.id)?;
        export_data.push(build_export_json(
            &doc.id,
            &doc.title,
            doc.doc_type.as_deref(),
            &doc.content,
            &links_from
                .iter()
                .map(|l| l.target_id.clone())
                .collect::<Vec<_>>(),
            &links_to
                .iter()
                .map(|l| l.source_id.clone())
                .collect::<Vec<_>>(),
        ));
    }
    let json_content = serde_json::to_string_pretty(&export_data)?;

    if to_stdout {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{}", json_content)?;
    } else if compress {
        #[cfg(feature = "compression")]
        {
            let compressed = zstd::encode_all(json_content.as_bytes(), 3)?;
            fs::write(output, compressed)?;
            println!(
                "Exported {} documents to {} (compressed)",
                docs.len(),
                output.display()
            );
        }
        #[cfg(not(feature = "compression"))]
        unreachable!("compression check at start should have caught this");
    } else {
        fs::write(output, json_content)?;
        println!("Exported {} documents to {}", docs.len(), output.display());
    }
    Ok(())
}

/// Export documents as YAML format.
pub fn export_yaml(
    docs: &[Document],
    db: &Database,
    output: &Path,
    to_stdout: bool,
) -> anyhow::Result<()> {
    use serde::Serialize;

    #[derive(Serialize)]
    struct ExportDoc {
        id: String,
        title: String,
        #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
        doc_type: Option<String>,
        content: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        links_to: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        linked_from: Vec<String>,
    }

    #[derive(Serialize)]
    struct ExportWrapper {
        documents: Vec<ExportDoc>,
    }

    let mut export_docs = Vec::with_capacity(docs.len());
    for doc in docs {
        let links_from = db.get_links_from(&doc.id)?;
        let links_to = db.get_links_to(&doc.id)?;
        export_docs.push(ExportDoc {
            id: doc.id.clone(),
            title: doc.title.clone(),
            doc_type: doc.doc_type.clone(),
            content: doc.content.clone(),
            links_to: links_from.iter().map(|l| l.target_id.clone()).collect(),
            linked_from: links_to.iter().map(|l| l.source_id.clone()).collect(),
        });
    }

    let wrapper = ExportWrapper {
        documents: export_docs,
    };
    let yaml_content = serde_yaml_ng::to_string(&wrapper)?;

    if to_stdout {
        let mut stdout = io::stdout().lock();
        write!(stdout, "{}", yaml_content)?;
    } else {
        fs::write(output, &yaml_content)?;
        println!("Exported {} documents to {}", docs.len(), output.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_export_json_basic() {
        let json = build_export_json(
            "abc123",
            "Test Document",
            Some("note"),
            "Content here",
            &[],
            &[],
        );
        assert_eq!(json["id"], "abc123");
        assert_eq!(json["title"], "Test Document");
        assert_eq!(json["type"], "note");
        assert_eq!(json["content"], "Content here");
        assert!(json["links_to"].as_array().unwrap().is_empty());
        assert!(json["linked_from"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_build_export_json_with_links() {
        let json = build_export_json(
            "abc123",
            "Test",
            Some("person"),
            "Content",
            &["def456".to_string(), "ghi789".to_string()],
            &["xyz000".to_string()],
        );
        let links_to = json["links_to"].as_array().unwrap();
        assert_eq!(links_to.len(), 2);
        assert_eq!(links_to[0], "def456");
        assert_eq!(links_to[1], "ghi789");
        let linked_from = json["linked_from"].as_array().unwrap();
        assert_eq!(linked_from.len(), 1);
        assert_eq!(linked_from[0], "xyz000");
    }

    #[test]
    fn test_build_export_json_null_type() {
        let json = build_export_json("abc123", "Test", None, "Content", &[], &[]);
        assert!(json["type"].is_null());
    }

    #[test]
    fn test_build_export_json_special_characters() {
        let json = build_export_json(
            "abc123",
            "Test \"quoted\" title",
            Some("note"),
            "Content with\nnewlines\tand\ttabs",
            &[],
            &[],
        );
        assert_eq!(json["title"], "Test \"quoted\" title");
        assert_eq!(json["content"], "Content with\nnewlines\tand\ttabs");
    }

    #[test]
    fn test_build_export_json_serialization_format() {
        let json = build_export_json("a1b2c3", "Title", Some("doc"), "Body", &[], &[]);
        let serialized = serde_json::to_string(&json).unwrap();
        // Verify all expected fields are present in serialized output
        assert!(serialized.contains("\"id\":\"a1b2c3\""));
        assert!(serialized.contains("\"title\":\"Title\""));
        assert!(serialized.contains("\"type\":\"doc\""));
        assert!(serialized.contains("\"content\":\"Body\""));
        assert!(serialized.contains("\"links_to\":[]"));
        assert!(serialized.contains("\"linked_from\":[]"));
    }
}
