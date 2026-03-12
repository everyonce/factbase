//! Cross-document duplicate entity entry detection.
//!
//! Finds the same entity appearing as an entry in multiple parent documents
//! (e.g., "Jane Smith" listed under both `companies/acme.md` and `companies/globex.md`).

use std::collections::{HashMap, HashSet};

use tracing::debug;

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::organize::detect::entity_entries::extract_entity_entries;
use crate::organize::types::{DuplicateEntry, EntryLocation};
use crate::patterns::MANUAL_LINK_REGEX;
use crate::ProgressReporter;

use super::collect_active_documents;
use super::cosine_similarity;

/// Similarity threshold for entry name matching via embeddings.
const NAME_SIMILARITY_THRESHOLD: f32 = 0.85;

/// Detect entity entries that appear in multiple documents.
///
/// Extracts named entries (sub-headings, bold-name list items) from all documents,
/// then groups them by name similarity — first by exact normalized match, then by
/// embedding similarity for fuzzy matches.
pub async fn detect_duplicate_entries(
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    repo_id: Option<&str>,
    progress: &ProgressReporter,
) -> Result<Vec<DuplicateEntry>, FactbaseError> {
    let docs = collect_active_documents(db, repo_id)?;

    progress.phase("Detecting duplicate entries");

    // Phase 0: Detect document-level duplicates (same title + same type, different paths).
    let mut doc_title_groups: HashMap<(String, Option<String>), Vec<&crate::models::Document>> =
        HashMap::new();
    for doc in &docs {
        let key = (normalize_name(&doc.title), doc.doc_type.clone());
        doc_title_groups.entry(key).or_default().push(doc);
    }

    let mut results: Vec<DuplicateEntry> = Vec::new();
    for ((norm_title, _), group) in &doc_title_groups {
        if group.len() >= 2 {
            let entries: Vec<EntryLocation> = group
                .iter()
                .map(|d| EntryLocation {
                    doc_id: d.id.clone(),
                    doc_title: d.title.clone(),
                    section: String::new(),
                    line_start: 1,
                    facts: vec![d.file_path.clone()],
                })
                .collect();
            results.push(DuplicateEntry {
                entity_name: norm_title.clone(),
                entries,
            });
        }
    }

    // Extract entries from all documents.
    let mut all_entries: Vec<(String, EntryLocation)> = Vec::new(); // (normalized_name, location)
                                                                    // Build title→doc_id map for authoritative document lookup.
    let title_to_doc: HashMap<String, &str> = docs
        .iter()
        .map(|d| (normalize_name(&d.title), d.id.as_str()))
        .collect();

    let total = docs.len();
    for (i, doc) in docs.iter().enumerate() {
        progress.report(i + 1, total, &doc.title);
        let entries = extract_entity_entries(&doc.content, &doc.id);
        let norm_doc_title = normalize_name(&doc.title);
        for entry in entries {
            let norm_name = normalize_name(&entry.name);

            // Filter: skip entries whose facts are all cross-reference links.
            if entry.facts.iter().all(|f| is_cross_reference(f)) {
                continue;
            }

            // Filter: skip self-mentions (entry name matches parent doc title).
            if norm_name == norm_doc_title {
                continue;
            }

            // Filter: skip entries from the entity's authoritative document.
            // If a document exists whose title matches this entry name, entries
            // from that document are not duplicates — it's the canonical source.
            if let Some(&auth_doc_id) = title_to_doc.get(&norm_name) {
                if auth_doc_id == doc.id {
                    continue;
                }
            }

            let location = EntryLocation {
                doc_id: doc.id.clone(),
                doc_title: doc.title.clone(),
                section: entry.section,
                line_start: entry.line_start,
                facts: entry.facts,
            };
            all_entries.push((norm_name, location));
        }
    }

    if all_entries.is_empty() {
        return Ok(results);
    }

    // Phase 1: Group by exact normalized name.
    let mut exact_groups: HashMap<String, Vec<EntryLocation>> = HashMap::new();
    for (name, loc) in &all_entries {
        exact_groups
            .entry(name.clone())
            .or_default()
            .push(loc.clone());
    }

    // Collect groups with 2+ entries from different documents.
    let mut matched_names: HashSet<String> = HashSet::new();

    for (name, locations) in &exact_groups {
        let unique_docs: HashSet<&str> = locations.iter().map(|l| l.doc_id.as_str()).collect();
        if unique_docs.len() >= 2 {
            results.push(DuplicateEntry {
                entity_name: name.clone(),
                entries: locations.clone(),
            });
            matched_names.insert(name.clone());
        }
    }

    // Phase 2: Embedding-based fuzzy matching for unmatched singleton entries.
    let singletons: Vec<&str> = exact_groups
        .iter()
        .filter(|(name, locs)| locs.len() == 1 && !matched_names.contains(name.as_str()))
        .map(|(name, _)| name.as_str())
        .collect();

    if singletons.len() >= 2 {
        progress.log(&format!(
            "Matching {} entries for fuzzy duplicates...",
            singletons.len()
        ));
        let texts: Vec<&str> = singletons.to_vec();
        let embeddings = embedding.generate_batch(&texts).await?;

        debug!(
            "Generated {} embeddings for singleton entry names",
            embeddings.len()
        );

        // Compare all pairs, merge similar ones.
        let mut merged: Vec<bool> = vec![false; singletons.len()];
        for i in 0..singletons.len() {
            if merged[i] {
                continue;
            }
            let mut group_names = vec![singletons[i]];
            for j in (i + 1)..singletons.len() {
                if merged[j] {
                    continue;
                }
                let sim = cosine_similarity(&embeddings[i], &embeddings[j]);
                if sim >= NAME_SIMILARITY_THRESHOLD {
                    group_names.push(singletons[j]);
                    merged[j] = true;
                }
            }
            if group_names.len() >= 2 {
                // Collect all locations for this fuzzy group.
                let mut locations = Vec::new();
                for gn in &group_names {
                    if let Some(locs) = exact_groups.get(*gn) {
                        locations.extend(locs.clone());
                    }
                }
                let unique_docs: HashSet<&str> =
                    locations.iter().map(|l| l.doc_id.as_str()).collect();
                if unique_docs.len() >= 2 {
                    results.push(DuplicateEntry {
                        entity_name: group_names[0].to_string(),
                        entries: locations,
                    });
                }
            }
        }
    }

    // Sort by number of entries descending (most duplicated first).
    results.sort_by(|a, b| b.entries.len().cmp(&a.entries.len()));

    Ok(results)
}

