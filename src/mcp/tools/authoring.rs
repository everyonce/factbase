//! Authoring guide MCP tool.

use serde_json::Value;

/// Returns the factbase document authoring guide for AI agents.
pub fn get_authoring_guide() -> Value {
    serde_json::json!({
        "format": "markdown (.md files)",
        "structure": {
            "title": "First # Heading becomes the document title",
            "type": "Derived from parent folder: people/ → 'person', companies/ → 'company'. Entity folder convention: if filename matches parent folder (e.g., companies/xsolis/xsolis.md), type comes from grandparent ('company')",
            "id_header": "<!-- factbase:XXXXXX --> is auto-injected on first scan — never create or modify this",
            "length": "Minimum 100 chars, optimal 500-5000 chars",
            "filenames": "lowercase-with-hyphens.md (e.g., alice-chen.md, platform-api.md)"
        },
        "temporal_tags": {
            "description": "Every dynamic fact MUST have a temporal tag. Static facts (historical events, awards) do not need one. Education degrees are static but graduation years should use @t[=YYYY].",
            "syntax": {
                "@t[=2024-03]": "Event — happened at this date (founding, graduation, marriage)",
                "@t[~2024-03]": "State — true as of this date, may have changed (location, role, contact info)",
                "@t[2020..2022]": "Date range (started and ended)",
                "@t[2021..]": "Started, still ongoing",
                "@t[..2020]": "Historical, ended",
                "@t[?]": "Unknown / unverified"
            },
            "granularity": "Year (2024), Quarter (2024-Q2), Month (2024-03), Day (2024-03-15)",
            "example": "- VP Engineering at BigCo @t[2022..] [^1]"
        },
        "sources": {
            "description": "Cite sources with markdown footnotes for fact verification. Every source MUST be traceable — include enough detail to locate the original data.",
            "format": "Add [^N] after the fact, define [^N]: at the bottom after a --- separator",
            "types": "LinkedIn profile, Company website, Press release, News article, SEC filing, Author knowledge (human-only), Email, Conference bio, Slack, Inferred, Unverified",
            "traceability": "A platform name alone is NEVER sufficient. Always include dates, URLs, channel names, subject lines, or other identifiers. BAD: 'Slack message'. GOOD: 'Slack #project-alpha, 2024-01-10, https://workspace.slack.com/archives/C01234/p1234'",
            "author_knowledge": "Facts known firsthand by the knowledge base owner belong in dedicated author-knowledge/ documents. Cite as: 'Author knowledge, see [[id]]'. Agents must NEVER create author knowledge files or use 'Author knowledge' as a source — always cite the actual data source instead.",
            "example": "- Acquired StartupX for $50M @t[=2023-06] [^1][^2]\n\n---\n[^1]: Press release, 2023-06-15\n[^2]: TechCrunch article (techcrunch.com/2023/06/15/...), 2023-06-15"
        },
        "linking": {
            "description": "Use exact entity names matching other document titles for automatic link detection.",
            "good": "Alice Chen approved the Platform API design",
            "bad": "Alice approved it",
            "manual": "See [[a1b2c3]] for the full specification"
        },
        "inbox_blocks": {
            "description": "Stage corrections or new facts for LLM-assisted integration into the document body.",
            "format": "<!-- factbase:inbox -->\n- New fact here\n<!-- /factbase:inbox -->",
            "processing": "Integrated by apply_review_answers or `factbase review --apply`, then the block is removed"
        },
        "common_mistakes": [
            "Missing temporal tags on dynamic facts (job titles, locations, team members)",
            "Vague entity references ('the project') instead of exact names ('Platform API')",
            "Duplicate content across documents — link instead with [[id]]",
            "Missing source footnotes — always cite where facts came from",
            "Untraceable sources — 'Slack message' or 'Outlook' alone is insufficient; include channel, date, URL, or subject",
            "Using 'Author knowledge' as a source — this is reserved for human-authored knowledge files only; agents must cite the actual data source",
            "Modifying the <!-- factbase:XXXXXX --> header"
        ],
        "templates": {
            "person": "# Full Name\n\n**Role:** Title at Company @t[2023..]\n**Location:** City, State @t[~2024-01]\n\n## Career History\n- Current role @t[2023..] [^1]\n\n## Current Focus\n- Key project @t[2024..]\n\n---\n[^1]: Source, date",
            "company": "# Company Name\n\n## Overview\nWhat the company does.\n\n## Key Facts\n- Founded @t[=2015]\n- Employees: ~500 @t[~2024-01]\n\n## Leadership\n- CEO: Name @t[2020..]\n\n---\n[^1]: Source, date",
            "project": "# Project Name\n\n## Overview\nPurpose and goals.\n\n## Status\nCurrent phase @t[2024-Q1..]\n\n## Team\n- Name - Role @t[2024..]\n\n---\n[^1]: Source, date",
            "definitions": "# Definitions: <Domain>\n\n## Acronyms\n- **TAM**: Total Addressable Market — the total revenue opportunity for a product\n- **NPS**: Net Promoter Score — customer satisfaction metric (-100 to 100)\n\n## Terms\n- **ARR**: Annual Recurring Revenue — annualized subscription revenue\n- **Runway**: Months of cash remaining at current burn rate"
        },
        "definitions_files": {
            "description": "When encountering undefined acronyms or ambiguous terms (@q[ambiguous] questions), create or update a definitions file rather than only answering inline. This builds a reusable glossary that prevents the same question from recurring across documents.",
            "folder": "Place in a definitions/ folder (type: 'definition'). Organize by domain: definitions/business-terms.md, definitions/technical-terms.md, definitions/product-terms.md",
            "workflow": "1. Check if a definitions file already covers the term. 2. If not, create or update the appropriate definitions file. 3. Answer the review question with: 'See [[id]] definitions file' so the fact gets linked. 4. The definition is now searchable and reusable across the knowledge base.",
            "when_to_create": "Create definitions files when you encounter @q[ambiguous] questions about acronyms, jargon, or domain-specific terms. Do NOT create them for one-off clarifications (like 'is this home or work address')."
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_authoring_guide_has_required_sections() {
        let guide = get_authoring_guide();
        assert!(guide["temporal_tags"]["syntax"].is_object());
        assert!(guide["sources"]["format"].is_string());
        assert!(guide["structure"]["title"].is_string());
        assert!(guide["templates"]["person"].is_string());
        assert!(guide["common_mistakes"].is_array());
    }
}
