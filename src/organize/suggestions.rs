use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::database::suggestions::OrganizationSuggestion;
use crate::error::FactbaseError;
use crate::processor::format::wikilink_path;
use crate::Database;

/// Result of executing a batch of organization suggestions.
#[derive(Debug, Default)]
pub struct SuggestionExecutionResult {
    pub moves: Vec<MoveAction>,
    pub renames: Vec<RenameAction>,
    pub title_changes: Vec<TitleChangeAction>,
    pub errors: Vec<String>,
}

#[derive(Debug)]
pub struct MoveAction {
    pub doc_id: String,
    pub old_path: String,
    pub new_path: String,
    pub links_updated: usize,
}

#[derive(Debug)]
pub struct RenameAction {
    pub doc_id: String,
    pub old_path: String,
    pub new_path: String,
    pub links_updated: usize,
}

#[derive(Debug)]
pub struct TitleChangeAction {
    pub doc_id: String,
    pub old_title: String,
    pub new_title: String,
    pub links_updated: usize,
}

/// Execute all pending organization suggestions for a repo.
/// Returns a summary of what was done. On any filesystem error, the
/// individual suggestion is recorded as an error and processing continues.
pub fn execute_suggestions(
    db: &Database,
    repo_id: Option<&str>,
    dry_run: bool,
) -> Result<SuggestionExecutionResult, FactbaseError> {
    let suggestions = db.list_suggestions(repo_id)?;
    if suggestions.is_empty() {
        return Ok(SuggestionExecutionResult::default());
    }

    // Group suggestions by doc_id for batch processing
    let mut by_doc: HashMap<String, Vec<OrganizationSuggestion>> = HashMap::new();
    for s in suggestions {
        by_doc.entry(s.doc_id.clone()).or_default().push(s);
    }

    let mut result = SuggestionExecutionResult::default();

    for (doc_id, doc_suggestions) in &by_doc {
        let doc = match db.get_document(doc_id)? {
            Some(d) => d,
            None => {
                // Document was deleted since suggestion was created
                for s in doc_suggestions {
                    if !dry_run {
                        db.delete_suggestion(s.id)?;
                    }
                }
                continue;
            }
        };
        let repo = db.require_repository(&doc.repo_id)?;

        for s in doc_suggestions {
            match s.suggestion_type.as_str() {
                "move" => {
                    match execute_move_suggestion(db, &doc, &repo.path, &s.suggested_value, dry_run)
                    {
                        Ok(action) => result.moves.push(action),
                        Err(e) => result.errors.push(format!("move {}: {e}", doc_id)),
                    }
                }
                "rename" => {
                    match execute_rename_suggestion(
                        db,
                        &doc,
                        &repo.path,
                        &s.suggested_value,
                        dry_run,
                    ) {
                        Ok(action) => result.renames.push(action),
                        Err(e) => result.errors.push(format!("rename {}: {e}", doc_id)),
                    }
                }
                "title" => {
                    match execute_title_suggestion(
                        db,
                        &doc,
                        &repo.path,
                        &s.suggested_value,
                        dry_run,
                    ) {
                        Ok(action) => result.title_changes.push(action),
                        Err(e) => result.errors.push(format!("title {}: {e}", doc_id)),
                    }
                }
                other => {
                    result
                        .errors
                        .push(format!("unknown suggestion type '{other}' for {doc_id}"));
                }
            }
            if !dry_run {
                db.delete_suggestion(s.id)?;
            }
        }
    }

    Ok(result)
}