/// Normalize an entity name for comparison: lowercase, trim, collapse whitespace.
fn normalize_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Check if a fact line is purely a cross-reference link (`[[id]]`).
fn is_cross_reference(fact: &str) -> bool {
    let stripped = fact.trim().trim_start_matches(['-', '*']).trim();
    MANUAL_LINK_REGEX.is_match(stripped) && stripped.len() <= 12 // `[[abcdef]]` = 10 chars + margin
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_doc, test_repo_in_db};
    use crate::embedding::test_helpers::HashEmbedding;

    fn make_doc(id: &str, title: &str, content: &str) -> crate::models::Document {
        let mut doc = test_doc(id, title);
        doc.repo_id = "r1".to_string();
        doc.content = content.to_string();
        doc
    }

    #[test]
    fn test_normalize_name() {
        assert_eq!(normalize_name("Jane Smith"), "jane smith");
        assert_eq!(normalize_name("  Jane   Smith  "), "jane smith");
        assert_eq!(normalize_name("ACME Corp"), "acme corp");
    }

    #[test]
    fn test_normalize_name_preserves_content() {
        assert_eq!(normalize_name("Dr. Jane O'Brien"), "dr. jane o'brien");
    }

    #[test]
    fn test_is_cross_reference() {
        assert!(is_cross_reference("- [[abc123]]"));
        assert!(is_cross_reference("* [[def456]]"));
        assert!(is_cross_reference("  - [[aaa111]]"));
        assert!(!is_cross_reference("- VP Engineering"));
        assert!(!is_cross_reference("- Works at [[abc123]] since 2020"));
    }

    #[tokio::test]
    async fn test_detect_no_documents() {
        let (db, _tmp) = test_db();
        let emb = HashEmbedding;
        let result = detect_duplicate_entries(&db, &emb, None, &crate::ProgressReporter::Silent)
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_detect_exact_name_match() {
        let (db, tmp) = test_db();
        test_repo_in_db(&db, "r1", tmp.path());

        let doc1 = make_doc(
            "aaa111",
            "Acme Corp",
            "# Acme Corp\n\n## Team\n\n### Jane Smith\n\n- VP Engineering\n- Joined 2020\n",
        );
        let doc2 = make_doc(
            "bbb222",
            "Globex Inc",
            "# Globex Inc\n\n## Staff\n\n### Jane Smith\n\n- Director\n- Since 2022\n",
        );
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        let emb = HashEmbedding;
        let result =
            detect_duplicate_entries(&db, &emb, Some("r1"), &crate::ProgressReporter::Silent)
                .await
                .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].entity_name, "jane smith");
        assert_eq!(result[0].entries.len(), 2);

        let doc_ids: Vec<&str> = result[0]
            .entries
            .iter()
            .map(|e| e.doc_id.as_str())
            .collect();
        assert!(doc_ids.contains(&"aaa111"));
        assert!(doc_ids.contains(&"bbb222"));
    }

    #[tokio::test]
    async fn test_detect_no_duplicates_single_doc() {
        let (db, tmp) = test_db();
        test_repo_in_db(&db, "r1", tmp.path());

        let doc1 = make_doc(
            "aaa111",
            "Acme Corp",
            "# Acme Corp\n\n## Team\n\n### Jane Smith\n\n- VP Engineering\n",
        );
        db.upsert_document(&doc1).unwrap();

        let emb = HashEmbedding;
        let result =
            detect_duplicate_entries(&db, &emb, Some("r1"), &crate::ProgressReporter::Silent)
                .await
                .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_detect_same_doc_not_duplicate() {
        let (db, tmp) = test_db();
        test_repo_in_db(&db, "r1", tmp.path());

        // Two entries in the same document should NOT be flagged.
        let doc1 = make_doc(
            "aaa111",
            "Acme Corp",
            "# Acme Corp\n\n## Team\n\n### Alice\n\n- Engineer\n\n### Bob\n\n- Manager\n",
        );
        db.upsert_document(&doc1).unwrap();

        let emb = HashEmbedding;
        let result =
            detect_duplicate_entries(&db, &emb, Some("r1"), &crate::ProgressReporter::Silent)
                .await
                .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_detect_preserves_entry_metadata() {
        let (db, tmp) = test_db();
        test_repo_in_db(&db, "r1", tmp.path());

        let doc1 = make_doc(
            "aaa111",
            "Acme Corp",
            "# Acme Corp\n\n## Team\n\n### Bob Jones\n\n- CTO\n- Founded division\n",
        );
        let doc2 = make_doc(
            "bbb222",
            "Globex Inc",
            "# Globex Inc\n\n## Leadership\n\n### Bob Jones\n\n- Advisor\n",
        );
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        let emb = HashEmbedding;
        let result =
            detect_duplicate_entries(&db, &emb, Some("r1"), &crate::ProgressReporter::Silent)
                .await
                .unwrap();

        assert_eq!(result.len(), 1);
        let acme_entry = result[0]
            .entries
            .iter()
            .find(|e| e.doc_id == "aaa111")
            .unwrap();
        assert_eq!(acme_entry.section, "Team");
        assert_eq!(acme_entry.facts.len(), 2);

        let globex_entry = result[0]
            .entries
            .iter()
            .find(|e| e.doc_id == "bbb222")
            .unwrap();
        assert_eq!(globex_entry.section, "Leadership");
        assert_eq!(globex_entry.facts.len(), 1);
    }

    #[tokio::test]
    async fn test_detect_deleted_docs_excluded() {
        let (db, tmp) = test_db();
        test_repo_in_db(&db, "r1", tmp.path());

        let doc1 = make_doc(
            "aaa111",
            "Acme Corp",
            "# Acme Corp\n\n## Team\n\n### Jane Smith\n\n- VP\n",
        );
        let doc2 = make_doc(
            "bbb222",
            "Globex Inc",
            "# Globex Inc\n\n## Staff\n\n### Jane Smith\n\n- Director\n",
        );
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();
        db.mark_deleted("bbb222").unwrap();

        let emb = HashEmbedding;
        let result =
            detect_duplicate_entries(&db, &emb, Some("r1"), &crate::ProgressReporter::Silent)
                .await
                .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_filter_cross_reference_entries() {
        let (db, tmp) = test_db();
        test_repo_in_db(&db, "r1", tmp.path());

        // Entry whose only fact is a cross-reference link should be filtered out.
        let doc1 = make_doc(
            "aaa111",
            "Acme Corp",
            "# Acme Corp\n\n## Team\n\n### Jane Smith\n\n- [[ab12cd]]\n",
        );
        let doc2 = make_doc(
            "bbb222",
            "Globex Inc",
            "# Globex Inc\n\n## Staff\n\n### Jane Smith\n\n- [[ab12cd]]\n",
        );
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        let emb = HashEmbedding;
        let result =
            detect_duplicate_entries(&db, &emb, Some("r1"), &crate::ProgressReporter::Silent)
                .await
                .unwrap();
        assert!(
            result.is_empty(),
            "Cross-reference-only entries should be filtered"
        );
    }

    #[tokio::test]
    async fn test_filter_self_mention() {
        let (db, tmp) = test_db();
        test_repo_in_db(&db, "r1", tmp.path());

        // A person doc mentioning themselves should be filtered.
        let doc1 = make_doc(
            "ab12cd",
            "Jane Smith",
            "# Jane Smith\n\n## About\n\n### Jane Smith\n\n- VP Engineering\n",
        );
        let doc2 = make_doc(
            "bbb222",
            "Acme Corp",
            "# Acme Corp\n\n## Team\n\n### Jane Smith\n\n- VP Engineering\n",
        );
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        let emb = HashEmbedding;
        let result =
            detect_duplicate_entries(&db, &emb, Some("r1"), &crate::ProgressReporter::Silent)
                .await
                .unwrap();
        // Only one entry remains (from Acme Corp), so no duplicate group.
        assert!(
            result.is_empty(),
            "Self-mention should be filtered, leaving only one entry"
        );
    }

    #[tokio::test]
    async fn test_filter_authoritative_doc() {
        let (db, tmp) = test_db();
        test_repo_in_db(&db, "r1", tmp.path());

        // Jane Smith has her own doc — entries from it should be excluded.
        let jane_doc = make_doc(
            "ab12cd",
            "Jane Smith",
            "# Jane Smith\n\n## Career\n\n### Acme Corp\n\n- VP Engineering @t[2022..]\n",
        );
        let acme_doc = make_doc(
            "aaa111",
            "Acme Corp",
            "# Acme Corp\n\n## Team\n\n### Jane Smith\n\n- VP Engineering\n",
        );
        let globex_doc = make_doc(
            "bbb222",
            "Globex Inc",
            "# Globex Inc\n\n## Staff\n\n### Jane Smith\n\n- Consultant\n",
        );
        db.upsert_document(&jane_doc).unwrap();
        db.upsert_document(&acme_doc).unwrap();
        db.upsert_document(&globex_doc).unwrap();

        let emb = HashEmbedding;
        let result =
            detect_duplicate_entries(&db, &emb, Some("r1"), &crate::ProgressReporter::Silent)
                .await
                .unwrap();

        // "Jane Smith" entries from acme and globex should be flagged as duplicates.
        // The entry from jane_doc is excluded (authoritative doc).
        let jane_dup = result.iter().find(|d| d.entity_name == "jane smith");
        assert!(
            jane_dup.is_some(),
            "Should detect Jane Smith duplicate across acme and globex"
        );
        let entries = &jane_dup.unwrap().entries;
        assert_eq!(entries.len(), 2);
        assert!(
            !entries.iter().any(|e| e.doc_id == "ab12cd"),
            "Authoritative doc should be excluded"
        );
    }

    #[tokio::test]
    async fn test_mixed_cross_ref_and_real_facts_kept() {
        let (db, tmp) = test_db();
        test_repo_in_db(&db, "r1", tmp.path());

        // Entry with a mix of cross-ref and real facts should NOT be filtered.
        let doc1 = make_doc(
            "aaa111",
            "Acme Corp",
            "# Acme Corp\n\n## Team\n\n### Jane Smith\n\n- [[ab12cd]]\n- VP Engineering\n",
        );
        let doc2 = make_doc(
            "bbb222",
            "Globex Inc",
            "# Globex Inc\n\n## Staff\n\n### Jane Smith\n\n- Director\n",
        );
        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        let emb = HashEmbedding;
        let result =
            detect_duplicate_entries(&db, &emb, Some("r1"), &crate::ProgressReporter::Silent)
                .await
                .unwrap();
        assert_eq!(result.len(), 1, "Mixed facts entry should be kept");
    }

    #[tokio::test]
    async fn test_detect_document_level_duplicates_same_title_and_type() {
        let (db, tmp) = test_db();
        test_repo_in_db(&db, "r1", tmp.path());

        // Two separate documents with the same title and type in different folders.
        let mut doc1 = make_doc(
            "cedb70",
            "Alice Chen",
            "# Alice Chen\n\n- CTO at Acme Corp\n",
        );
        doc1.doc_type = Some("person".to_string());
        doc1.file_path = "customers/acme/people/alice-chen.md".to_string();

        let mut doc2 = make_doc(
            "479fee",
            "Alice Chen",
            "# Alice Chen\n\n- CTO at Beta Inc\n",
        );
        doc2.doc_type = Some("person".to_string());
        doc2.file_path = "customers/beta/people/alice-chen.md".to_string();

        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        let emb = HashEmbedding;
        let result =
            detect_duplicate_entries(&db, &emb, Some("r1"), &crate::ProgressReporter::Silent)
                .await
                .unwrap();

        let dup = result.iter().find(|d| d.entity_name == "alice chen");
        assert!(dup.is_some(), "Should detect document-level duplicate");
        let entries = &dup.unwrap().entries;
        assert_eq!(entries.len(), 2);
        let ids: HashSet<&str> = entries.iter().map(|e| e.doc_id.as_str()).collect();
        assert!(ids.contains("cedb70"));
        assert!(ids.contains("479fee"));
        // facts contain file paths for document-level duplicates
        assert!(entries.iter().any(|e| e.facts[0].contains("acme")));
        assert!(entries.iter().any(|e| e.facts[0].contains("beta")));
    }

    #[tokio::test]
    async fn test_no_document_level_dup_for_different_types() {
        let (db, tmp) = test_db();
        test_repo_in_db(&db, "r1", tmp.path());

        // Same title but different types should NOT be flagged.
        let mut doc1 = make_doc("aaa111", "Atlas", "# Atlas\n\n- Greek titan\n");
        doc1.doc_type = Some("mythology".to_string());

        let mut doc2 = make_doc("bbb222", "Atlas", "# Atlas\n\n- Software project\n");
        doc2.doc_type = Some("project".to_string());

        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        let emb = HashEmbedding;
        let result =
            detect_duplicate_entries(&db, &emb, Some("r1"), &crate::ProgressReporter::Silent)
                .await
                .unwrap();

        let dup = result.iter().find(|d| d.entity_name == "atlas");
        assert!(
            dup.is_none(),
            "Different types should not be flagged as duplicates"
        );
    }

    #[tokio::test]
    async fn test_document_level_dup_with_no_entry_content() {
        let (db, tmp) = test_db();
        test_repo_in_db(&db, "r1", tmp.path());

        // Documents with no sub-entries should still be detected as duplicates.
        let mut doc1 = make_doc("aaa111", "Jane Smith", "# Jane Smith\n\nSome notes.\n");
        doc1.doc_type = Some("person".to_string());
        doc1.file_path = "folder-a/jane-smith.md".to_string();

        let mut doc2 = make_doc("bbb222", "Jane Smith", "# Jane Smith\n\nOther notes.\n");
        doc2.doc_type = Some("person".to_string());
        doc2.file_path = "folder-b/jane-smith.md".to_string();

        db.upsert_document(&doc1).unwrap();
        db.upsert_document(&doc2).unwrap();

        let emb = HashEmbedding;
        let result =
            detect_duplicate_entries(&db, &emb, Some("r1"), &crate::ProgressReporter::Silent)
                .await
                .unwrap();

        assert!(
            result.iter().any(|d| d.entity_name == "jane smith"),
            "Should detect doc-level dup even with no sub-entries"
        );
    }
}
