//! Misplaced document detection.
//!
//! Identifies documents whose content doesn't match their folder-derived type
//! by comparing document embeddings to type centroids, and detects documents
//! filed under the wrong container folder by analyzing cross-references.

use super::{
    collect_active_documents, compute_centroid, cosine_similarity, get_document_embedding,
};
use crate::database::Database;
use crate::error::FactbaseError;
use crate::organize::MisplacedCandidate;
use crate::ProgressReporter;
use std::collections::HashMap;
use std::path::Path;

/// Minimum documents per type to compute a meaningful centroid.
const MIN_DOCS_PER_TYPE: usize = 2;

/// Detect documents that may be misplaced based on embedding similarity to type centroids.
///
/// Computes centroid embedding per document type, then compares each document's
/// embedding to all centroids. If the closest centroid differs from the document's
/// current type, it's flagged as a misplaced candidate.
///
/// # Arguments
/// * `db` - Database connection
/// * `repo_id` - Optional repository filter
///
/// # Returns
/// Vector of misplaced candidates sorted by confidence descending.
pub fn detect_misplaced(
    db: &Database,
    repo_id: Option<&str>,
    progress: &ProgressReporter,
) -> Result<Vec<MisplacedCandidate>, FactbaseError> {
    // Get all documents with their types
    let docs = get_documents_with_types(db, repo_id)?;
    if docs.is_empty() {
        return Ok(Vec::new());
    }

    progress.phase("Detecting misplaced documents");

    // Group documents by type
    let docs_by_type = group_by_type(&docs);

    // Compute centroid embedding per type (only for types with enough docs)
    let centroids = compute_type_centroids(db, &docs_by_type)?;
    if centroids.is_empty() {
        return Ok(Vec::new());
    }

    // Compare each document to all centroids
    let mut candidates = Vec::new();
    let total = docs.len();
    for (i, (doc_id, doc_title, current_type)) in docs.iter().enumerate() {
        progress.report(i + 1, total, doc_title);
        // Skip docs whose type doesn't have a centroid (too few docs)
        if !centroids.contains_key(current_type) {
            continue;
        }

        // Get document embedding
        let Some(embedding) = get_document_embedding(db, doc_id)? else {
            continue;
        };

        // Find closest centroid
        let (closest_type, closest_sim) = find_closest_centroid(&embedding, &centroids);

        // If closest type differs from current type, it's a candidate
        if closest_type != *current_type {
            // Calculate similarity to current type for confidence
            let current_sim = if let Some(current_centroid) = centroids.get(current_type) {
                cosine_similarity(&embedding, current_centroid)
            } else {
                0.0
            };

            // Confidence is how much closer the suggested type is
            let confidence = closest_sim - current_sim;

            // Only report if confidence is positive (suggested type is actually closer)
            if confidence > 0.0 {
                candidates.push(MisplacedCandidate {
                    doc_id: doc_id.clone(),
                    doc_title: doc_title.clone(),
                    current_type: current_type.clone(),
                    suggested_type: closest_type.clone(),
                    confidence,
                    rationale: format!(
                        "Similarity to '{closest_type}': {closest_sim:.2}, to '{current_type}': {current_sim:.2}"
                    ),
                    current_folder: None,
                    suggested_folder: None,
                });
            }
        }
    }

    // Sort by confidence descending
    candidates.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Phase 2: Detect folder misplacement via cross-references
    let folder_candidates = detect_folder_misplacement(db, repo_id)?;
    candidates.extend(folder_candidates);

    Ok(candidates)
}

/// Get all non-deleted documents with their types.
fn get_documents_with_types(
    db: &Database,
    repo_id: Option<&str>,
) -> Result<Vec<(String, String, String)>, FactbaseError> {
    Ok(collect_active_documents(db, repo_id)?
        .into_iter()
        .map(|d| (d.id, d.title, d.doc_type.unwrap_or_default()))
        .collect())
}

/// Group documents by their type.
fn group_by_type(docs: &[(String, String, String)]) -> HashMap<String, Vec<String>> {
    let mut by_type: HashMap<String, Vec<String>> = HashMap::new();
    for (doc_id, _, doc_type) in docs {
        if !doc_type.is_empty() {
            by_type
                .entry(doc_type.clone())
                .or_default()
                .push(doc_id.clone());
        }
    }
    by_type
}

