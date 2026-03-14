//! Citation quality scoring — three-tier validator.
//!
//! Tier 1 (this module): Rust validator — detect source type, check structural requirements.
//! Tier 2: Batch LLM review (maintain workflow step 4).
//! Tier 3: Deferred for human review.
//!
//! Key rule: if the source type supports direct navigation (URL, record ID, verse reference),
//! REQUIRE it. Don't accept a tool name + date when a URL is possible.

use regex::Regex;
use std::sync::LazyLock;

// --- Regexes ---

static URL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://").expect("url regex"));

static FILE_PATH_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[\s(])(?:[/~][^\s]+|[^\s]+\.(?:md|pdf|doc|docx|txt|csv|xlsx|html))\b")
        .expect("file path regex")
});

static PAGE_SECTION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:pp?\.\s*\d|§\s*\d|[Ss]ection\s+\d|[Cc]hapter\s+\d|[Vv]erse\s+\d)")
        .expect("page section regex")
});

/// Standard identifiers: ISBN, DOI, RFC, ISSN, arXiv, PMID, CVE, or numbered ticket/observation
static IDENTIFIER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:ISBN|DOI|RFC|ISSN|arXiv|PMID|CVE)[\s:-]+[\w./-]+|(?:#|observation\s*#?|ticket\s*#?|thread\s*#?|issue\s*#?)\s*\d+")
        .expect("identifier regex")
});

static DATE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d{4}-\d{2}(?:-\d{2})?").expect("date regex"));

/// Domain-style URL without protocol (e.g. "linkedin.com/in/username", "github.com/org/repo")
static DOMAIN_URL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b[a-zA-Z0-9][a-zA-Z0-9-]*\.[a-zA-Z]{2,}/[^\s,]+").expect("domain url regex")
});

/// Catalog/record number: 1-4 uppercase letters + separator + 2+ digits (e.g. "CL 1355", "A-77", "SD 1361")
static CATALOG_NUMBER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b[A-Z]{1,4}[-\s]\d{2,}\b").expect("catalog number regex")
});

static NAMED_PERSON_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:with|from|by|interview)\s+[A-Z][a-z]+(?:\s+[A-Z][a-z]+)+")
        .expect("named person regex")
});

/// Scripture: book + chapter:verse (e.g. "Genesis 1:1", "John 3:16")
static SCRIPTURE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[A-Z][a-z]+\s+\d+:\d+").expect("scripture regex")
});

/// Academic: author + year + venue (e.g. "Smith 2024, Nature 612:45" or "Smith et al. 2024")
/// Matches: single capitalized word (non-month) + year, or "et al." pattern.
static ACADEMIC_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:[A-Z][a-z]+\s+et\s+al\.?\s+\d{4}|[A-Z][a-z]+(?:\s+[A-Z][a-z]+)*\s+\d{4})")
        .expect("academic regex")
});

/// Month names to exclude from academic detection
static MONTH_NAMES: &[&str] = &[
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
];

/// Known tool names that require a URL (navigable tools)
static KNOWN_TOOL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:phonetool|linkedin|github|jira|confluence|notion|salesforce|workday|servicenow|zendesk|datadog|splunk|tableau|looker|grafana|pagerduty|okta|slack|teams|zoom|google\s+docs?|sharepoint|dropbox|box\.com|figma|miro|asana|trello|monday\.com)\b")
        .expect("known tool regex")
});

/// System/DB: system name + record ID pattern (e.g. "Jira PROJ-678", "ServiceNow INC0012345")
static SYSTEM_ID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[A-Z][A-Z0-9_-]+[-_]\d+\b").expect("system id regex")
});

/// Email keywords
static EMAIL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bemail\b").expect("email regex"));

/// Meeting/call/conversation keywords
static CONVERSATION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:meeting|call|conversation|discussion|interview|one-on-one|standup|sync)\b")
        .expect("conversation regex")
});

/// Book/publication keywords
static BOOK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?:book|guide|manual|handbook|textbook|edition|published|press|publisher|journal|proceedings|conference|workshop|symposium|volume|vol\.|pp?\.|chapter|ch\.)\b"#)
        .expect("book regex")
});

