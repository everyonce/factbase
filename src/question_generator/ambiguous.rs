//! Ambiguous question generation.
//!
//! Generates `@q[ambiguous]` questions for unclear phrasing
//! that needs clarification.

use std::collections::HashSet;

use chrono::Utc;

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::extract_reviewed_date;

use super::iter_fact_lines;

/// Default number of days a reviewed marker suppresses question regeneration.
const REVIEWED_SKIP_DAYS: i64 = 180;

/// Extract defined terms from a definitions/glossary document.
///
/// Parses `**TERM**:` patterns (the standard definitions file format) and
/// `## TERM` headings, returning the set of defined terms.
pub fn extract_defined_terms(content: &str) -> HashSet<String> {
    let mut terms = HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim().trim_start_matches("- ");
        // Match **TERM**: definition pattern
        if let Some(rest) = trimmed.strip_prefix("**") {
            if let Some(end) = rest.find("**") {
                let term = rest[..end].trim();
                if !term.is_empty() {
                    terms.insert(term.to_string());
                }
            }
        }
        // Match ## TERM heading pattern
        if let Some(heading) = trimmed.strip_prefix("## ") {
            let term = heading.trim();
            if !term.is_empty() && !term.contains(' ') {
                terms.insert(term.to_string());
            }
        }
    }
    terms
}

/// Generate ambiguous questions for a document.
///
/// Detects facts with unclear phrasing that needs clarification:
/// 1. Locations without context (could be home, work, or other)
/// 2. Relationships without direction (e.g., "knows John" - professional or personal?)
/// 3. Vague pronouns or references
///
/// Returns a list of `ReviewQuestion` with `question_type = Ambiguous`.
pub fn generate_ambiguous_questions(content: &str) -> Vec<ReviewQuestion> {
    generate_ambiguous_questions_with_type(content, None, &HashSet::new())
}

/// Generate ambiguous questions, optionally skipping acronym detection for definition documents.
///
/// `defined_terms` contains terms from definitions files in the repo that should
/// not be flagged as undefined acronyms.
pub fn generate_ambiguous_questions_with_type(
    content: &str,
    doc_type: Option<&str>,
    defined_terms: &HashSet<String>,
) -> Vec<ReviewQuestion> {
    let mut questions = Vec::new();
    let today = Utc::now().date_naive();

    // Skip acronym detection for glossary/definition documents
    let skip_acronyms = doc_type.is_some_and(|t| {
        let lower = t.to_lowercase();
        lower == "definition" || lower == "glossary"
    }) || content
        .lines()
        .take(3)
        .any(|l| {
            let lower = l.to_lowercase();
            lower.contains("# glossary") || lower.contains("# definitions")
        });

    for (line_number, line, fact_text) in iter_fact_lines(content) {
        // Skip facts with a recent reviewed marker
        if extract_reviewed_date(line).is_some_and(|d| (today - d).num_days() <= REVIEWED_SKIP_DAYS)
        {
            continue;
        }

        // Check for ambiguous location (no context like "home", "work", "office")
        let ambiguity = detect_ambiguous_location(&fact_text)
            .or_else(|| detect_ambiguous_relationship(&fact_text))
            .map(String::from)
            .or_else(|| {
                if skip_acronyms {
                    None
                } else {
                    detect_undefined_acronym(&fact_text, defined_terms)
                }
            });

        if let Some(ambiguity) = ambiguity {
            questions.push(ReviewQuestion::new(
                QuestionType::Ambiguous,
                Some(line_number),
                format!("\"{fact_text}\" - {ambiguity}"),
            ));
        }
    }

    questions
}

/// Detect ambiguous location references.
/// Returns Some(clarification_question) if ambiguous, None otherwise.
fn detect_ambiguous_location(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();

    // Location phrases that need context
    let location_phrases = ["lives in", "based in", "located in", "resides in"];

    // Context words that clarify the location type
    let context_words = [
        "home",
        "work",
        "office",
        "headquarters",
        "hq",
        "remote",
        "primary",
        "secondary",
        "main",
    ];

    // Check if it's a location statement
    let is_location = location_phrases.iter().any(|p| lower.contains(p));
    if !is_location {
        return None;
    }

    // Check if context is provided
    let has_context = context_words.iter().any(|c| lower.contains(c));
    if has_context {
        return None;
    }

    Some("is this home, work, or another type of location?")
}

