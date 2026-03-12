//! Citation quality scoring.
//!
//! Determines whether a source footnote is specific enough for independent verification.
//! A citation is "specific" if someone else could find the exact source from the description.

use regex::Regex;
use std::sync::LazyLock;

/// URL pattern (http:// or https://)
static URL_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"https?://").expect("url regex"));

/// File path pattern (absolute paths, or common doc extensions)
static FILE_PATH_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[\s(])(?:[/~][^\s]+|[^\s]+\.(?:md|pdf|doc|docx|txt|csv|xlsx|html))\b")
        .expect("file path regex")
});

/// Page/section/chapter reference
static PAGE_SECTION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:pp?\.\s*\d|§\s*\d|[Ss]ection\s+\d|[Cc]hapter\s+\d)")
        .expect("page section regex")
});

/// Standard identifiers (ISBN, DOI, RFC, ISSN, arXiv, ticket/observation/thread with number)
static IDENTIFIER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:ISBN|DOI|RFC|ISSN|arXiv|PMID|CVE)[\s:-]+[\w./-]+|(?:#|observation\s*#?|ticket\s*#?|thread\s*#?|issue\s*#?)\s*\d+").expect("identifier regex")
});

/// Date pattern (YYYY-MM-DD or YYYY-MM or similar)
static DATE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d{4}-\d{2}(?:-\d{2})?").expect("date regex"));

/// Named person pattern (capitalized words suggesting a person name)
static NAMED_PERSON_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:with|from|by|interview)\s+[A-Z][a-z]+(?:\s+[A-Z][a-z]+)+")
        .expect("named person regex")
});

/// Named channel/thread with date (e.g., "#project-alpha, thread 2026-01-20")
static CHANNEL_DATE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#[a-zA-Z][\w-]+.*\d{4}-\d{2}").expect("channel date regex"));

/// Returns true if the citation text is specific enough for independent verification.
///
/// A citation is specific if it contains ANY of:
/// - A URL
/// - A file path
/// - A page/section/chapter reference
/// - A standard identifier (ISBN, DOI, RFC, etc.)
/// - A named person + date + context
/// - A named channel/thread + date
pub fn is_citation_specific(text: &str) -> bool {
    if URL_REGEX.is_match(text) {
        return true;
    }
    if FILE_PATH_REGEX.is_match(text) {
        return true;
    }
    if PAGE_SECTION_REGEX.is_match(text) {
        return true;
    }
    if IDENTIFIER_REGEX.is_match(text) {
        return true;
    }
    // Named person + date
    if NAMED_PERSON_REGEX.is_match(text) && DATE_REGEX.is_match(text) {
        return true;
    }
    // Named channel + date
    if CHANNEL_DATE_REGEX.is_match(text) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Good citations (specific) ---

    #[test]
    fn test_url_is_specific() {
        assert!(is_citation_specific("https://docs.aws.amazon.com/config/latest/developerguide/WhatIsConfig.html, accessed 2026-03-07"));
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

    // --- Bad citations (vague) ---

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
}