/// Standard body keywords (RFC, ISO, IEEE, NIST, ANSI, etc.)
static STANDARD_BODY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:RFC|ISO|IEEE|NIST|ANSI|IETF|W3C|OWASP|PCI|HIPAA|GDPR|SOC|FedRAMP)\b")
        .expect("standard body regex")
});

/// Standard number pattern (e.g. "RFC 7231", "ISO 27001", "IEEE 802.11")
static STANDARD_NUMBER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:RFC|ISO|IEEE|NIST|ANSI|IETF|W3C)\s+[\d.]+\b").expect("standard number regex")
});

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// The detected type of a citation source.
#[derive(Debug, Clone, PartialEq)]
pub enum CitationType {
    /// URL present — already navigable, no further check needed.
    Url,
    /// File path present — already navigable.
    FilePath,
    /// Known navigable tool (Phonetool, LinkedIn, Jira, etc.) — URL required.
    NavigableTool,
    /// Book, guide, or publication — page/chapter/section required.
    Book,
    /// Slack or Teams message — channel + author + date required.
    SlackOrTeams,
    /// Email — subject + sender + date required.
    Email,
    /// System or database record (Jira, ServiceNow, etc.) — record ID required.
    SystemOrDb,
    /// Standards body document (RFC, ISO, IEEE) — body + number required.
    Standard,
    /// Scripture reference — book + chapter:verse required.
    Scripture,
    /// Academic paper — author + venue + year required.
    Academic,
    /// Meeting, call, or conversation — participants + date required.
    Conversation,
    /// Doesn't match any recognized type.
    Unknown,
}

/// Detect the source type from citation text.
pub fn detect_citation_type(text: &str) -> CitationType {
    if URL_REGEX.is_match(text) {
        return CitationType::Url;
    }
    if FILE_PATH_REGEX.is_match(text) {
        return CitationType::FilePath;
    }
    if STANDARD_NUMBER_REGEX.is_match(text) || IDENTIFIER_REGEX.is_match(text) {
        return CitationType::Standard;
    }
    if EMAIL_REGEX.is_match(text) {
        return CitationType::Email;
    }
    if text.contains('#') && (text.to_lowercase().contains("slack") || text.to_lowercase().contains("teams") || text.contains('#')) {
        // Has a channel reference
        if Regex::new(r"#[a-zA-Z][\w-]+").unwrap().is_match(text) {
            return CitationType::SlackOrTeams;
        }
    }
    if text.to_lowercase().contains("slack") || text.to_lowercase().contains("teams") {
        return CitationType::SlackOrTeams;
    }
    if SYSTEM_ID_REGEX.is_match(text) {
        return CitationType::SystemOrDb;
    }
    if KNOWN_TOOL_REGEX.is_match(text) {
        return CitationType::NavigableTool;
    }
    if STANDARD_BODY_REGEX.is_match(text) {
        return CitationType::Standard;
    }
    // Check CONVERSATION before ACADEMIC to avoid false positives
    if CONVERSATION_REGEX.is_match(text) {
        return CitationType::Conversation;
    }
    // Check ACADEMIC before SCRIPTURE to avoid "Nature 612:45" matching as scripture
    if ACADEMIC_REGEX.is_match(text) {
        // Exclude month-name + year patterns (e.g. "January 2026")
        let is_month_year = MONTH_NAMES.iter().any(|m| {
            let pattern = format!("{} ", m);
            text.contains(&pattern) || text.ends_with(m)
        });
        if !is_month_year {
            return CitationType::Academic;
        }
    }
    if SCRIPTURE_REGEX.is_match(text) {
        return CitationType::Scripture;
    }
    if BOOK_REGEX.is_match(text) {
        return CitationType::Book;
    }
    // Fallback: if the text has a page/section reference, treat as Book (has locatable reference)
    if PAGE_SECTION_REGEX.is_match(text) {
        return CitationType::Book;
    }
    CitationType::Unknown
}