fn execute_move_suggestion(
    db: &Database,
    doc: &crate::models::Document,
    repo_path: &Path,
    target_dir: &str,
    dry_run: bool,
) -> Result<MoveAction, FactbaseError> {
    let old_path = &doc.file_path;
    let filename = Path::new(old_path)
        .file_name()
        .ok_or_else(|| FactbaseError::internal(format!("Invalid file path: {old_path}")))?;
    let dest = target_dir.trim_end_matches('/');
    let new_path = format!("{}/{}", dest, filename.to_string_lossy());

    let links_updated = if dry_run {
        count_wikilink_references(db, &doc.repo_id, old_path)
    } else {
        let old_abs = repo_path.join(old_path);
        let new_abs = repo_path.join(&new_path);
        if !old_abs.exists() {
            return Err(FactbaseError::not_found(format!(
                "Source file not found: {}",
                old_abs.display()
            )));
        }
        if new_abs.exists() {
            // Auto-merge: destination already exists, merge source into it
            return execute_move_with_merge(db, doc, repo_path, &new_path, &new_abs);
        }
        if let Some(parent) = new_abs.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&old_abs, &new_abs)?;

        let count = cascade_wikilink_path_change(db, &doc.repo_id, repo_path, old_path, &new_path)?;

        // Update DB
        let mut updated = doc.clone();
        updated.file_path = new_path.clone();
        updated.doc_type = Some(
            crate::processor::DocumentProcessor::new()
                .derive_type(Path::new(&new_path), Path::new("")),
        );
        db.upsert_document(&updated)?;
        count
    };

    Ok(MoveAction {
        doc_id: doc.id.clone(),
        old_path: old_path.clone(),
        new_path,
        links_updated,
    })
}

fn execute_rename_suggestion(
    db: &Database,
    doc: &crate::models::Document,
    repo_path: &Path,
    new_filename: &str,
    dry_run: bool,
) -> Result<RenameAction, FactbaseError> {
    let old_path = &doc.file_path;
    let parent = Path::new(old_path).parent().unwrap_or(Path::new(""));
    let new_path = if parent == Path::new("") {
        new_filename.to_string()
    } else {
        format!("{}/{}", parent.display(), new_filename)
    };

    let links_updated = if dry_run {
        count_wikilink_references(db, &doc.repo_id, old_path)
    } else {
        let old_abs = repo_path.join(old_path);
        let new_abs = repo_path.join(&new_path);
        if !old_abs.exists() {
            return Err(FactbaseError::not_found(format!(
                "Source file not found: {}",
                old_abs.display()
            )));
        }
        if new_abs.exists() {
            return Err(FactbaseError::internal(format!(
                "Destination already exists: {}",
                new_abs.display()
            )));
        }
        fs::rename(&old_abs, &new_abs)?;

        let count = cascade_wikilink_path_change(db, &doc.repo_id, repo_path, old_path, &new_path)?;

        let mut updated = doc.clone();
        updated.file_path = new_path.clone();
        db.upsert_document(&updated)?;
        count
    };

    Ok(RenameAction {
        doc_id: doc.id.clone(),
        old_path: old_path.clone(),
        new_path,
        links_updated,
    })
}

fn execute_title_suggestion(
    db: &Database,
    doc: &crate::models::Document,
    repo_path: &Path,
    new_title: &str,
    dry_run: bool,
) -> Result<TitleChangeAction, FactbaseError> {
    let old_title = &doc.title;
    let old_path = &doc.file_path;
    let wikilink_target = wikilink_path(old_path);

    let links_updated = if dry_run {
        count_wikilink_title_references(db, &doc.repo_id, wikilink_target, old_title)
    } else {
        let count = cascade_wikilink_title_change(
            db,
            &doc.repo_id,
            repo_path,
            wikilink_target,
            old_title,
            new_title,
        )?;
        db.update_document_title(&doc.id, new_title)?;
        count
    };

    Ok(TitleChangeAction {
        doc_id: doc.id.clone(),
        old_title: old_title.clone(),
        new_title: new_title.to_string(),
        links_updated,
    })
}

// --- Auto-merge helper for move conflicts ---

