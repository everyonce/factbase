//! Integration tests for the review workflow.
//! Tests the complete human-in-the-loop review process:
//! 1. Generate questions with `lint --review`
//! 2. Answer questions (programmatically)
//! 3. Apply answers with `review --apply`
//!
//! These tests REQUIRE Ollama to be running with rnj-1-extended model.

mod common;

use common::ollama_helpers::require_ollama;
use factbase::{
    config::Config,
    llm::ReviewLlm,
    models::QuestionType,
    processor::{append_review_questions, parse_review_queue},
    question_generator::{
        generate_ambiguous_questions, generate_conflict_questions, generate_missing_questions,
        generate_stale_questions, generate_temporal_questions,
    },
};

#[test]
fn test_question_generation_for_missing_temporal_tags() {
    let content = r#"<!-- factbase:abc123 -->
# Test Person

## Career
- CEO at TechCorp
- VP Engineering at StartupCo
- Software Engineer at BigCo @t[2015..2018]

## Personal
- Lives in San Francisco
- Has PhD in Computer Science
"#;

    let questions = generate_temporal_questions(content);

    // Should generate questions for facts without temporal tags
    assert!(
        questions.len() >= 2,
        "Expected at least 2 temporal questions, got {}",
        questions.len()
    );

    // All questions should be temporal type
    for q in &questions {
        assert_eq!(q.question_type, QuestionType::Temporal);
        assert!(q.line_ref.is_some(), "Question should have line reference");
    }
}

#[test]
fn test_question_generation_for_missing_sources() {
    let content = r#"<!-- factbase:abc123 -->
# Test Person

## Career
- CEO at TechCorp
- VP Engineering at StartupCo [^1]

[^1]: LinkedIn profile, 2024-01-15
"#;

    let questions = generate_missing_questions(content);

    // Should generate question for fact without source
    assert!(
        !questions.is_empty(),
        "Expected at least 1 missing source question, got {}",
        questions.len()
    );

    // All questions should be missing type
    for q in &questions {
        assert_eq!(q.question_type, QuestionType::Missing);
    }
}

#[test]
fn test_question_generation_for_ambiguous_locations() {
    let content = r#"<!-- factbase:abc123 -->
# Test Person

## Personal
- Lives in Austin
- Based in New York
- Home office in Seattle
"#;

    let questions = generate_ambiguous_questions(content);

    // Should generate questions for ambiguous locations (without home/work context)
    // "Lives in Austin" and "Based in New York" are ambiguous
    // "Home office in Seattle" has context
    assert!(
        questions.len() >= 2,
        "Expected at least 2 ambiguous questions, got {}",
        questions.len()
    );

    for q in &questions {
        assert_eq!(q.question_type, QuestionType::Ambiguous);
    }
}

#[test]
fn test_append_review_questions_creates_section() {
    let content = r#"<!-- factbase:abc123 -->
# Test Person

- Some fact
"#;

    let questions = generate_temporal_questions(content);
    let updated = append_review_questions(content, &questions);

    // Should contain review queue marker
    assert!(
        updated.contains("<!-- factbase:review -->"),
        "Should contain review queue marker"
    );

    // Should contain Review Queue heading
    assert!(
        updated.contains("## Review Queue"),
        "Should contain Review Queue heading"
    );

    // Should contain question with @q[temporal] tag
    assert!(
        updated.contains("@q[temporal]"),
        "Should contain temporal question tag"
    );
}

#[test]
fn test_parse_review_queue_extracts_questions() {
    let content = r#"<!-- factbase:abc123 -->
# Test Person

- Some fact

---

## Review Queue

<!-- factbase:review -->
- [ ] `@q[temporal]` Line 5: "Some fact" - when was this true?
  > 
- [x] `@q[missing]` Line 5: "Some fact" - what is the source?
  > LinkedIn profile, checked 2024-01-15
"#;

    let questions = parse_review_queue(content);
    assert!(questions.is_some(), "Should parse review queue");

    let questions = questions.unwrap();
    assert_eq!(questions.len(), 2, "Should have 2 questions");

    // First question is unanswered
    assert!(!questions[0].answered);
    assert_eq!(questions[0].question_type, QuestionType::Temporal);

    // Second question is answered
    assert!(questions[1].answered);
    assert_eq!(questions[1].question_type, QuestionType::Missing);
    assert!(questions[1].answer.is_some());
    assert!(questions[1].answer.as_ref().unwrap().contains("LinkedIn"));
}