/// Compute centroid embedding for each type with enough documents.
fn compute_type_centroids(
    db: &Database,
    docs_by_type: &HashMap<String, Vec<String>>,
) -> Result<HashMap<String, Vec<f32>>, FactbaseError> {
    let mut centroids = HashMap::new();

    for (doc_type, doc_ids) in docs_by_type {
        if doc_ids.len() < MIN_DOCS_PER_TYPE {
            continue;
        }

        // Collect embeddings for this type
        let mut embeddings = Vec::new();
        for doc_id in doc_ids {
            if let Some(emb) = get_document_embedding(db, doc_id)? {
                embeddings.push(emb);
            }
        }

        if embeddings.len() < MIN_DOCS_PER_TYPE {
            continue;
        }

        // Compute centroid (average of all embeddings)
        let centroid = compute_centroid(&embeddings);
        centroids.insert(doc_type.clone(), centroid);
    }

    Ok(centroids)
}

/// Find the type with the closest centroid to the given embedding.
fn find_closest_centroid(
    embedding: &[f32],
    centroids: &HashMap<String, Vec<f32>>,
) -> (String, f32) {
    let mut best_type = String::new();
    let mut best_sim = f32::NEG_INFINITY;

    for (doc_type, centroid) in centroids {
        let sim = cosine_similarity(embedding, centroid);
        if sim > best_sim {
            best_sim = sim;
            best_type = doc_type.clone();
        }
    }

    (best_type, best_sim)
}

/// Detect documents filed under the wrong container folder by analyzing cross-references.
///
/// A "container document" follows the entity folder convention: `prefix/name/name.md`.
/// If document A lives under container X but links to container Y, A may be misplaced.
fn detect_folder_misplacement(
    db: &Database,
    repo_id: Option<&str>,
) -> Result<Vec<MisplacedCandidate>, FactbaseError> {
    let docs = collect_active_documents(db, repo_id)?;
    if docs.is_empty() {
        return Ok(Vec::new());
    }

    // Identify container documents (filename stem == parent folder name)
    // and build: container_folder_path → (doc_id, title)
    let mut container_by_folder: HashMap<String, (String, String)> = HashMap::new();
    // Also: doc_id → container_folder_path for container docs
    let mut container_by_id: HashMap<String, String> = HashMap::new();

    for doc in &docs {
        let path = Path::new(&doc.file_path);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let parent = path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("");
        if !stem.is_empty() && stem.eq_ignore_ascii_case(parent) {
            // This is a container document. The container folder is the parent directory.
            if let Some(parent_path) = path.parent() {
                let folder_key = parent_path.to_string_lossy().to_string();
                container_by_folder.insert(folder_key.clone(), (doc.id.clone(), doc.title.clone()));
                container_by_id.insert(doc.id.clone(), folder_key);
            }
        }
    }

    if container_by_folder.is_empty() {
        return Ok(Vec::new());
    }

    // For each non-container document, find which container it lives under
    let mut candidates = Vec::new();
    let doc_ids: Vec<&str> = docs.iter().map(|d| d.id.as_str()).collect();
    let all_links = db.get_links_for_documents(&doc_ids)?;

    for doc in &docs {
        // Skip container documents themselves
        if container_by_id.contains_key(&doc.id) {
            continue;
        }

        // Find which container folder this doc lives under
        let doc_path = Path::new(&doc.file_path);
        let doc_container = find_ancestor_container(doc_path, &container_by_folder);
        let Some(doc_container) = doc_container else {
            continue; // Not under any container
        };

        // Check outgoing links for references to other containers
        let (outgoing, _) = match all_links.get(&doc.id) {
            Some(links) => links,
            None => continue,
        };

        for link in outgoing {
            let Some(linked_container) = container_by_id.get(&link.target_id) else {
                continue; // Target is not a container document
            };

            if *linked_container != doc_container {
                let (_, suggested_title) = &container_by_folder[linked_container];
                let (_, current_title) = &container_by_folder[&doc_container];
                candidates.push(MisplacedCandidate {
                    doc_id: doc.id.clone(),
                    doc_title: doc.title.clone(),
                    current_type: doc.doc_type.clone().unwrap_or_default(),
                    suggested_type: doc.doc_type.clone().unwrap_or_default(),
                    confidence: 1.0,
                    rationale: format!(
                        "Links to '{}' but filed under '{}'",
                        suggested_title, current_title
                    ),
                    current_folder: Some(doc_container.clone()),
                    suggested_folder: Some(linked_container.clone()),
                });
                break; // One suggestion per document
            }
        }
    }

    Ok(candidates)
}