/// When a move destination already exists, merge the source into the existing file.
fn execute_move_with_merge(
    db: &Database,
    doc: &crate::models::Document,
    repo_path: &Path,
    new_path: &str,
    new_abs: &Path,
) -> Result<MoveAction, FactbaseError> {
    let old_path = &doc.file_path;
    let old_abs = repo_path.join(old_path);

    // Find the target document by file_path
    let target_doc = db
        .list_documents(None, Some(&doc.repo_id), None, 100_000)?
        .into_iter()
        .find(|d| d.file_path == new_path)
        .ok_or_else(|| {
            FactbaseError::internal(format!(
                "Destination file exists but no matching document in DB: {}",
                new_abs.display()
            ))
        })?;

    // Read both files
    let target_content = fs::read_to_string(new_abs)?;
    let source_body = crate::mcp::tools::document::strip_factbase_header(&doc.content);

    // Append source body to target
    let mut merged = target_content;
    if !merged.ends_with('\n') {
        merged.push('\n');
    }
    merged.push_str(&format!(
        "\n## Merged from {}\n\n{}\n",
        doc.title, source_body
    ));
    fs::write(new_abs, &merged)?;

    // Redirect links from source to target
    let links_updated = crate::organize::redirect_links(db, &doc.id, &target_doc.id, repo_path)?;

    // Delete source file and mark deleted
    if old_abs.exists() {
        fs::remove_file(&old_abs)?;
    }
    db.mark_deleted(&doc.id)?;

    Ok(MoveAction {
        doc_id: doc.id.clone(),
        old_path: old_path.clone(),
        new_path: format!("{} (merged into {})", new_path, target_doc.id),
        links_updated,
    })
}

// --- Link cascade helpers ---

/// Replace wikilink path references across all repo documents.
/// Changes `[[old_wikipath|Display]]` to `[[new_wikipath|Display]]`.
fn cascade_wikilink_path_change(
    db: &Database,
    repo_id: &str,
    repo_path: &Path,
    old_file_path: &str,
    new_file_path: &str,
) -> Result<usize, FactbaseError> {
    let old_wikipath = wikilink_path(old_file_path);
    let new_wikipath = wikilink_path(new_file_path);
    if old_wikipath == new_wikipath {
        return Ok(0);
    }

    let pattern = format!("[[{old_wikipath}");
    let docs = db.get_documents_for_repo(repo_id)?;
    let mut total = 0;

    for d in docs.values() {
        if !d.content.contains(&pattern) {
            continue;
        }
        let new_content = replace_wikilink_path(&d.content, old_wikipath, new_wikipath);
        if new_content == d.content {
            continue;
        }
        let replacements = d.content.matches(&pattern).count();
        total += replacements;

        let file = repo_path.join(&d.file_path);
        if file.exists() {
            fs::write(&file, &new_content)?;
        }
        let new_hash = crate::processor::content_hash(&new_content);
        db.update_document_content(&d.id, &new_content, &new_hash)?;
    }

    Ok(total)
}

/// Replace the display portion of wikilinks that target a specific path.
/// Changes `[[path|Old Title]]` to `[[path|New Title]]`.
fn cascade_wikilink_title_change(
    db: &Database,
    repo_id: &str,
    repo_path: &Path,
    wikilink_target: &str,
    old_title: &str,
    new_title: &str,
) -> Result<usize, FactbaseError> {
    let old_pattern = format!("[[{}|{}]]", wikilink_target, old_title);
    let new_pattern = format!("[[{}|{}]]", wikilink_target, new_title);
    let docs = db.get_documents_for_repo(repo_id)?;
    let mut total = 0;

    for d in docs.values() {
        if !d.content.contains(&old_pattern) {
            continue;
        }
        let new_content = d.content.replace(&old_pattern, &new_pattern);
        let replacements = d.content.matches(&old_pattern).count();
        total += replacements;

        let file = repo_path.join(&d.file_path);
        if file.exists() {
            fs::write(&file, &new_content)?;
        }
        let new_hash = crate::processor::content_hash(&new_content);
        db.update_document_content(&d.id, &new_content, &new_hash)?;
    }

    Ok(total)
}

/// Replace `[[old_path|...]]` and `[[old_path]]` with `[[new_path|...]]` / `[[new_path]]`.
fn replace_wikilink_path(content: &str, old_path: &str, new_path: &str) -> String {
    // Replace [[old_path|display]] → [[new_path|display]]
    // and [[old_path]] → [[new_path]]
    let mut result = content.replace(&format!("[[{old_path}|"), &format!("[[{new_path}|"));
    result = result.replace(&format!("[[{old_path}]]"), &format!("[[{new_path}]]"));
    result
}