/// Detect ambiguous relationship references.
/// Returns Some(clarification_question) if ambiguous, None otherwise.
fn detect_ambiguous_relationship(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();

    // Vague relationship phrases
    let vague_relationships = [
        ("knows ", "is this a professional or personal relationship?"),
        ("connected to ", "what is the nature of this connection?"),
        (
            "associated with ",
            "what is the nature of this association?",
        ),
        (
            "works with ",
            "is this a direct colleague, collaborator, or other?",
        ),
        ("met ", "in what context did they meet?"),
    ];

    for (phrase, question) in vague_relationships {
        if lower.contains(phrase) {
            // Check if there's already clarifying context
            let clarifiers = [
                "colleague",
                "friend",
                "mentor",
                "manager",
                "report",
                "client",
                "partner",
                "investor",
                "advisor",
                "board",
                "professional",
                "personal",
            ];
            let has_clarifier = clarifiers.iter().any(|c| lower.contains(c));
            if !has_clarifier {
                return Some(question);
            }
        }
    }

    None
}

/// Detect undefined acronyms/abbreviations that could have multiple meanings.
///
/// Flags uppercase sequences (2-5 chars) that aren't preceded by their expansion
/// in the same line or a nearby heading. Common well-known acronyms are excluded.
fn detect_undefined_acronym(text: &str, defined_terms: &HashSet<String>) -> Option<String> {
    // Well-known acronyms that don't need definition in a knowledge base context.
    // Includes business, tech, cloud/AWS, and general industry terms.
    static KNOWN: &[&str] = &[
        // Business & titles
        "CEO", "CTO", "CFO", "COO", "CMO", "CIO", "CISO", "CPO", "CRO", "CSO",
        "VP", "SVP", "EVP", "MD", "PhD", "MBA", "BS", "BA", "MS", "JD",
        "HR", "IT", "PM", "AM",
        "IPO", "LLC", "INC", "LTD", "PLC", "AG",
        "Q1", "Q2", "Q3", "Q4", "YoY", "QoQ", "MoM",
        "KPI", "OKR", "ROI", "P&L", "R&D", "M&A",
        "SaaS", "PaaS", "IaaS", "B2B", "B2C", "B2G", "D2C",
        "PR", "IR", "VC", "PE", "LP", "GP",
        "ARR", "MRR", "GMV", "TAM", "SAM", "SOM", "NPS", "CAC", "LTV", "EBITDA",
        "FTE", "PTO", "WFH", "RTO", "OOO",
        // Geography & general
        "US", "USA", "UK", "EU", "UN", "NATO",
        "USD", "EUR", "GBP", "JPY", "CAD", "AUD",
        "NYC", "SF", "LA", "DC", "HQ",
        "NA", "EMEA", "APAC", "LATAM", "AMER", "ANZ", "DACH",
        "ASAP", "TBD", "TBA", "ID", "OK", "ETA", "EOD", "COB",
        // Core tech
        "AI", "ML", "LLM", "NLP", "GPU", "CPU", "RAM", "SSD", "HDD",
        "API", "SDK", "CLI", "GUI", "IDE", "URL", "URI",
        "SQL", "DB", "ORM", "ETL", "ELT",
        "DNS", "HTTP", "HTTPS", "SSH", "TCP", "IP", "UDP", "TLS", "SSL",
        "REST", "RPC", "gRPC", "MQTT", "AMQP",
        "PDF", "CSV", "JSON", "YAML", "XML", "HTML", "CSS", "JS", "TS",
        "CI", "CD", "QA", "UAT", "SLA", "SLO", "SLI",
        "OS", "VM", "VPN", "SSO", "MFA", "RBAC", "IAM", "LDAP", "SAML",
        "CRUD", "CQRS", "DDD", "TDD", "BDD", "OOP",
        "JWT", "OAuth", "OIDC",
        "CIDR", "VLAN", "BGP", "CDN", "WAF",
        // AWS services & terms
        "AWS", "EC2", "S3", "RDS", "ECS", "EKS", "ELB", "ALB", "NLB",
        "VPC", "SNS", "SQS", "SES", "DMS", "KMS", "ACM",
        "EMR", "MSK", "MQ", "DAX", "DDB",
        "EBS", "EFS", "FSx",
        "ECR", "EKS", "ECS",
        "WAF", "ACL", "NAT", "IGW",
        "SSM", "ASG", "AMI", "AZ", "ARN",
        // Other cloud & infra
        "GCP", "GKE", "GCE", "GCS",
        "K8s", "CNCF", "OCI", "WASM",
        "SOC", "PCI", "DSS", "HIPAA", "GDPR", "SOX", "FedRAMP",
    ];

    // Find uppercase sequences of 2-5 chars that look like acronyms
    let mut found: Option<&str> = None;
    for word in text.split(|c: char| !c.is_alphanumeric() && c != '&') {
        let trimmed = word.trim();
        if trimmed.len() < 2 || trimmed.len() > 5 {
            continue;
        }
        // Must be mostly uppercase letters (allow digits like "S3" or "&" like "P&L")
        let alpha_chars: Vec<char> = trimmed.chars().filter(|c| c.is_alphabetic()).collect();
        if alpha_chars.len() < 2 || !alpha_chars.iter().all(|c| c.is_uppercase()) {
            continue;
        }
        if KNOWN.iter().any(|k| k.eq_ignore_ascii_case(trimmed)) {
            continue;
        }
        // Skip terms defined in the repo's definitions files
        if defined_terms.iter().any(|t| t.eq_ignore_ascii_case(trimmed)) {
            continue;
        }
        // Check if the expansion appears in the same line (e.g., "Total Addressable Market (TAM)")
        let lower = text.to_lowercase();
        let acronym_lower = trimmed.to_lowercase();
        if lower.contains(&format!("({acronym_lower})"))
            || lower.contains(&format!("({trimmed})"))
        {
            continue;
        }
        found = Some(trimmed);
        break; // One per fact line
    }

    found.map(|acronym| format!("what does \"{acronym}\" mean in this context?"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ambiguous_questions_no_facts() {
        let content = "# Title\n\nSome paragraph text.";
        let questions = generate_ambiguous_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_location_without_context() {
        let content = "# Person\n\n- Lives in San Francisco";
        let questions = generate_ambiguous_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Ambiguous);
        assert!(questions[0].description.contains("home, work, or another"));
    }

    #[test]
    fn test_generate_ambiguous_questions_location_with_context() {
        let content = "# Person\n\n- Lives in San Francisco (home)";
        let questions = generate_ambiguous_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_location_work_context() {
        let content = "# Person\n\n- Based in NYC office";
        let questions = generate_ambiguous_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_relationship_vague() {
        let content = "# Person\n\n- Knows John Smith";
        let questions = generate_ambiguous_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Ambiguous);
        assert!(questions[0]
            .description
            .contains("professional or personal"));
    }

    #[test]
    fn test_generate_ambiguous_questions_relationship_with_context() {
        let content = "# Person\n\n- Knows John Smith (colleague from Acme)";
        let questions = generate_ambiguous_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_works_with_vague() {
        let content = "# Person\n\n- Works with Jane Doe";
        let questions = generate_ambiguous_questions(content);
        assert_eq!(questions.len(), 1);
        assert!(questions[0]
            .description
            .contains("direct colleague, collaborator"));
    }

    #[test]
    fn test_generate_ambiguous_questions_works_with_context() {
        let content = "# Person\n\n- Works with Jane Doe as her manager";
        let questions = generate_ambiguous_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_met_vague() {
        let content = "# Person\n\n- Met Bob at a conference";
        let questions = generate_ambiguous_questions(content);
        // "at a conference" provides context, but we still ask for more detail
        assert_eq!(questions.len(), 1);
        assert!(questions[0].description.contains("context"));
    }

    #[test]
    fn test_generate_ambiguous_questions_line_numbers() {
        let content = "# Person\n\n- Clear fact\n- Lives in Boston\n- Another clear fact";
        let questions = generate_ambiguous_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].line_ref, Some(4));
    }

    #[test]
    fn test_generate_ambiguous_questions_multiple() {
        let content = "# Person\n\n- Lives in NYC\n- Knows Jane";
        let questions = generate_ambiguous_questions(content);
        assert_eq!(questions.len(), 2);
    }

    #[test]
    fn test_detect_ambiguous_location_various_phrases() {
        assert!(detect_ambiguous_location("Lives in NYC").is_some());
        assert!(detect_ambiguous_location("Based in London").is_some());
        assert!(detect_ambiguous_location("Located in Paris").is_some());
        assert!(detect_ambiguous_location("Resides in Tokyo").is_some());
    }

    #[test]
    fn test_detect_ambiguous_location_with_clarifiers() {
        assert!(detect_ambiguous_location("Lives in NYC (home)").is_none());
        assert!(detect_ambiguous_location("Based in London office").is_none());
        assert!(detect_ambiguous_location("Located in Paris headquarters").is_none());
        assert!(detect_ambiguous_location("Primary residence in Tokyo").is_none());
    }

    #[test]
    fn test_detect_ambiguous_relationship_various() {
        assert!(detect_ambiguous_relationship("Knows John").is_some());
        assert!(detect_ambiguous_relationship("Connected to Jane").is_some());
        assert!(detect_ambiguous_relationship("Associated with Acme").is_some());
        assert!(detect_ambiguous_relationship("Works with Bob").is_some());
    }

    #[test]
    fn test_detect_ambiguous_relationship_with_clarifiers() {
        assert!(detect_ambiguous_relationship("Knows John (colleague)").is_none());
        assert!(detect_ambiguous_relationship("Connected to Jane as mentor").is_none());
        assert!(detect_ambiguous_relationship("Works with Bob as his manager").is_none());
        assert!(detect_ambiguous_relationship("Met Jane, now a close friend").is_none());
    }

    #[test]
    fn test_reviewed_marker_suppresses_ambiguous() {
        let today = Utc::now().date_naive();
        let marker_date = today - chrono::Duration::days(30);
        let content = format!(
            "# Person\n\n- Lives in San Francisco <!-- reviewed:{} -->",
            marker_date.format("%Y-%m-%d")
        );
        let questions = generate_ambiguous_questions(&content);
        assert!(
            questions.is_empty(),
            "Recent reviewed marker should suppress ambiguous question"
        );
    }

    #[test]
    fn test_old_reviewed_marker_still_generates_ambiguous() {
        let content = "# Person\n\n- Lives in San Francisco <!-- reviewed:2020-01-01 -->";
        let questions = generate_ambiguous_questions(content);
        assert_eq!(
            questions.len(),
            1,
            "Old reviewed marker should not suppress ambiguous question"
        );
    }

    // ==================== Acronym Detection Tests ====================

    #[test]
    fn test_undefined_acronym_flagged() {
        // XYZQ is not in KNOWN or any definitions file
        let content = "# Company\n\n- Leading XYZQ expansion in healthcare";
        let questions = generate_ambiguous_questions(content);
        assert_eq!(questions.len(), 1);
        assert!(questions[0].description.contains("XYZQ"));
        assert!(questions[0].description.contains("what does"));
    }

    #[test]
    fn test_known_acronym_not_flagged() {
        let content = "# Person\n\n- CTO at Acme Corp @t[2024..]";
        let questions = generate_ambiguous_questions(content);
        assert!(
            questions.is_empty(),
            "CTO is a well-known acronym, should not be flagged"
        );
    }

    #[test]
    fn test_builtin_cloud_acronyms_not_flagged() {
        // These were previously flagged but are now in the expanded KNOWN list
        for acronym in &["ECS", "RDS", "SOC", "TAM", "VPC", "SQS", "EKS", "ALB"] {
            let content = format!("# Project\n\n- Uses {acronym} for deployment");
            let questions = generate_ambiguous_questions(&content);
            let acronym_q: Vec<_> = questions
                .iter()
                .filter(|q| q.description.contains(acronym))
                .collect();
            assert!(
                acronym_q.is_empty(),
                "{acronym} should be in the built-in known list"
            );
        }
    }

    #[test]
    fn test_expanded_acronym_not_flagged() {
        let content =
            "# Company\n\n- Total Addressable Market (TAM) is $5B";
        let questions = generate_ambiguous_questions(content);
        let acronym_q: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("TAM"))
            .collect();
        assert!(acronym_q.is_empty(), "Expanded acronym should not be flagged");
    }

    #[test]
    fn test_short_uppercase_word_not_flagged() {
        // Single uppercase letter or very common patterns
        let content = "# Doc\n\n- Phase A of the project";
        let questions = generate_ambiguous_questions(content);
        let acronym_q: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("what does"))
            .collect();
        assert!(acronym_q.is_empty(), "Single letter should not be flagged");
    }

    #[test]
    fn test_multiple_acronyms_only_first_flagged() {
        // Use unknown acronyms since TAM/SAM are now in KNOWN
        let content = "# Doc\n\n- Working on XYZQ and ABCD analysis";
        let questions = generate_ambiguous_questions(content);
        let acronym_q: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("what does"))
            .collect();
        assert_eq!(acronym_q.len(), 1, "Only one acronym question per fact line");
    }

    #[test]
    fn test_aws_not_flagged() {
        let content = "# Project\n\n- Deployed on AWS infrastructure";
        let questions = generate_ambiguous_questions(content);
        let acronym_q: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("AWS"))
            .collect();
        assert!(acronym_q.is_empty());
    }

    // ==================== Definitions-Aware Tests ====================

    #[test]
    fn test_extract_defined_terms_bold_pattern() {
        let content = "# Definitions: Business Terms\n\n## Acronyms\n- **TAM**: Total Addressable Market\n- **NPS**: Net Promoter Score\n";
        let terms = extract_defined_terms(content);
        assert!(terms.contains("TAM"));
        assert!(terms.contains("NPS"));
    }

    #[test]
    fn test_extract_defined_terms_heading_pattern() {
        let content = "# Glossary\n\n## XYZQ\nSome custom term\n\n## ABCD\nAnother term\n";
        let terms = extract_defined_terms(content);
        assert!(terms.contains("XYZQ"));
        assert!(terms.contains("ABCD"));
    }

    #[test]
    fn test_extract_defined_terms_ignores_multi_word_headings() {
        let content = "# Glossary\n\n## Some Phrase\nNot a term\n";
        let terms = extract_defined_terms(content);
        assert!(terms.is_empty());
    }

    #[test]
    fn test_defined_term_not_flagged() {
        let defined = HashSet::from(["XYZQ".to_string()]);
        let content = "# Company\n\n- Uses XYZQ for analytics";
        let questions = generate_ambiguous_questions_with_type(content, None, &defined);
        let acronym_q: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("XYZQ"))
            .collect();
        assert!(
            acronym_q.is_empty(),
            "Term defined in definitions file should not be flagged"
        );
    }

    #[test]
    fn test_defined_term_case_insensitive() {
        let defined = HashSet::from(["xyzq".to_string()]);
        let content = "# Company\n\n- Uses XYZQ for analytics";
        let questions = generate_ambiguous_questions_with_type(content, None, &defined);
        let acronym_q: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("XYZQ"))
            .collect();
        assert!(
            acronym_q.is_empty(),
            "Defined term matching should be case-insensitive"
        );
    }

    #[test]
    fn test_undefined_term_still_flagged_with_definitions() {
        let defined = HashSet::from(["XYZQ".to_string()]);
        let content = "# Company\n\n- Uses ABCD for analytics";
        let questions = generate_ambiguous_questions_with_type(content, None, &defined);
        let acronym_q: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("ABCD"))
            .collect();
        assert_eq!(
            acronym_q.len(),
            1,
            "Undefined term should still be flagged even when other terms are defined"
        );
    }
}
