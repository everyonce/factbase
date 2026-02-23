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

/// Check if a word is a valid Roman numeral (I through MMMCMXCIX / 1-3999).
///
/// Standard ordinal suffixes like II, III, IV, VIII, XIV etc. appear in proper
/// nouns across many domains (monarchs, popes, ship classes, sequels) and should
/// not be flagged as undefined acronyms.
fn is_roman_numeral(s: &str) -> bool {
    use std::sync::LazyLock;
    static RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^M{0,3}(CM|CD|D?C{0,3})(XC|XL|L?X{0,3})(IX|IV|V?I{0,3})$")
            .expect("roman numeral regex should compile")
    });
    !s.is_empty() && RE.is_match(s)
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
        // Skip valid Roman numerals (ordinal suffixes in proper nouns)
        if is_roman_numeral(trimmed) {
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
        assert!(generate_ambiguous_questions("# Title\n\nSome paragraph text.").is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_location() {
        // Without context → flagged
        let q = generate_ambiguous_questions("# Person\n\n- Lives in San Francisco");
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].question_type, QuestionType::Ambiguous);
        assert!(q[0].description.contains("home, work, or another"));
        // With context → not flagged
        assert!(generate_ambiguous_questions("# Person\n\n- Lives in San Francisco (home)").is_empty());
        assert!(generate_ambiguous_questions("# Person\n\n- Based in NYC office").is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_relationship() {
        // Vague → flagged
        let q = generate_ambiguous_questions("# Person\n\n- Knows John Smith");
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("professional or personal"));
        // With context → not flagged
        assert!(generate_ambiguous_questions("# Person\n\n- Knows John Smith (colleague from Acme)").is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_works_with() {
        let q = generate_ambiguous_questions("# Person\n\n- Works with Jane Doe");
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("direct colleague, collaborator"));
        assert!(generate_ambiguous_questions("# Person\n\n- Works with Jane Doe as her manager").is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_met() {
        let q = generate_ambiguous_questions("# Person\n\n- Met Bob at a conference");
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("context"));
    }

    #[test]
    fn test_generate_ambiguous_questions_line_numbers_and_multiple() {
        let content = "# Person\n\n- Clear fact\n- Lives in Boston\n- Another clear fact";
        let q = generate_ambiguous_questions(content);
        assert_eq!(q[0].line_ref, Some(4));

        let q2 = generate_ambiguous_questions("# Person\n\n- Lives in NYC\n- Knows Jane");
        assert_eq!(q2.len(), 2);
    }

    #[test]
    fn test_detect_ambiguous_location() {
        for phrase in ["Lives in NYC", "Based in London", "Located in Paris", "Resides in Tokyo"] {
            assert!(detect_ambiguous_location(phrase).is_some(), "Should flag: {}", phrase);
        }
        for phrase in ["Lives in NYC (home)", "Based in London office", "Located in Paris headquarters", "Primary residence in Tokyo"] {
            assert!(detect_ambiguous_location(phrase).is_none(), "Should NOT flag: {}", phrase);
        }
    }

    #[test]
    fn test_detect_ambiguous_relationship() {
        for phrase in ["Knows John", "Connected to Jane", "Associated with Acme", "Works with Bob"] {
            assert!(detect_ambiguous_relationship(phrase).is_some(), "Should flag: {}", phrase);
        }
        for phrase in ["Knows John (colleague)", "Connected to Jane as mentor", "Works with Bob as his manager", "Met Jane, now a close friend"] {
            assert!(detect_ambiguous_relationship(phrase).is_none(), "Should NOT flag: {}", phrase);
        }
    }

    #[test]
    fn test_reviewed_marker_suppresses_ambiguous() {
        let today = Utc::now().date_naive();
        let marker_date = today - chrono::Duration::days(30);
        let content = format!("# Person\n\n- Lives in San Francisco <!-- reviewed:{} -->", marker_date.format("%Y-%m-%d"));
        assert!(generate_ambiguous_questions(&content).is_empty());
        // Old marker does NOT suppress
        assert_eq!(generate_ambiguous_questions("# Person\n\n- Lives in San Francisco <!-- reviewed:2020-01-01 -->").len(), 1);
    }

    #[test]
    fn test_acronym_detection() {
        // Unknown acronym flagged
        let q = generate_ambiguous_questions("# Company\n\n- Leading XYZQ expansion in healthcare");
        assert_eq!(q.len(), 1);
        assert!(q[0].description.contains("XYZQ"));

        // Known acronyms not flagged
        for acronym in ["CTO", "ECS", "RDS", "SOC", "TAM", "VPC", "SQS", "AWS"] {
            let content = format!("# Project\n\n- Uses {acronym} for deployment");
            let qs = generate_ambiguous_questions(&content);
            assert!(qs.iter().all(|q| !q.description.contains(acronym)), "{acronym} should not be flagged");
        }

        // Expanded acronym not flagged
        let q2 = generate_ambiguous_questions("# Company\n\n- Total Addressable Market (TAM) is $5B");
        assert!(q2.iter().all(|q| !q.description.contains("TAM")));

        // Short uppercase word not flagged
        let q3 = generate_ambiguous_questions("# Doc\n\n- Phase A of the project");
        assert!(q3.iter().all(|q| !q.description.contains("what does")));

        // Only first unknown acronym per line flagged
        let q4 = generate_ambiguous_questions("# Doc\n\n- Working on XYZQ and ABCD analysis");
        assert_eq!(q4.iter().filter(|q| q.description.contains("what does")).count(), 1);
    }

    #[test]
    fn test_extract_defined_terms() {
        // Bold pattern
        let c1 = "# Definitions\n\n## Acronyms\n- **TAM**: Total Addressable Market\n- **NPS**: Net Promoter Score\n";
        let t1 = extract_defined_terms(c1);
        assert!(t1.contains("TAM") && t1.contains("NPS"));
        // Heading pattern
        let c2 = "# Glossary\n\n## XYZQ\nSome custom term\n\n## ABCD\nAnother term\n";
        let t2 = extract_defined_terms(c2);
        assert!(t2.contains("XYZQ") && t2.contains("ABCD"));
        // Multi-word headings ignored
        assert!(extract_defined_terms("# Glossary\n\n## Some Phrase\nNot a term\n").is_empty());
    }

    #[test]
    fn test_defined_terms_suppress_acronym_questions() {
        let defined = HashSet::from(["XYZQ".to_string()]);
        // Defined term not flagged
        let q1 = generate_ambiguous_questions_with_type("# Company\n\n- Uses XYZQ for analytics", None, &defined);
        assert!(q1.iter().all(|q| !q.description.contains("XYZQ")));
        // Case-insensitive
        let defined2 = HashSet::from(["xyzq".to_string()]);
        let q2 = generate_ambiguous_questions_with_type("# Company\n\n- Uses XYZQ for analytics", None, &defined2);
        assert!(q2.iter().all(|q| !q.description.contains("XYZQ")));
        // Undefined term still flagged
        let q3 = generate_ambiguous_questions_with_type("# Company\n\n- Uses ABCD for analytics", None, &defined);
        assert_eq!(q3.iter().filter(|q| q.description.contains("ABCD")).count(), 1);
    }

    #[test]
    fn test_is_roman_numeral() {
        // Valid Roman numerals
        for s in ["II", "III", "IV", "VI", "VII", "VIII", "IX", "XI", "XII", "XIV", "XV", "XX", "XXI", "XL", "CD", "CM", "MM", "MCMX"] {
            assert!(is_roman_numeral(s), "{s} should be a Roman numeral");
        }
        // Not Roman numerals
        for s in ["", "A", "AB", "CEO", "AWS", "XYZQ", "IC", "VIP", "DM"] {
            assert!(!is_roman_numeral(s), "{s} should NOT be a Roman numeral");
        }
    }

    #[test]
    fn test_roman_numerals_not_flagged_as_acronyms() {
        // Roman numeral ordinals in proper nouns should not generate questions
        for name in ["Sargon II", "Tiglath-Pileser III", "Nebuchadnezzar II", "Henry VIII", "Louis XIV", "Elizabeth II", "Pope Pius XII"] {
            let content = format!("# Entity\n\n- {name} ruled the empire");
            let q = generate_ambiguous_questions(&content);
            assert!(
                q.iter().all(|q| !q.description.contains("what does")),
                "Roman numeral in '{name}' should not be flagged"
            );
        }
        // Non-Roman-numeral acronyms still flagged
        let q = generate_ambiguous_questions("# Entity\n\n- Joined XYZQ in 2020");
        assert!(q.iter().any(|q| q.description.contains("XYZQ")));
    }
}
