//! Folder placement check via link graph analysis.
//!
//! Generates `@q[ambiguous]` review questions when a document has more outgoing
//! entity links to a different container than to its own container.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::{Document, QuestionType, ReviewQuestion};
use std::collections::HashMap;
use std::path::Path;

/// Check folder placement for a set of documents and return questions keyed by doc ID.
///
/// A "container document" follows the convention: filename stem == parent folder name.
/// For each non-container doc, counts outgoing links to entities in each container.
/// If another container has strictly more links than the current one, generates a question.
pub fn check_folder_placement(
    docs: &[Document],
    db: &Database,
) -> Result<HashMap<String, Vec<ReviewQuestion>>, FactbaseError> {
    // Identify container documents: filename stem == parent folder name
    // container_folder_path → (doc_id, title)
    let mut container_by_folder: HashMap<String, (String, String)> = HashMap::new();

    for doc in docs {
        let path = Path::new(&doc.file_path);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let parent = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if !stem.is_empty() && stem.eq_ignore_ascii_case(parent) {
            if let Some(parent_path) = path.parent() {
                let folder_key = parent_path.to_string_lossy().to_string();
                container_by_folder.insert(folder_key, (doc.id.clone(), doc.title.clone()));
            }
        }
    }

    if container_by_folder.is_empty() {
        return Ok(HashMap::new());
    }

    // Build doc_id → container_folder for ALL docs (not just containers)
    let mut doc_container: HashMap<&str, String> = HashMap::new();
    for doc in docs {
        let path = Path::new(&doc.file_path);
        if let Some(c) = find_ancestor_container(path, &container_by_folder) {
            doc_container.insert(&doc.id, c);
        }
    }

    // Container doc IDs (to skip them from analysis)
    let container_doc_ids: std::collections::HashSet<&str> = container_by_folder
        .values()
        .map(|(id, _)| id.as_str())
        .collect();

    let doc_ids: Vec<&str> = docs.iter().map(|d| d.id.as_str()).collect();
    let all_links = db.get_links_for_documents(&doc_ids)?;

    let mut questions: HashMap<String, Vec<ReviewQuestion>> = HashMap::new();

    for doc in docs {
        if container_doc_ids.contains(doc.id.as_str()) {
            continue;
        }

        let Some(my_container) = doc_container.get(doc.id.as_str()) else {
            continue;
        };

        let (outgoing, _) = match all_links.get(&doc.id) {
            Some(links) => links,
            None => continue,
        };

        // Count outgoing links by which container the TARGET lives under
        let mut counts: HashMap<&str, usize> = HashMap::new();
        let mut total = 0usize;
        for link in outgoing {
            if let Some(target_container) = doc_container.get(link.target_id.as_str()) {
                *counts.entry(target_container.as_str()).or_default() += 1;
                total += 1;
            }
        }

        if total == 0 {
            continue;
        }

        let current_count = counts.get(my_container.as_str()).copied().unwrap_or(0);

        // Find the container with the most links (other than current)
        let best_other = counts
            .iter()
            .filter(|(&folder, _)| folder != my_container.as_str())
            .max_by_key(|(_, &count)| count);

        if let Some((&other_folder, &other_count)) = best_other {
            if other_count > current_count {
                let current_title = &container_by_folder[my_container].1;
                let other_title = &container_by_folder[other_folder].1;
                questions.entry(doc.id.clone()).or_default().push(
                    ReviewQuestion::new(
                        QuestionType::Ambiguous,
                        None,
                        format!(
                            "Filed under '{}' but {} of {} entity links point to '{}'. Is this document filed correctly?",
                            current_title, other_count, total, other_title
                        ),
                    ),
                );
            }
        }
    }

    Ok(questions)
}