/// Returns true if the citation passes tier-1 structural validation.
///
/// Pass → no question needed.
/// Fail → send to tier-2 batch LLM review.
pub fn validate_citation(ct: &CitationType, text: &str) -> bool {
    match ct {
        // Already navigable — pass immediately
        CitationType::Url | CitationType::FilePath => true,

        // Navigable tool: REQUIRE a URL or domain-style URL
        CitationType::NavigableTool => URL_REGEX.is_match(text) || DOMAIN_URL_REGEX.is_match(text),

        // Book: require page/chapter/section/verse reference
        CitationType::Book => PAGE_SECTION_REGEX.is_match(text),

        // Slack/Teams: require channel (#name) + date
        CitationType::SlackOrTeams => {
            let has_channel = Regex::new(r"#[a-zA-Z][\w-]+").unwrap().is_match(text);
            let has_date = DATE_REGEX.is_match(text);
            has_channel && has_date
        }

        // Email: require subject + sender + date
        CitationType::Email => {
            let has_date = DATE_REGEX.is_match(text);
            let has_sender = NAMED_PERSON_REGEX.is_match(text)
                || text.to_lowercase().contains("from ")
                || text.to_lowercase().contains("subj");
            has_date && has_sender
        }

        // System/DB: require a record ID pattern
        CitationType::SystemOrDb => SYSTEM_ID_REGEX.is_match(text),

        // Standard: require body name + number (already detected via STANDARD_NUMBER_REGEX)
        CitationType::Standard => {
            STANDARD_NUMBER_REGEX.is_match(text) || IDENTIFIER_REGEX.is_match(text)
        }

        // Scripture: require book + chapter:verse
        CitationType::Scripture => SCRIPTURE_REGEX.is_match(text),

        // Academic: require author + year (venue is optional for tier 1)
        CitationType::Academic => ACADEMIC_REGEX.is_match(text),

        // Conversation: require participants + date
        CitationType::Conversation => {
            let has_date = DATE_REGEX.is_match(text);
            let has_participants = NAMED_PERSON_REGEX.is_match(text)
                || text.to_lowercase().contains("with ")
                || text.to_lowercase().contains("and ");
            has_date && has_participants
        }

        // Unknown: check fallback patterns that don't fit a named type but are still specific
        CitationType::Unknown => {
            // Domain-style URL without protocol (e.g. linkedin.com/in/username)
            if DOMAIN_URL_REGEX.is_match(text) {
                return true;
            }
            // Named person + ISO date (e.g. "from John Smith, 2026-02-15")
            if NAMED_PERSON_REGEX.is_match(text) && DATE_REGEX.is_match(text) {
                return true;
            }
            // Channel reference + ISO date without a Slack/Teams keyword
            let has_channel = Regex::new(r"#[a-zA-Z][\w-]+").unwrap().is_match(text);
            if has_channel && DATE_REGEX.is_match(text) {
                return true;
            }
            // Catalog/record number (e.g. "CL 1355", "A-77", "SD 1361")
            if CATALOG_NUMBER_REGEX.is_match(text) {
                return true;
            }
            false
        }
    }
}

/// Human-readable description of what's missing for a failing citation.
pub fn citation_failure_reason(ct: &CitationType) -> &'static str {
    match ct {
        CitationType::Url | CitationType::FilePath => "already valid",
        CitationType::NavigableTool => "tool name present but no URL — add the direct URL",
        CitationType::Book => "book/publication present but no page/chapter/section reference",
        CitationType::SlackOrTeams => "Slack/Teams source missing channel (#name) or date",
        CitationType::Email => "email source missing sender or date",
        CitationType::SystemOrDb => "system/DB source missing record ID (e.g. PROJ-678)",
        CitationType::Standard => "standard body present but no document number (e.g. RFC 7231)",
        CitationType::Scripture => "scripture reference missing chapter:verse",
        CitationType::Academic => "academic source missing author + year",
        CitationType::Conversation => "meeting/call source missing participants or date",
        CitationType::Unknown => "source type unrecognized — add URL, record ID, or other navigable reference",
    }
}