/// Walk up the path to find the nearest ancestor that is a known container folder.
fn find_ancestor_container(
    path: &Path,
    containers: &HashMap<String, (String, String)>,
) -> Option<String> {
    let mut current = path.parent(); // start from parent dir of the file
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

    #[test]
    fn test_find_closest_centroid() {
        let mut centroids = HashMap::new();
        centroids.insert("type_a".to_string(), vec![1.0, 0.0, 0.0]);
        centroids.insert("type_b".to_string(), vec![0.0, 1.0, 0.0]);

        let embedding = vec![0.9, 0.1, 0.0];
        let (closest, sim) = find_closest_centroid(&embedding, &centroids);
        assert_eq!(closest, "type_a");
        assert!(sim > 0.9);
    }

    #[test]
    fn test_group_by_type() {
        let docs = vec![
            (
                "doc1".to_string(),
                "Doc 1".to_string(),
                "person".to_string(),
            ),
            (
                "doc2".to_string(),
                "Doc 2".to_string(),
                "person".to_string(),
            ),
            (
                "doc3".to_string(),
                "Doc 3".to_string(),
                "project".to_string(),
            ),
            ("doc4".to_string(), "Doc 4".to_string(), "".to_string()), // Empty type ignored
        ];
        let by_type = group_by_type(&docs);
        assert_eq!(by_type.get("person").map(|v| v.len()), Some(2));
        assert_eq!(by_type.get("project").map(|v| v.len()), Some(1));
        assert!(!by_type.contains_key(""));
    }

    #[test]
    fn test_misplaced_candidate_struct() {
        let candidate = MisplacedCandidate {
            doc_id: "abc123".to_string(),
            doc_title: "John Smith".to_string(),
            current_type: "project".to_string(),
            suggested_type: "person".to_string(),
            confidence: 0.15,
            rationale: "Similarity to 'person': 0.85, to 'project': 0.70".to_string(),
            current_folder: None,
            suggested_folder: None,
        };
        assert_eq!(candidate.doc_id, "abc123");
        assert_eq!(candidate.current_type, "project");
        assert_eq!(candidate.suggested_type, "person");
        assert!(candidate.confidence > 0.0);
    }

    #[test]
    fn test_find_ancestor_container() {
        let mut containers = HashMap::new();
        containers.insert(
            "customers/acme".to_string(),
            ("id1".to_string(), "Acme Corp".to_string()),
        );
        containers.insert(
            "customers/globex".to_string(),
            ("id2".to_string(), "Globex Inc".to_string()),
        );

        // Doc under acme
        let path = Path::new("customers/acme/people/john.md");
        assert_eq!(
            find_ancestor_container(path, &containers),
            Some("customers/acme".to_string())
        );

        // Doc under globex
        let path = Path::new("customers/globex/notes/meeting.md");
        assert_eq!(
            find_ancestor_container(path, &containers),
            Some("customers/globex".to_string())
        );

        // Doc not under any container
        let path = Path::new("misc/random.md");
        assert_eq!(find_ancestor_container(path, &containers), None);
    }

    #[test]
    fn test_find_ancestor_container_nested() {
        let mut containers = HashMap::new();
        containers.insert(
            "a/b".to_string(),
            ("id1".to_string(), "B".to_string()),
        );

        // Deeply nested doc still finds ancestor
        let path = Path::new("a/b/c/d/e/file.md");
        assert_eq!(
            find_ancestor_container(path, &containers),
            Some("a/b".to_string())
        );
    }

    #[test]
    fn test_folder_misplacement_candidate_fields() {
        let candidate = MisplacedCandidate {
            doc_id: "f39677".to_string(),
            doc_title: "Dr. Heather Bassett".to_string(),
            current_type: "person".to_string(),
            suggested_type: "person".to_string(),
            confidence: 1.0,
            rationale: "Links to 'XSOLIS' but filed under 'Trend Health Partners'".to_string(),
            current_folder: Some("customers/trend-health-partners".to_string()),
            suggested_folder: Some("customers/xsolis".to_string()),
        };
        assert!(candidate.current_folder.is_some());
        assert!(candidate.suggested_folder.is_some());
        // Type stays the same for folder misplacement
        assert_eq!(candidate.current_type, candidate.suggested_type);
    }
}