/// Walk up the path to find the nearest ancestor that is a known container folder.
fn find_ancestor_container(
    path: &Path,
    containers: &HashMap<String, (String, String)>,
) -> Option<String> {
    let mut current = path.parent();
    while let Some(dir) = current {
        let key = dir.to_string_lossy().to_string();
        if containers.contains_key(&key) {
            return Some(key);
        }
        current = dir.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link_detection::DetectedLink;
    use crate::models::Repository;
    use tempfile::TempDir;

    fn test_db() -> (Database, TempDir) {
        let tmp = TempDir::new().unwrap();
        let db = Database::new(&tmp.path().join("test.db")).unwrap();
        db.upsert_repository(&Repository {
            id: "test-repo".to_string(),
            name: "Test".to_string(),
            path: std::path::PathBuf::from("/tmp/test"),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_lint_at: None,
        })
        .unwrap();
        (db, tmp)
    }

    fn make_doc(id: &str, title: &str, file_path: &str) -> Document {
        Document {
            id: id.to_string(),
            title: title.to_string(),
            file_path: file_path.to_string(),
            ..Document::test_default()
        }
    }

    fn set_links(db: &Database, source_id: &str, targets: &[(&str, &str)]) {
        let links: Vec<DetectedLink> = targets
            .iter()
            .map(|(tid, ctx)| DetectedLink {
                target_id: tid.to_string(),
                target_title: String::new(),
                mention_text: String::new(),
                context: ctx.to_string(),
            })
            .collect();
        db.update_links(source_id, &links).unwrap();
    }

    #[test]
    fn test_find_ancestor_container() {
        let mut containers = HashMap::new();
        containers.insert(
            "customers/acme".to_string(),
            ("id1".to_string(), "Acme Corp".to_string()),
        );

        assert_eq!(
            find_ancestor_container(Path::new("customers/acme/people/john.md"), &containers),
            Some("customers/acme".to_string())
        );
        assert_eq!(
            find_ancestor_container(Path::new("misc/random.md"), &containers),
            None
        );
    }

    #[test]
    fn test_find_ancestor_container_nested() {
        let mut containers = HashMap::new();
        containers.insert("a/b".to_string(), ("id1".to_string(), "B".to_string()));

        assert_eq!(
            find_ancestor_container(Path::new("a/b/c/d/e/file.md"), &containers),
            Some("a/b".to_string())
        );
    }

    #[test]
    fn test_no_containers_returns_empty() {
        let (db, _tmp) = test_db();
        let docs = vec![make_doc("d1", "Doc 1", "notes/doc1.md")];
        assert!(check_folder_placement(&docs, &db).unwrap().is_empty());
    }

    #[test]
    fn test_majority_rule_generates_question() {
        let (db, _tmp) = test_db();

        // Containers
        let c_acme = make_doc("acme01", "Acme Corp", "acme/acme.md");
        let c_globex = make_doc("globex1", "Globex Inc", "globex/globex.md");
        // Entities under each container
        let person_acme = make_doc("pa0001", "Alice", "acme/people/alice.md");
        let person_g1 = make_doc("pg0001", "Bob", "globex/people/bob.md");
        let person_g2 = make_doc("pg0002", "Carol", "globex/people/carol.md");
        // Doc under acme that links to 1 acme entity + 2 globex entities
        let misplaced = make_doc("doc001", "Some Doc", "acme/notes/some-doc.md");

        for d in [
            &c_acme,
            &c_globex,
            &person_acme,
            &person_g1,
            &person_g2,
            &misplaced,
        ] {
            db.upsert_document(d).unwrap();
        }

        set_links(
            &db,
            &misplaced.id,
            &[
                (&person_acme.id, "mentions"),
                (&person_g1.id, "mentions"),
                (&person_g2.id, "mentions"),
            ],
        );

        let docs = vec![
            c_acme,
            c_globex,
            person_acme,
            person_g1,
            person_g2,
            misplaced.clone(),
        ];
        let result = check_folder_placement(&docs, &db).unwrap();

        assert!(result.contains_key(&misplaced.id));
        let qs = &result[&misplaced.id];
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].question_type, QuestionType::Ambiguous);
        assert!(qs[0].description.contains("Globex Inc"));
        assert!(qs[0].description.contains("2 of 3"));
    }

    #[test]
    fn test_equal_links_no_question() {
        let (db, _tmp) = test_db();

        let c_acme = make_doc("acme01", "Acme Corp", "acme/acme.md");
        let c_globex = make_doc("globex1", "Globex Inc", "globex/globex.md");
        let person_a = make_doc("pa0001", "Alice", "acme/people/alice.md");
        let person_g = make_doc("pg0001", "Bob", "globex/people/bob.md");
        let doc = make_doc("doc001", "Some Doc", "acme/notes/some-doc.md");

        for d in [&c_acme, &c_globex, &person_a, &person_g, &doc] {
            db.upsert_document(d).unwrap();
        }

        // 1 link to acme entity, 1 to globex entity — tie, no question
        set_links(
            &db,
            &doc.id,
            &[(&person_a.id, "mentions"), (&person_g.id, "mentions")],
        );

        let docs = vec![c_acme, c_globex, person_a, person_g, doc.clone()];
        assert!(!check_folder_placement(&docs, &db)
            .unwrap()
            .contains_key(&doc.id));
    }

    #[test]
    fn test_more_links_to_own_container_no_question() {
        let (db, _tmp) = test_db();

        let c_acme = make_doc("acme01", "Acme Corp", "acme/acme.md");
        let c_globex = make_doc("globex1", "Globex Inc", "globex/globex.md");
        let pa1 = make_doc("pa0001", "Alice", "acme/people/alice.md");
        let pa2 = make_doc("pa0002", "Dave", "acme/people/dave.md");
        let pg1 = make_doc("pg0001", "Bob", "globex/people/bob.md");
        let doc = make_doc("doc001", "Some Doc", "acme/notes/some-doc.md");

        for d in [&c_acme, &c_globex, &pa1, &pa2, &pg1, &doc] {
            db.upsert_document(d).unwrap();
        }

        // 2 links to acme entities, 1 to globex
        set_links(
            &db,
            &doc.id,
            &[
                (&pa1.id, "mentions"),
                (&pa2.id, "mentions"),
                (&pg1.id, "mentions"),
            ],
        );

        let docs = vec![c_acme, c_globex, pa1, pa2, pg1, doc.clone()];
        assert!(!check_folder_placement(&docs, &db)
            .unwrap()
            .contains_key(&doc.id));
    }

    #[test]
    fn test_container_docs_skipped() {
        let (db, _tmp) = test_db();

        let c_acme = make_doc("acme01", "Acme Corp", "acme/acme.md");
        let c_globex = make_doc("globex1", "Globex Inc", "globex/globex.md");
        let pg1 = make_doc("pg0001", "Bob", "globex/people/bob.md");

        for d in [&c_acme, &c_globex, &pg1] {
            db.upsert_document(d).unwrap();
        }

        // Container links to entity in another container — should not generate question
        set_links(&db, &c_acme.id, &[(&pg1.id, "mentions")]);

        let docs = vec![c_acme.clone(), c_globex, pg1];
        assert!(!check_folder_placement(&docs, &db)
            .unwrap()
            .contains_key(&c_acme.id));
    }

    #[test]
    fn test_doc_not_under_any_container_skipped() {
        let (db, _tmp) = test_db();

        let c_acme = make_doc("acme01", "Acme Corp", "acme/acme.md");
        let orphan = make_doc("orph01", "Orphan", "misc/orphan.md");
        let pa1 = make_doc("pa0001", "Alice", "acme/people/alice.md");

        for d in [&c_acme, &orphan, &pa1] {
            db.upsert_document(d).unwrap();
        }

        set_links(&db, &orphan.id, &[(&pa1.id, "mentions")]);

        let docs = vec![c_acme, orphan.clone(), pa1];
        assert!(!check_folder_placement(&docs, &db)
            .unwrap()
            .contains_key(&orphan.id));
    }
}
