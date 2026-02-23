//! Compressed archive export handler.

use factbase::{Database, Document, ProgressReporter};
use std::fs;
use std::path::Path;

/// Export documents as a compressed tar.zst archive.
#[cfg(feature = "compression")]
pub fn export_archive(
    docs: &[Document],
    db: &Database,
    output: &Path,
    repo_path: &Path,
    with_metadata: bool,
    progress: &ProgressReporter,
) -> anyhow::Result<()> {
    let file = fs::File::create(output)?;
    let encoder = zstd::Encoder::new(file, 3)?;
    let mut archive = tar::Builder::new(encoder);

    let total = docs.len();
    for (i, doc) in docs.iter().enumerate() {
        progress.report(i + 1, total, &doc.title);
        let rel_path = Path::new(&doc.file_path)
            .strip_prefix(repo_path)
            .unwrap_or(Path::new(&doc.file_path));

        let mut header = tar::Header::new_gnu();
        header.set_size(doc.content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append_data(&mut header, rel_path, doc.content.as_bytes())?;
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
        let meta_json = serde_json::to_string_pretty(&metadata)?;
        let mut header = tar::Header::new_gnu();
        header.set_size(meta_json.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append_data(&mut header, "_metadata.json", meta_json.as_bytes())?;
    }

    let encoder = archive.into_inner()?;
    encoder.finish()?;
    println!(
        "Exported {} documents to {} (compressed archive)",
        docs.len(),
        output.display()
    );
    Ok(())
}