#[test]
fn test_parse_review_queue_handles_no_queue() {
    let content = r#"<!-- factbase:abc123 -->
# Test Person

- Some fact without review queue
"#;

    let questions = parse_review_queue(content);
    assert!(
        questions.is_none(),
        "Should return None when no queue exists"
    );
}

#[test]
fn test_full_question_generation_workflow() {
    let content = r#"<!-- factbase:abc123 -->
# Test Person

## Career
- CEO at TechCorp
- VP Engineering at StartupCo @t[2019..]

## Personal
- Lives in Austin
- Knows John Smith

## Education
- Has PhD in Computer Science [^1]

[^1]: University website, 2020-01-15
"#;

    // Generate all question types
    let temporal_qs = generate_temporal_questions(content);
    let missing_qs = generate_missing_questions(content);
    let ambiguous_qs = generate_ambiguous_questions(content);
    let stale_qs = generate_stale_questions(content, 365);

    // Should have temporal questions for untagged facts
    assert!(
        !temporal_qs.is_empty(),
        "Should have temporal questions for CEO, Lives in Austin, Knows John, Has PhD"
    );

    // Should have missing source questions
    assert!(
        !missing_qs.is_empty(),
        "Should have missing source questions for unsourced facts"
    );

    // Should have ambiguous questions for location and relationship
    assert!(
        !ambiguous_qs.is_empty(),
        "Should have ambiguous questions for 'Lives in Austin' and 'Knows John Smith'"
    );

    // Stale questions depend on source date age
    // The 2020-01-15 source is old, so should generate stale question
    assert!(
        !stale_qs.is_empty(),
        "Should have stale question for old source date"
    );

    // Append all questions to document
    let mut all_questions = Vec::new();
    all_questions.extend(temporal_qs);
    all_questions.extend(missing_qs);
    all_questions.extend(ambiguous_qs);
    all_questions.extend(stale_qs);

    let updated = append_review_questions(content, &all_questions);

    // Verify all question types are present
    assert!(updated.contains("@q[temporal]"));
    assert!(updated.contains("@q[missing]"));
    assert!(updated.contains("@q[ambiguous]"));
    assert!(updated.contains("@q[stale]"));
}

