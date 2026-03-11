use factbase::models::Repository;
use factbase::output::{format_bytes, format_json};
use factbase::processor::{DocumentProcessor, calculate_fact_stats, count_facts_with_sources};
use factbase::scanner::Scanner;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize)]
pub(super) struct FileAssessment {
    pub path: String,
    pub title: String,
    pub doc_type: String,
    pub size_bytes: u64,
    pub has_id_header: bool,
    pub has_h1_title: bool,
    pub total_facts: usize,
    pub facts_with_temporal: usize,
    pub temporal_coverage: f32,
    pub facts_with_sources: usize,
    pub source_coverage: f32,
    pub in_typed_folder: bool,
    pub quality_score: u8,
}

#[derive(Debug, Serialize)]
pub(super) struct AssessmentSummary {
    pub total_files: usize,
    pub total_size: u64,
    pub total_size_human: String,
    pub files_with_ids: usize,
    pub files_with_temporal: usize,
    pub files_with_sources: usize,
    pub files_in_typed_folders: usize,
    pub files_in_root: usize,
    pub avg_quality_score: f32,
    pub type_distribution: HashMap<String, usize>,
    pub readiness: String,
    pub next_steps: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct Assessment {
    pub repositories: Vec<RepoAssessment>,
    pub summary: AssessmentSummary,
    pub structure_suggestions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct RepoAssessment {
    pub id: String,
    pub name: String,
    pub path: String,
    pub files: Vec<FileAssessment>,
}

fn assess_file(path: &Path, repo_root: &Path) -> Option<FileAssessment> {
    let content = fs::read_to_string(path).ok()?;
    let metadata = fs::metadata(path).ok()?;
    let processor = DocumentProcessor::new();

    let has_id = processor.extract_id(&content).is_some();
    let title = processor.extract_title(&content, path);
    let doc_type = processor.derive_type(path, repo_root);

    let has_h1 = content.lines().any(|l| {
        let t = l.trim();
        !t.starts_with("<!-- factbase:") && t.starts_with("# ")
    });

    let fact_stats = calculate_fact_stats(&content);
    let facts_with_sources = count_facts_with_sources(&content);

    let source_coverage = if fact_stats.total_facts > 0 {
        facts_with_sources as f32 / fact_stats.total_facts as f32
    } else {
        0.0
    };

    let relative = path.strip_prefix(repo_root).unwrap_or(path);
    let in_typed_folder = relative
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|n| !n.is_empty());

    // Quality score: 0-100
    let mut score: u8 = 0;
    if has_id {
        score += 10;
    }
    if has_h1 {
        score += 15;
    }
    if fact_stats.total_facts > 0 {
        score += 15;
    }
    score += (25.0 * fact_stats.coverage) as u8;
    score += (25.0 * source_coverage) as u8;
    if in_typed_folder {
        score += 10;
    }

    Some(FileAssessment {
        path: relative.display().to_string(),
        title,
        doc_type,
        size_bytes: metadata.len(),
        has_id_header: has_id,
        has_h1_title: has_h1,
        total_facts: fact_stats.total_facts,
        facts_with_temporal: fact_stats.facts_with_tags,
        temporal_coverage: fact_stats.coverage,
        facts_with_sources,
        source_coverage,
        in_typed_folder,
        quality_score: score,
    })
}

fn build_summary(repos: &[RepoAssessment]) -> (AssessmentSummary, Vec<String>) {
    let all_files: Vec<&FileAssessment> = repos.iter().flat_map(|r| &r.files).collect();
    let total = all_files.len();

    if total == 0 {
        return (
            AssessmentSummary {
                total_files: 0,
                total_size: 0,
                total_size_human: "0 B".to_string(),
                files_with_ids: 0,
                files_with_temporal: 0,
                files_with_sources: 0,
                files_in_typed_folders: 0,
                files_in_root: 0,
                avg_quality_score: 0.0,
                type_distribution: HashMap::new(),
                readiness: "No markdown files found".to_string(),
                next_steps: vec!["Create markdown files or check repository path".to_string()],
            },
            vec![],
        );
    }

    let total_size: u64 = all_files.iter().map(|f| f.size_bytes).sum();
    let files_with_ids = all_files.iter().filter(|f| f.has_id_header).count();
    let files_with_temporal = all_files.iter().filter(|f| f.facts_with_temporal > 0).count();
    let files_with_sources = all_files.iter().filter(|f| f.facts_with_sources > 0).count();
    let files_in_typed = all_files.iter().filter(|f| f.in_typed_folder).count();
    let files_in_root = total - files_in_typed;
    let avg_score: f32 =
        all_files.iter().map(|f| f.quality_score as f32).sum::<f32>() / total as f32;

    let mut type_dist: HashMap<String, usize> = HashMap::new();
    for f in &all_files {
        *type_dist.entry(f.doc_type.clone()).or_default() += 1;
    }

    // Readiness assessment
    let well_formatted = all_files
        .iter()
        .filter(|f| f.quality_score >= 50)
        .count();
    let pct_well = (well_formatted as f32 / total as f32 * 100.0) as u8;
    let needs_temporal = all_files
        .iter()
        .filter(|f| f.total_facts > 0 && f.temporal_coverage < 0.5)
        .count();
    let needs_sources = all_files
        .iter()
        .filter(|f| f.total_facts > 0 && f.source_coverage < 0.5)
        .count();

    let readiness = format!(
        "{}% of files are well-formatted (score >= 50), {}% need temporal tags, {}% need sources",
        pct_well,
        if total > 0 {
            (needs_temporal as f32 / total as f32 * 100.0) as u8
        } else {
            0
        },
        if total > 0 {
            (needs_sources as f32 / total as f32 * 100.0) as u8
        } else {
            0
        },
    );

    let mut next_steps = Vec::new();
    if files_with_ids < total {
        next_steps.push(format!(
            "Run `factbase scan` to assign document IDs to {} untracked file(s)",
            total - files_with_ids
        ));
    }
    if needs_temporal > 0 {
        next_steps.push(format!(
            "Run `factbase check` to generate temporal tag suggestions for {needs_temporal} file(s)"
        ));
    }
    if needs_sources > 0 {
        next_steps.push(format!(
            "{needs_sources} file(s) have unsourced facts — add source footnotes for provenance"
        ));
    }
    if files_in_root > 0 {
        next_steps.push(format!(
            "Consider organizing {files_in_root} root-level file(s) into type folders"
        ));
    }
    if next_steps.is_empty() {
        next_steps.push("Files look good! Run `factbase scan` to index.".to_string());
    }

    // Structure suggestions
    let mut suggestions = Vec::new();
    for (doc_type, count) in &type_dist {
        if doc_type != "document" {
            suggestions.push(format!(
                "Found {count} file(s) in {doc_type}/ — these will become doc type \"{doc_type}\""
            ));
        }
    }
    if files_in_root > 0 {
        suggestions.push(format!(
            "Found {files_in_root} file(s) in root with no folder — consider organizing into type folders"
        ));
    }

    (
        AssessmentSummary {
            total_files: total,
            total_size,
            total_size_human: format_bytes(total_size),
            files_with_ids,
            files_with_temporal,
            files_with_sources,
            files_in_typed_folders: files_in_typed,
            files_in_root,
            avg_quality_score: avg_score,
            type_distribution: type_dist,
            readiness,
            next_steps,
        },
        suggestions,
    )
}

pub(super) fn cmd_scan_assess(
    repos: &[Repository],
    scanner: &Scanner,
    json_output: bool,
    quiet: bool,
    detailed: bool,
) -> anyhow::Result<()> {
    let mut repo_assessments = Vec::new();

    for repo in repos {
        let files = scanner.find_markdown_files(&repo.path);
        let mut file_assessments = Vec::new();

        for file in &files {
            if let Some(assessment) = assess_file(file, &repo.path) {
                file_assessments.push(assessment);
            }
        }

        repo_assessments.push(RepoAssessment {
            id: repo.id.clone(),
            name: repo.name.clone(),
            path: repo.path.display().to_string(),
            files: file_assessments,
        });
    }

    let (summary, structure_suggestions) = build_summary(&repo_assessments);

    let assessment = Assessment {
        repositories: repo_assessments,
        summary,
        structure_suggestions,
    };

    if json_output {
        println!("{}", format_json(&assessment)?);
    } else if !quiet {
        println!("Onboarding Assessment");
        println!("=====================\n");

        // File inventory
        println!(
            "Files: {} found, {} total",
            assessment.summary.total_files, assessment.summary.total_size_human
        );
        println!(
            "  With IDs: {}, With temporal tags: {}, With sources: {}",
            assessment.summary.files_with_ids,
            assessment.summary.files_with_temporal,
            assessment.summary.files_with_sources,
        );
        println!(
            "  In typed folders: {}, In root: {}",
            assessment.summary.files_in_typed_folders, assessment.summary.files_in_root,
        );
        println!(
            "  Average quality score: {:.0}/100\n",
            assessment.summary.avg_quality_score
        );

        // Type distribution
        if !assessment.summary.type_distribution.is_empty() {
            let mut types: Vec<_> = assessment.summary.type_distribution.iter().collect();
            types.sort_by(|a, b| b.1.cmp(a.1));
            println!("Document types:");
            for (t, c) in &types {
                println!("  {t}: {c}");
            }
            println!();
        }

        // Per-file details (if --detailed)
        if detailed {
            for repo in &assessment.repositories {
                if assessment.repositories.len() > 1 {
                    println!("Repository: {} ({})", repo.name, repo.id);
                }
                for f in &repo.files {
                    let temporal = if f.total_facts > 0 {
                        format!(
                            "{}/{} ({:.0}%)",
                            f.facts_with_temporal,
                            f.total_facts,
                            f.temporal_coverage * 100.0
                        )
                    } else {
                        "no facts".to_string()
                    };
                    let sources = if f.total_facts > 0 {
                        format!(
                            "{}/{} ({:.0}%)",
                            f.facts_with_sources,
                            f.total_facts,
                            f.source_coverage * 100.0
                        )
                    } else {
                        "no facts".to_string()
                    };
                    println!(
                        "  [{}] {} ({})",
                        f.quality_score, f.path, f.doc_type
                    );
                    println!(
                        "       ID: {} | Title: {} | Temporal: {} | Sources: {}",
                        if f.has_id_header { "✓" } else { "✗" },
                        if f.has_h1_title { "✓" } else { "✗" },
                        temporal,
                        sources,
                    );
                }
                println!();
            }
        }

        // Readiness
        println!("Readiness: {}\n", assessment.summary.readiness);

        // Structure suggestions
        if !assessment.structure_suggestions.is_empty() {
            println!("Structure:");
            for s in &assessment.structure_suggestions {
                println!("  • {s}");
            }
            println!();
        }

        // Next steps
        println!("Next steps:");
        for (i, step) in assessment.summary.next_steps.iter().enumerate() {
            println!("  {}. {step}", i + 1);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_file(dir: &Path, rel_path: &str, content: &str) {
        let path = dir.join(rel_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_assess_file_plain_markdown() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        create_file(root, "notes.md", "# My Notes\n\n- fact one\n- fact two\n");

        let result = assess_file(&root.join("notes.md"), root).unwrap();
        assert_eq!(result.title, "My Notes");
        assert!(!result.has_id_header);
        assert!(result.has_h1_title);
        assert_eq!(result.total_facts, 2);
        assert_eq!(result.facts_with_temporal, 0);
        assert_eq!(result.temporal_coverage, 0.0);
        assert!(!result.in_typed_folder);
        assert_eq!(result.doc_type, "document");
        // has_h1(15) + facts(15) = 30
        assert_eq!(result.quality_score, 30);
    }

    #[test]
    fn test_assess_file_well_formatted() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        create_file(
            root,
            "people/alice.md",
            "<!-- factbase:abc123 -->\n# Alice\n\n- Works at Acme @t[2020..] [^1]\n- Lives in NYC @t[=2023] [^1]\n\n---\n[^1]: LinkedIn, 2024-01\n",
        );

        let result = assess_file(&root.join("people/alice.md"), root).unwrap();
        assert!(result.has_id_header);
        assert!(result.has_h1_title);
        assert_eq!(result.total_facts, 2);
        assert_eq!(result.facts_with_temporal, 2);
        assert_eq!(result.facts_with_sources, 2);
        assert_eq!(result.temporal_coverage, 1.0);
        assert_eq!(result.source_coverage, 1.0);
        assert!(result.in_typed_folder);
        assert_eq!(result.doc_type, "people");
        // id(10) + h1(15) + facts(15) + temporal(25) + sources(25) + folder(10) = 100
        assert_eq!(result.quality_score, 100);
    }

    #[test]
    fn test_assess_file_partial_tags() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        create_file(
            root,
            "projects/atlas.md",
            "# Project Atlas\n\n- Started in 2020 @t[2020..]\n- Budget approved\n- Team of 5\n",
        );

        let result = assess_file(&root.join("projects/atlas.md"), root).unwrap();
        assert_eq!(result.total_facts, 3);
        assert_eq!(result.facts_with_temporal, 1);
        // coverage = 1/3 ≈ 0.333
        assert!((result.temporal_coverage - 0.333).abs() < 0.01);
        assert!(result.in_typed_folder);
        assert_eq!(result.doc_type, "project");
    }

    #[test]
    fn test_assess_file_no_facts() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        create_file(root, "readme.md", "# Welcome\n\nThis is a readme.\n");

        let result = assess_file(&root.join("readme.md"), root).unwrap();
        assert_eq!(result.total_facts, 0);
        assert_eq!(result.temporal_coverage, 0.0);
        assert_eq!(result.source_coverage, 0.0);
        // h1(15) only
        assert_eq!(result.quality_score, 15);
    }

    #[test]
    fn test_assess_file_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let result = assess_file(&tmp.path().join("nope.md"), tmp.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_build_summary_empty() {
        let (summary, suggestions) = build_summary(&[]);
        assert_eq!(summary.total_files, 0);
        assert_eq!(summary.readiness, "No markdown files found");
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_build_summary_mixed_files() {
        let repos = vec![RepoAssessment {
            id: "test".to_string(),
            name: "Test".to_string(),
            path: "/test".to_string(),
            files: vec![
                FileAssessment {
                    path: "people/alice.md".to_string(),
                    title: "Alice".to_string(),
                    doc_type: "people".to_string(),
                    size_bytes: 500,
                    has_id_header: true,
                    has_h1_title: true,
                    total_facts: 5,
                    facts_with_temporal: 5,
                    temporal_coverage: 1.0,
                    facts_with_sources: 5,
                    source_coverage: 1.0,
                    in_typed_folder: true,
                    quality_score: 100,
                },
                FileAssessment {
                    path: "notes.md".to_string(),
                    title: "Notes".to_string(),
                    doc_type: "document".to_string(),
                    size_bytes: 200,
                    has_id_header: false,
                    has_h1_title: true,
                    total_facts: 3,
                    facts_with_temporal: 0,
                    temporal_coverage: 0.0,
                    facts_with_sources: 0,
                    source_coverage: 0.0,
                    in_typed_folder: false,
                    quality_score: 30,
                },
            ],
        }];

        let (summary, suggestions) = build_summary(&repos);
        assert_eq!(summary.total_files, 2);
        assert_eq!(summary.files_with_ids, 1);
        assert_eq!(summary.files_with_temporal, 1);
        assert_eq!(summary.files_with_sources, 1);
        assert_eq!(summary.files_in_typed_folders, 1);
        assert_eq!(summary.files_in_root, 1);
        assert_eq!(summary.avg_quality_score, 65.0);
        assert_eq!(summary.type_distribution.get("people"), Some(&1));
        assert_eq!(summary.type_distribution.get("document"), Some(&1));

        // next_steps should mention scan, check, sources, and root files
        assert!(summary.next_steps.iter().any(|s| s.contains("scan")));
        assert!(summary.next_steps.iter().any(|s| s.contains("check")));
        assert!(summary.next_steps.iter().any(|s| s.contains("root-level")));

        // structure suggestions
        assert!(suggestions.iter().any(|s| s.contains("people")));
        assert!(suggestions.iter().any(|s| s.contains("root")));
    }

    #[test]
    fn test_build_summary_all_well_formatted() {
        let repos = vec![RepoAssessment {
            id: "test".to_string(),
            name: "Test".to_string(),
            path: "/test".to_string(),
            files: vec![FileAssessment {
                path: "people/bob.md".to_string(),
                title: "Bob".to_string(),
                doc_type: "people".to_string(),
                size_bytes: 300,
                has_id_header: true,
                has_h1_title: true,
                total_facts: 4,
                facts_with_temporal: 4,
                temporal_coverage: 1.0,
                facts_with_sources: 4,
                source_coverage: 1.0,
                in_typed_folder: true,
                quality_score: 100,
            }],
        }];

        let (summary, _) = build_summary(&repos);
        assert!(summary.readiness.contains("100%"));
        assert!(summary
            .next_steps
            .iter()
            .any(|s| s.contains("look good")));
    }

    #[test]
    fn test_assessment_json_serialization() {
        let assessment = Assessment {
            repositories: vec![],
            summary: AssessmentSummary {
                total_files: 0,
                total_size: 0,
                total_size_human: "0 B".to_string(),
                files_with_ids: 0,
                files_with_temporal: 0,
                files_with_sources: 0,
                files_in_typed_folders: 0,
                files_in_root: 0,
                avg_quality_score: 0.0,
                type_distribution: HashMap::new(),
                readiness: "No files".to_string(),
                next_steps: vec![],
            },
            structure_suggestions: vec![],
        };

        let json = serde_json::to_string(&assessment).unwrap();
        assert!(json.contains("\"total_files\":0"));
        assert!(json.contains("\"readiness\":\"No files\""));
    }

    #[test]
    fn test_cmd_scan_assess_integration() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        create_file(root, "people/alice.md", "# Alice\n\n- Fact @t[2024]\n");
        create_file(root, "notes.md", "# Notes\n\nSome text\n");

        let repo = Repository {
            id: "test".to_string(),
            name: "Test".to_string(),
            path: root.to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        let scanner = Scanner::new(&[]);

        // Should not error
        let result = cmd_scan_assess(&[repo], &scanner, false, true, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cmd_scan_assess_json() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        create_file(root, "doc.md", "# Doc\n\n- fact\n");

        let repo = Repository {
            id: "test".to_string(),
            name: "Test".to_string(),
            path: root.to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        let scanner = Scanner::new(&[]);

        // JSON mode should not error
        let result = cmd_scan_assess(&[repo], &scanner, true, false, false);
        assert!(result.is_ok());
    }
}
