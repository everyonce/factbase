//! Tests for TestContext perspective constructors.

mod common;

use common::TestContext;
use factbase::models::{Perspective, ReviewPerspective};
use std::collections::HashMap;

#[test]
fn test_context_with_perspective() {
    let perspective = Perspective {
        type_name: "knowledge-base".to_string(),
        organization: Some("Test Org".to_string()),
        focus: Some("Testing".to_string()),
        allowed_types: Some(vec!["person".to_string(), "project".to_string()]),
        review: None,
        format: None,
        link_match_mode: None,
        ..Default::default()
    };

    let ctx = TestContext::with_perspective("test-repo", perspective);

    // Verify perspective is stored in repository
    let repo = ctx
        .db
        .get_repository("test-repo")
        .expect("get repository")
        .expect("repository exists");

    let p = repo.perspective.expect("perspective exists");
    assert_eq!(p.type_name, "knowledge-base");
    assert_eq!(p.organization, Some("Test Org".to_string()));
    assert_eq!(p.focus, Some("Testing".to_string()));
    assert_eq!(
        p.allowed_types,
        Some(vec!["person".to_string(), "project".to_string()])
    );
}

#[test]
fn test_context_with_files_and_perspective() {
    let perspective = Perspective {
        type_name: "personal".to_string(),
        organization: None,
        focus: Some("Notes".to_string()),
        allowed_types: None,
        review: None,
        format: None,
        link_match_mode: None,
        ..Default::default()
    };

    let files = &[
        ("people/alice.md", "# Alice\n\nA person."),
        ("projects/alpha.md", "# Project Alpha\n\nA project."),
    ];

    let ctx = TestContext::with_files_and_perspective("test-repo", files, perspective);

    // Verify perspective is stored
    let repo = ctx
        .db
        .get_repository("test-repo")
        .expect("get repository")
        .expect("repository exists");

    let p = repo.perspective.expect("perspective exists");
    assert_eq!(p.type_name, "personal");
    assert_eq!(p.focus, Some("Notes".to_string()));

    // Verify files were created
    assert!(ctx.repo_path.join("people/alice.md").exists());
    assert!(ctx.repo_path.join("projects/alpha.md").exists());
}

#[test]
fn test_context_without_perspective() {
    let ctx = TestContext::new("test-repo");

    // Verify no perspective
    let repo = ctx
        .db
        .get_repository("test-repo")
        .expect("get repository")
        .expect("repository exists");

    assert!(repo.perspective.is_none());
}

#[test]
fn test_context_with_review_perspective() {
    let mut required_fields = HashMap::new();
    required_fields.insert(
        "person".to_string(),
        vec!["current_role".to_string(), "location".to_string()],
    );

    let review = ReviewPerspective {
        stale_days: Some(180),
        required_fields: Some(required_fields),
        ignore_patterns: Some(vec!["*.draft.md".to_string()]),
        glossary_types: None,
        source_types: None,
        stale_days_by_type: None,
    };

    let perspective = Perspective {
        type_name: "knowledge-base".to_string(),
        organization: Some("Test Org".to_string()),
        focus: None,
        allowed_types: None,
        review: Some(review),
        format: None,
        link_match_mode: None,
        ..Default::default()
    };

    let ctx = TestContext::with_perspective("test-repo", perspective);

    let repo = ctx
        .db
        .get_repository("test-repo")
        .expect("get repository")
        .expect("repository exists");

    let p = repo.perspective.expect("perspective exists");
    let r = p.review.expect("review config exists");

    assert_eq!(r.stale_days, Some(180));
    assert!(r.required_fields.is_some());
    let rf = r.required_fields.unwrap();
    assert_eq!(
        rf.get("person"),
        Some(&vec!["current_role".to_string(), "location".to_string()])
    );
    assert_eq!(r.ignore_patterns, Some(vec!["*.draft.md".to_string()]));
}

#[test]
fn test_review_perspective_default() {
    let review = ReviewPerspective::default();
    assert!(review.stale_days.is_none());
    assert!(review.required_fields.is_none());
    assert!(review.ignore_patterns.is_none());
}