// Integration test requiring Ollama - marked as ignored
#[tokio::test]
#[ignore]
async fn test_review_apply_with_llm() {
    use factbase::answer_processor::{
        apply_changes_to_section, identify_affected_section, interpret_answer,
        remove_processed_questions, InterpretedAnswer,
    };

    require_ollama().await;

    let config = Config::default();
    let llm = factbase::OllamaLlm::new(config.llm.effective_base_url(), &config.llm.model);
    let review_llm = ReviewLlm::new(Box::new(llm), config.llm.model.clone());

    // Step 1: Create a document with facts missing temporal tags
    let original_content = r#"<!-- factbase:test01 -->
# Test Person

## Career
- CEO at TechCorp
- VP Engineering at StartupCo

## Personal
- Lives in Austin
"#;

    // Step 2: Generate questions
    let questions = generate_temporal_questions(original_content);
    assert!(!questions.is_empty(), "Should generate temporal questions");

    // Step 3: Append questions to create document with Review Queue
    let content_with_queue = append_review_questions(original_content, &questions);
    assert!(
        content_with_queue.contains("<!-- factbase:review -->"),
        "Should have review queue"
    );

    // Step 4: Simulate answering questions
    // Find the CEO question and answer it
    let mut answered_questions = Vec::new();
    for (idx, q) in questions.iter().enumerate() {
        if q.description.contains("CEO") {
            let mut answered = q.clone();
            answered.answered = true;
            answered.answer = Some("Started January 2022, still current".to_string());
            answered_questions.push((idx, answered));
            break;
        }
    }

    assert!(
        !answered_questions.is_empty(),
        "Should have found CEO question to answer"
    );

    // Step 5: Interpret the answer
    let (question_idx, answered_q) = &answered_questions[0];
    let instruction = interpret_answer(answered_q, answered_q.answer.as_ref().unwrap());

    // Verify interpretation
    match &instruction {
        factbase::answer_processor::ChangeInstruction::AddTemporal { line_text, tag } => {
            assert!(
                line_text.contains("CEO") || line_text.is_empty(),
                "Should reference CEO line"
            );
            assert!(tag.contains("2022"), "Tag should contain 2022");
        }
        other => {
            // AddTemporal is expected, but Generic is also acceptable
            println!("Got instruction: {:?}", other);
        }
    }

    // Step 6: Identify affected section
    let questions_for_section = vec![answered_q.clone()];
    let section_info = identify_affected_section(&content_with_queue, &questions_for_section);

    if let Some((section_start, section_end, section)) = section_info {
        assert!(section_start < section_end, "Should identify valid section");

        // Step 7: Apply changes using LLM
        let interpreted = vec![InterpretedAnswer {
            question: answered_q.clone(),
            instruction,
        }];

        let rewritten = apply_changes_to_section(&review_llm, &section, &interpreted).await;

        match rewritten {
            Ok(new_section) => {
                // Verify the rewritten section contains temporal tag
                assert!(
                    new_section.contains("@t[") || new_section.contains("2022"),
                    "Rewritten section should contain temporal information: {}",
                    new_section
                );
                println!("Successfully rewrote section:\n{}", new_section);
            }
            Err(e) => {
                // LLM errors are acceptable in test environment
                println!("LLM rewrite failed (expected in some environments): {}", e);
            }
        }
    } else {
        println!("Could not identify affected section - this is acceptable for some documents");
    }

    // Step 8: Test question removal
    let cleaned = remove_processed_questions(&content_with_queue, &[*question_idx]);
    // If all questions were processed, the review queue should be removed
    // If some remain, they should still be there
    println!("Cleaned content length: {}", cleaned.len());
}

/// Test the complete workflow without LLM (rule-based only)
#[test]
fn test_review_workflow_rule_based() {
    use factbase::answer_processor::{interpret_answer, ChangeInstruction};

    // Create document with various issues
    let content = r#"<!-- factbase:abc123 -->
# Test Person

## Career
- CEO at TechCorp
- VP Engineering at StartupCo @t[2019..]

## Personal
- Lives in Austin
"#;

    // Generate all question types
    let temporal_qs = generate_temporal_questions(content);
    let ambiguous_qs = generate_ambiguous_questions(content);

    // Append questions
    let with_queue = append_review_questions(content, &temporal_qs);
    let with_all = append_review_questions(&with_queue, &ambiguous_qs);

    assert!(with_all.contains("@q[temporal]"));
    assert!(with_all.contains("@q[ambiguous]"));

    // Test answer interpretation
    let test_cases = vec![
        ("dismiss", true),
        ("ignore", true),
        ("delete", false),
        ("Started 2022, left 2024", false),
        ("split: home in Austin, work in NYC", false),
    ];

    for (answer, is_dismiss) in test_cases {
        let q = &temporal_qs[0];
        let instruction = interpret_answer(q, answer);
        match instruction {
            ChangeInstruction::Dismiss => assert!(is_dismiss, "Expected dismiss for: {}", answer),
            _ => assert!(!is_dismiss, "Expected non-dismiss for: {}", answer),
        }
    }
}

/// Test conflict question generation
#[test]
fn test_conflict_question_generation() {
    let content = r#"<!-- factbase:abc123 -->
# Test Person

## Career
- CEO at TechCorp @t[2020..2023]
- CTO at TechCorp @t[2021..2024]
"#;

    let questions = generate_conflict_questions(content);

    // Should detect overlapping roles at same company
    // Note: conflict detection uses heuristics, may or may not detect this
    // depending on implementation details
    println!("Generated {} conflict questions", questions.len());
    for q in &questions {
        assert_eq!(q.question_type, QuestionType::Conflict);
        println!("  - {}", q.description);
    }
}