fn count_wikilink_references(db: &Database, repo_id: &str, file_path: &str) -> usize {
    let wp = wikilink_path(file_path);
    let pattern = format!("[[{wp}");
    db.get_documents_for_repo(repo_id)
        .unwrap_or_default()
        .values()
        .map(|d| d.content.matches(&pattern).count())
        .sum()
}

fn count_wikilink_title_references(
    db: &Database,
    repo_id: &str,
    wikilink_target: &str,
    title: &str,
) -> usize {
    let pattern = format!("[[{}|{}]]", wikilink_target, title);
    db.get_documents_for_repo(repo_id)
        .unwrap_or_default()
        .values()
        .map(|d| d.content.matches(&pattern).count())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_wikilink_path_with_display() {
        let content = "See [[people/alice|Alice Smith]] for details.";
        let result = replace_wikilink_path(content, "people/alice", "team/alice");
        assert_eq!(result, "See [[team/alice|Alice Smith]] for details.");
    }

    #[test]
    fn test_replace_wikilink_path_bare() {
        let content = "See [[people/alice]] for details.";
        let result = replace_wikilink_path(content, "people/alice", "team/alice");
        assert_eq!(result, "See [[team/alice]] for details.");
    }

    #[test]
    fn test_replace_wikilink_path_multiple() {
        let content = "[[old/a|A]] and [[old/a|B]] and [[old/b|C]]";
        let result = replace_wikilink_path(content, "old/a", "new/a");
        assert_eq!(result, "[[new/a|A]] and [[new/a|B]] and [[old/b|C]]");
    }

    #[test]
    fn test_replace_wikilink_path_no_match() {
        let content = "No links here.";
        let result = replace_wikilink_path(content, "old/path", "new/path");
        assert_eq!(result, "No links here.");
    }

    #[test]
    fn test_execute_move_with_merge_when_destination_exists() {
        let (db, tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", tmp.path());

        // Create source file
        let src_dir = tmp.path().join("old");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(
            src_dir.join("entity.md"),
            "---\nfactbase_id: src001\n---\n# Entity\n\n- Source fact",
        )
        .unwrap();

        // Create destination file (already exists at target location)
        let dst_dir = tmp.path().join("new");
        fs::create_dir_all(&dst_dir).unwrap();
        fs::write(
            dst_dir.join("entity.md"),
            "---\nfactbase_id: dst001\n---\n# Entity\n\n- Existing fact",
        )
        .unwrap();

        let mut src = crate::models::Document::test_default();
        src.id = "src001".to_string();
        src.title = "Entity".to_string();
        src.content = "---\nfactbase_id: src001\n---\n# Entity\n\n- Source fact".to_string();
        src.file_path = "old/entity.md".to_string();
        src.repo_id = "test".to_string();
        db.upsert_document(&src).unwrap();

        let mut dst = crate::models::Document::test_default();
        dst.id = "dst001".to_string();
        dst.title = "Entity".to_string();
        dst.content = "---\nfactbase_id: dst001\n---\n# Entity\n\n- Existing fact".to_string();
        dst.file_path = "new/entity.md".to_string();
        dst.repo_id = "test".to_string();
        db.upsert_document(&dst).unwrap();

        // Insert move suggestion that would conflict
        db.insert_suggestion("src001", "move", "new/", "update")
            .unwrap();

        // Execute suggestions
        let result = execute_suggestions(&db, Some("test"), false).unwrap();

        // Should auto-merge instead of erroring
        assert_eq!(result.moves.len(), 1);
        assert!(result.moves[0].new_path.contains("merged into"));
        assert!(result.errors.is_empty());

        // Source file should be deleted
        assert!(!src_dir.join("entity.md").exists());

        // Destination file should have merged content
        let content = fs::read_to_string(dst_dir.join("entity.md")).unwrap();
        assert!(content.contains("Existing fact"));
        assert!(content.contains("Source fact"));
        assert!(content.contains("Merged from Entity"));

        // Suggestion should be consumed
        assert!(db.list_suggestions(None).unwrap().is_empty());
    }
}