/// Returns true if the citation text is specific enough for independent verification.
///
/// This is the public API used by the rest of the codebase.
/// Internally uses detect_citation_type + validate_citation (tier 1).
pub fn is_citation_specific(text: &str) -> bool {
    let ct = detect_citation_type(text);
    validate_citation(&ct, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- detect_citation_type ---

    #[test]
    fn test_detect_url() {
        assert_eq!(detect_citation_type("https://example.com"), CitationType::Url);
    }

    #[test]
    fn test_detect_file_path() {
        assert_eq!(
            detect_citation_type("/home/user/kb/notes.md"),
            CitationType::FilePath
        );
    }

    #[test]
    fn test_detect_navigable_tool_phonetool() {
        assert_eq!(
            detect_citation_type("Phonetool lookup, 2026-02-10"),
            CitationType::NavigableTool
        );
    }

    #[test]
    fn test_detect_navigable_tool_linkedin() {
        assert_eq!(
            detect_citation_type("LinkedIn profile, accessed 2026-01"),
            CitationType::NavigableTool
        );
    }

    #[test]
    fn test_detect_book() {
        assert_eq!(
            detect_citation_type("Peterson Field Guide to Mushrooms"),
            CitationType::Book
        );
    }

    #[test]
    fn test_detect_slack() {
        assert_eq!(
            detect_citation_type("Slack #project-alpha, 2026-01-20"),
            CitationType::SlackOrTeams
        );
    }

    #[test]
    fn test_detect_slack_no_channel() {
        assert_eq!(
            detect_citation_type("Slack conversation, 2026-01-15"),
            CitationType::SlackOrTeams
        );
    }

    #[test]
    fn test_detect_email() {
        assert_eq!(
            detect_citation_type("Email from John Smith, 2026-02-15"),
            CitationType::Email
        );
    }

    #[test]
    fn test_detect_system_jira() {
        assert_eq!(
            detect_citation_type("Jira PROJ-678"),
            CitationType::SystemOrDb
        );
    }

    #[test]
    fn test_detect_standard_rfc() {
        assert_eq!(
            detect_citation_type("RFC 7231, Section 6.5.1"),
            CitationType::Standard
        );
    }

    #[test]
    fn test_detect_scripture() {
        assert_eq!(
            detect_citation_type("Genesis 1:1"),
            CitationType::Scripture
        );
    }

    #[test]
    fn test_detect_academic() {
        assert_eq!(
            detect_citation_type("Smith 2024, Nature 612:45"),
            CitationType::Academic
        );
    }

    #[test]
    fn test_detect_conversation() {
        assert_eq!(
            detect_citation_type("Meeting with account team, January 2026"),
            CitationType::Conversation
        );
    }

    #[test]
    fn test_detect_unknown() {
        assert_eq!(
            detect_citation_type("AWS documentation"),
            CitationType::Unknown
        );
    }

    // --- validate_citation (tier 1 pass/fail) ---

    #[test]
    fn test_url_passes() {
        let ct = CitationType::Url;
        assert!(validate_citation(&ct, "https://docs.aws.amazon.com/page.html"));
    }

    #[test]
    fn test_navigable_tool_with_url_passes() {
        let ct = CitationType::NavigableTool;
        assert!(validate_citation(
            &ct,
            "Phonetool (https://phonetool.amazon.com/users/jsmith), 2026-02-10"
        ));
    }

    #[test]
    fn test_navigable_tool_without_url_fails() {
        let ct = CitationType::NavigableTool;
        assert!(!validate_citation(&ct, "Phonetool lookup, 2026-02-10"));
    }

    #[test]
    fn test_book_with_page_passes() {
        let ct = CitationType::Book;
        assert!(validate_citation(
            &ct,
            "Peterson Field Guide to Mushrooms of North America, p.247"
        ));
    }

    #[test]
    fn test_book_without_page_fails() {
        let ct = CitationType::Book;
        assert!(!validate_citation(
            &ct,
            "Peterson Field Guide to Mushrooms of North America"
        ));
    }

    #[test]
    fn test_slack_with_channel_and_date_passes() {
        let ct = CitationType::SlackOrTeams;
        assert!(validate_citation(
            &ct,
            "Slack #project-alpha, @user, 2026-01-20"
        ));
    }

    #[test]
    fn test_slack_without_channel_fails() {
        let ct = CitationType::SlackOrTeams;
        assert!(!validate_citation(&ct, "Slack conversation, 2026-01-15"));
    }

    #[test]
    fn test_email_with_sender_and_date_passes() {
        let ct = CitationType::Email;
        assert!(validate_citation(
            &ct,
            "Email from John Smith, 2026-02-15, subject Q4 Review"
        ));
    }

    #[test]
    fn test_email_without_date_fails() {
        let ct = CitationType::Email;
        assert!(!validate_citation(&ct, "Email correspondence"));
    }

    #[test]
    fn test_system_with_id_passes() {
        let ct = CitationType::SystemOrDb;
        assert!(validate_citation(&ct, "Jira PROJ-678"));
    }

    #[test]
    fn test_system_without_id_fails() {
        let ct = CitationType::SystemOrDb;
        assert!(!validate_citation(&ct, "Jira ticket"));
    }

    #[test]
    fn test_standard_with_number_passes() {
        let ct = CitationType::Standard;
        assert!(validate_citation(&ct, "RFC 7231, Section 6.5.1"));
    }

    #[test]
    fn test_scripture_with_verse_passes() {
        let ct = CitationType::Scripture;
        assert!(validate_citation(&ct, "Genesis 1:1"));
    }

    #[test]
    fn test_conversation_with_participants_and_date_passes() {
        let ct = CitationType::Conversation;
        assert!(validate_citation(
            &ct,
            "Meeting with John Smith and Jane Doe, 2026-01-15"
        ));
    }

    #[test]
    fn test_conversation_without_date_fails() {
        let ct = CitationType::Conversation;
        assert!(!validate_citation(&ct, "Meeting notes"));
    }

    #[test]
    fn test_unknown_fails() {
        let ct = CitationType::Unknown;
        assert!(!validate_citation(&ct, "AWS documentation"));
    }

    // --- is_citation_specific (backward-compat wrapper) ---

    #[test]
    fn test_url_is_specific() {
        assert!(is_citation_specific(
            "https://docs.aws.amazon.com/config/latest/developerguide/WhatIsConfig.html, accessed 2026-03-07"
        ));
    }

    #[test]
    fn test_book_with_page_is_specific() {
        assert!(is_citation_specific(
            "Peterson Field Guide to Mushrooms of North America, p.247"
        ));
    }

    #[test]
    fn test_email_with_person_date_subject_is_specific() {
        assert!(is_citation_specific(
            "Email from John Smith to Daniel, 2026-02-15, subject \"Q4 Architecture Review\""
        ));
    }

    #[test]
    fn test_slack_channel_thread_date_is_specific() {
        assert!(is_citation_specific(
            "Slack #project-alpha, thread 2026-01-20, re: deployment timeline"
        ));
    }

    #[test]
    fn test_file_path_is_specific() {
        assert!(is_citation_specific(
            "/home/user/kb/customers/acme/architecture.md"
        ));
    }

    #[test]
    fn test_rfc_is_specific() {
        assert!(is_citation_specific("RFC 7231, Section 6.5.1"));
    }

    #[test]
    fn test_interview_with_person_date_is_specific() {
        assert!(is_citation_specific(
            "Interview with Jane Doe, CTO of Acme Corp, 2025-11-03"
        ));
    }

    #[test]
    fn test_observation_number_is_specific() {
        assert!(is_citation_specific(
            "iNaturalist observation #12345, 2024-06-15"
        ));
    }

    #[test]
    fn test_isbn_is_specific() {
        assert!(is_citation_specific("ISBN 978-0-13-468599-1, Chapter 12"));
    }

    #[test]
    fn test_doi_is_specific() {
        assert!(is_citation_specific("DOI: 10.1038/nature12373"));
    }

    #[test]
    fn test_section_reference_is_specific() {
        assert!(is_citation_specific(
            "AWS Well-Architected Framework, Section 3.2"
        ));
    }

    #[test]
    fn test_pp_reference_is_specific() {
        assert!(is_citation_specific(
            "Knuth, The Art of Computer Programming, pp.42-45"
        ));
    }

    #[test]
    fn test_paragraph_symbol_is_specific() {
        assert!(is_citation_specific("Internal policy document, §4.2"));
    }

    // --- Vague citations → not specific ---

    #[test]
    fn test_aws_documentation_is_vague() {
        assert!(!is_citation_specific("AWS documentation"));
    }

    #[test]
    fn test_aws_documentation_with_date_is_vague() {
        assert!(!is_citation_specific(
            "AWS documentation, accessed 2026-03-07"
        ));
    }

    #[test]
    fn test_internal_documents_is_vague() {
        assert!(!is_citation_specific("Internal documents"));
    }

    #[test]
    fn test_author_knowledge_is_vague() {
        assert!(!is_citation_specific("Author knowledge"));
    }

    #[test]
    fn test_common_knowledge_is_vague() {
        assert!(!is_citation_specific("Common knowledge"));
    }

    #[test]
    fn test_company_records_is_vague() {
        assert!(!is_citation_specific("Company records"));
    }

    #[test]
    fn test_slack_alone_is_vague() {
        assert!(!is_citation_specific("Slack"));
    }

    #[test]
    fn test_wikipedia_alone_is_vague() {
        assert!(!is_citation_specific("Wikipedia"));
    }

    #[test]
    fn test_email_correspondence_is_vague() {
        assert!(!is_citation_specific("Email correspondence"));
    }

    #[test]
    fn test_google_is_vague() {
        assert!(!is_citation_specific("Google"));
    }

    #[test]
    fn test_slack_with_no_specifics_is_vague() {
        assert!(!is_citation_specific("Slack conversation, 2026-01-15"));
    }

    #[test]
    fn test_unverified_is_vague() {
        assert!(!is_citation_specific("unverified"));
    }

    // --- New tier-1 cases from task spec ---

    #[test]
    fn test_phonetool_without_url_is_vague() {
        // Tool name + date but no URL → tier 2
        assert!(!is_citation_specific("Phonetool, 2026"));
    }

    #[test]
    fn test_phonetool_with_url_is_specific() {
        assert!(is_citation_specific(
            "Phonetool (https://phonetool.amazon.com/users/jsmith), 2026-02-10"
        ));
    }

    #[test]
    fn test_meeting_notes_alone_is_vague() {
        assert!(!is_citation_specific("Meeting notes"));
    }

    #[test]
    fn test_jira_ticket_without_id_is_vague() {
        assert!(!is_citation_specific("Jira ticket"));
    }

    #[test]
    fn test_jira_with_id_is_specific() {
        assert!(is_citation_specific("Jira PROJ-678"));
    }

    #[test]
    fn test_genesis_verse_is_specific() {
        assert!(is_citation_specific("Genesis 1:1"));
    }

    #[test]
    fn test_genesis_alone_is_vague() {
        // "Genesis" alone doesn't have chapter:verse
        assert!(!is_citation_specific("Genesis"));
    }

    #[test]
    fn test_industry_standard_is_vague() {
        assert!(!is_citation_specific("Industry standard"));
    }

    // --- Regression: patterns that should pass tier 1 ---

    #[test]
    fn test_named_person_with_date_is_specific() {
        // Named person + ISO date — was valid before #590, must still pass
        assert!(is_citation_specific("from John Smith, 2026-02-15"));
    }

    #[test]
    fn test_channel_author_date_is_specific() {
        // #channel + date without "Slack" keyword
        assert!(is_citation_specific("#general, @author, 2026-02"));
    }

    #[test]
    fn test_linkedin_domain_url_is_specific() {
        // Domain-style URL without https:// — navigable, should pass
        assert!(is_citation_specific("linkedin.com/in/username"));
    }

    #[test]
    fn test_catalog_number_cl_is_specific() {
        assert!(is_citation_specific("CL 1355"));
    }

    #[test]
    fn test_catalog_number_sd_is_specific() {
        assert!(is_citation_specific("SD 1361"));
    }

    #[test]
    fn test_catalog_number_hyphen_is_specific() {
        assert!(is_citation_specific("A-77"));
    }

    #[test]
    fn test_phonetool_without_url_still_fails() {
        // Tool name + date but no URL or domain URL → tier 2
        assert!(!is_citation_specific("Phonetool lookup, 2026-02-10"));
    }

    #[test]
    fn test_meeting_notes_still_fails() {
        assert!(!is_citation_specific("Meeting notes"));
    }
}
