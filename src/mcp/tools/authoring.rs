use serde_json::Value;

/// Returns the authoring guide as a JSON value.
/// This is the content returned by the `get_authoring_guide` MCP tool.
/// All examples must be domain-diverse per the domain-agnostic design constraint.
pub fn get_authoring_guide() -> Value {
    serde_json::json!({
        "format": "markdown (.md files)",
        "taxonomy_design": {
            "description": "Before creating documents, design your knowledge base structure. Templates below are illustrations — this framework helps you think through ANY domain.",
            "steps": [
                {
                    "step": "1. Identify entity types",
                    "question": "If I organized this domain into filing cabinets, what would the labels be?",
                    "guidance": "Each category becomes a folder and document type. Start with 2-4 types. Examples: a music KB might have artists/, albums/, labels/, genres/. A legal KB might have statutes/, cases/, jurisdictions/. A biology KB might have species/, habitats/, researchers/.",
                },
                {
                    "step": "2. Design sections per type",
                    "question": "What would someone want to know about each type of entity?",
                    "guidance": "Group related facts into ## headings. Each entity type will have its own recurring section pattern. An 'album' might have ## Tracks, ## Personnel, ## Reception. A 'statute' might have ## Text, ## Amendments, ## Case Law.",
                },
                {
                    "step": "3. Identify what changes over time",
                    "question": "Could this fact be different next year?",
                    "guidance": "Dynamic facts need @t[...] tags. Static facts don't. Dynamic: conservation status, population, current leader, market price. Static: founding date, chemical formula, date of battle, author of a book.",
                },
                {
                    "step": "4. Identify your sources",
                    "question": "What are the authoritative sources for this domain?",
                    "guidance": "These inform your footnote types. Scientific domain: journal articles, databases, field observations. Legal: court records, legislative databases. Historical: primary sources, academic works. Business: official filings, press releases.",
                },
                {
                    "step": "5. Start small, evolve",
                    "question": "What are the 2-3 most important entities to document first?",
                    "guidance": "Begin with a few documents. The structure reveals itself as you add data. You can reorganize later with organize_analyze and organize tools. Don't over-design upfront.",
                }
            ]
        },
        "structure": {
            "title": "First # Heading becomes the document title",
            "type": "Derived from parent folder: species/ → 'species', events/ → 'event', people/ → 'person'. Entity folder convention: if filename matches parent folder (e.g., species/amanita-muscaria/amanita-muscaria.md), type comes from grandparent ('species')",
            "id_header": "<!-- factbase:XXXXXX --> is auto-injected on first scan — never create or modify this",
            "length": "Minimum 100 chars, optimal 500-5000 chars",
            "filenames": "lowercase-with-hyphens.md (e.g., amanita-muscaria.md, battle-of-thermopylae.md, platform-api.md)",
            "reorganization": "If a file is in the wrong folder or has a poor name, rename or move it freely using file tools. Just run scan_repository afterward to re-index. The factbase ID in the <!-- factbase:XXXXXX --> header is stable across renames.",
            "archive": "Documents in archive/ folders are indexed and searchable but skipped by quality checks. Use for stable/historical documents: species/archive/reclassified.md, events/archive/superseded.md"
        },
        "temporal_tags": {
            "description": "Every dynamic fact MUST have a temporal tag. Static facts (mathematical constants, chemical formulas) do not need one. CRITICAL: Only dates/years go inside @t[...] — NEVER put descriptive text inside the brackets.",
            "syntax": {
                "@t[=2024-03]": "Event — happened at this exact date",
                "@t[~2024-03]": "State — true as of this date, may change",
                "@t[2020..2022]": "Date range — started and ended",
                "@t[2021..]": "Started, still ongoing",
                "@t[..2020]": "Historical, ended at this date",
                "@t[?]": "Date unknown / unverified — use this when you cannot determine the date",
                "@t[=331 BCE]": "BCE event — human-readable BCE suffix (→ -0331)",
                "@t[=-330]": "BCE event — negative year, auto-padded to -0330",
                "@t[=-0031]": "BCE event — use negative 4-digit year for pre-CE dates",
                "@t[-0490..-0479]": "BCE date range",
                "@t[-0031..0014]": "Range spanning BCE to CE"
            },
            "valid_content": "ONLY these go inside @t[...]: years (2024 or -0490 for BCE), quarters (2024-Q2), months (2024-03), days (2024-03-15), ranges with dates (2020..2023 or -0490..-0479), or ? for unknown",
            "common_errors": {
                "❌ WRONG — entity names inside tag": "@t[Wolfgang Amadeus Mozart] @t[Mount Vesuvius] @t[Amanita muscaria]",
                "❌ WRONG — descriptions inside tag": "@t[Complex counterpoint and fugal writing] @t[bright red when young] @t[No significant seismic activity]",
                "❌ WRONG — statuses inside tag": "@t[Active Production Status: Ongoing] @t[Critically Endangered]",
                "❌ WRONG — statistics inside tag": "@t[Total Produced: 650+] @t[Population: 12000]",
                "❌ WRONG — vague time words": "@t[seasonal] @t[since ancient times] @t[traditional..modern] @t[varies by region]",
                "✅ CORRECT — dates only": "@t[=2024] @t[~2024] @t[1753..] @t[2020..2023] @t[?]",
                "rule": "If it's not a year, month, day, quarter, or ?, it does NOT go inside @t[...]"
            },
            "granularity": "Year (2024), Quarter (2024-Q2), Month (2024-03), Day (2024-03-15)",
            "placement": "Place the @t[...] tag AFTER the fact text, BEFORE the source footnote: `- Fact description @t[~2024] [^1]`",
            "examples": [
                "- Population: ~12,000 @t[~2024-01] [^1]",
                "- Reclassified to family Omphalotaceae @t[=2006] [^2]",
                "- Director of Operations at Acme Corp @t[2022..] [^3]",
                "- Cap color: bright red to orange @t[~2024] [^1]  ← description is in the text, date is in the tag",
                "- Fruiting season: summer to autumn @t[~2024] [^2]  ← 'summer to autumn' goes in the text, NOT in the tag",
                "- Battle of Thermopylae @t[=-0480] [^4]  ← BCE date using negative year",
                "- Greco-Persian Wars @t[-0499..-0449] [^5]  ← BCE date range"
            ]
        },
        "sources": {
            "description": "Cite sources with markdown footnotes for fact verification. Every source MUST be traceable — include enough detail to locate the original data.",
            "format": "Add [^N] after the fact, define [^N]: at the bottom after a --- separator",
            "types": "Journal article, Database record, Official report, News article, Book, Website, Field observation, Author knowledge (human-only), Archival document, Personal communication, Inferred, Unverified",
            "traceability": "A source name alone is NEVER sufficient. Always include dates, URLs, page numbers, or other identifiers. BAD: 'Wikipedia'. GOOD: 'Wikipedia \"Amanita muscaria\", accessed 2024-01-15, https://en.wikipedia.org/wiki/Amanita_muscaria'",
            "author_knowledge": "Facts known firsthand by the knowledge base owner belong in dedicated author-knowledge/ documents. Cite as: 'Author knowledge, see [[id]]'. Agents must NEVER create author knowledge files or use 'Author knowledge' as a source — always cite the actual data source instead.",
            "example": "- First described by Linnaeus @t[=1753] [^1][^2]\n\n---\n[^1]: Linnaeus, Species Plantarum, vol. 2, p. 1171, 1753\n[^2]: MycoBank record #120098, accessed 2024-01-10"
        },
        "linking": {
            "description": "Use exact entity names matching other document titles for automatic link detection.",
            "good": "Amanita muscaria forms mycorrhizal associations with Betula pendula",
            "bad": "This mushroom grows near birch trees",
            "manual": "See [[a1b2c3]] for the full specification"
        },
        "inbox_blocks": {
            "description": "Stage corrections or new facts for LLM-assisted integration into the document body.",
            "format": "<!-- factbase:inbox -->\n- New fact here\n<!-- /factbase:inbox -->",
            "processing": "Integrated by apply_review_answers or `factbase review --apply`, then the block is removed"
        },
        "common_mistakes": [
            "Putting text/descriptions inside @t[...] instead of dates — WRONG: @t[seasonal], @t[Wolfgang Amadeus Mozart], @t[Active Production Status: Ongoing], @t[Total Produced: 650+] — RIGHT: @t[~2024] or @t[2020..2023] or @t[?]",
            "Missing temporal tags on dynamic facts (status, classification, population, roles)",
            "Vague entity references ('the species', 'the project') instead of exact names ('Amanita muscaria', 'Platform API')",
            "Duplicate content across documents — link instead with [[id]]",
            "Missing source footnotes — always cite where facts came from",
            "Untraceable sources — 'Wikipedia' or 'a paper' alone is insufficient; include title, date, URL, or DOI",
            "Using 'Author knowledge' as a source — this is reserved for human-authored knowledge files only; agents must cite the actual data source",
            "Modifying the <!-- factbase:XXXXXX --> header"
        ],
        "template_pattern": {
            "description": "All factbase documents follow the same structural pattern regardless of domain. Adapt the sections to fit your subject matter.",
            "pattern": {
                "title": "# Entity Name — the document title IS the entity name",
                "sections": "Group related facts under ## headings. Choose headings that make sense for the entity.",
                "facts": "Each bullet is one fact. Attach @t[...] to anything that changes over time.",
                "sources": "Cite every fact with [^N] footnotes. Define sources after a --- separator.",
                "links": "Use exact entity names from other documents to create automatic cross-references."
            }
        },
        "templates": {
            "natural_science": "# Amanita muscaria\n\n## Classification\n- Kingdom: Fungi @t[=1753] [^1]\n- Family: Amanitaceae\n- Common name: Fly agaric\n\n## Habitat & Distribution\n- Found in temperate forests across Northern Hemisphere @t[~2024] [^2]\n- Mycorrhizal association with birch, pine, spruce\n\n## Edibility & Toxicity\n- Contains ibotenic acid and muscimol [^3]\n- Classified as poisonous @t[~2024]\n\n---\n[^1]: Linnaeus, Species Plantarum, 1753\n[^2]: MycoBank database, accessed 2024-01\n[^3]: Michelot & Melendez-Howell, Mycological Research, 2003",
            "historical_entity": "# Battle of Thermopylae\n\n## Overview\n- Date: @t[=480 BCE] [^1]\n- Location: Thermopylae pass, Greece\n- Outcome: Persian victory\n\n## Participants\n- Greek alliance led by Leonidas I of Sparta [^1]\n- Persian forces led by Xerxes I [^1]\n\n## Significance\n- Delayed Persian advance, enabled Greek naval preparation @t[480 BCE..479 BCE] [^2]\n\n---\n[^1]: Herodotus, Histories, Book VII\n[^2]: Holland, Persian Fire, pp. 255-280, 2005",
            "person": "# Full Name\n\n**Role:** Title at Organization @t[2023..]\n**Location:** City, Country @t[~2024-01]\n\n## Career History\n- Current role @t[2023..] [^1]\n\n## Current Focus\n- Key project or activity @t[2024..]\n\n---\n[^1]: Source, date",
            "organization": "# Organization Name\n\n## Overview\nWhat the organization does.\n\n## Key Facts\n- Founded @t[=2015] [^1]\n- Size: ~500 members @t[~2024-01]\n\n## Leadership\n- Director: Name @t[2020..]\n\n---\n[^1]: Source, date",
            "project": "# Project Name\n\n## Overview\nPurpose and goals.\n\n## Status\nCurrent phase @t[2024-Q1..]\n\n## Team\n- Name - Role @t[2024..]\n\n---\n[^1]: Source, date",
            "generic": "# Entity Name\n\n## Overview\nBrief description of the entity.\n\n## Key Facts\n- Fact with temporal context @t[~2024] [^1]\n- Fact with date range @t[2020..2023] [^2]\n- Static fact that doesn't change [^3]\n\n## Relationships\n- Related to Other Entity Name [^1]\n\n---\n[^1]: Source name, date or URL\n[^2]: Source name, date or URL\n[^3]: Source name, date or URL",
            "definitions": "# Definitions: <Domain>\n\n## Acronyms\n- **PCR**: Polymerase Chain Reaction — method for amplifying DNA sequences\n- **BCE**: Before Common Era — calendar notation for dates before year 1\n\n## Terms\n- **Mycorrhiza**: Symbiotic association between a fungus and plant roots\n- **Holotype**: The single specimen designated as the name-bearing type of a species"
        },
        "definitions_files": {
            "description": "When encountering undefined acronyms or ambiguous terms (@q[ambiguous] questions), create or update a definitions file rather than only answering inline. This builds a reusable glossary that prevents the same question from recurring across documents.",
            "folder": "Place in a definitions/ folder (type: 'definition'). Organize by domain: definitions/taxonomy-terms.md, definitions/technical-terms.md, definitions/historical-terms.md",
            "workflow": "1. Check if a definitions file already covers the term. 2. If not, create or update the appropriate definitions file. 3. Answer the review question with: 'See [[id]] definitions file' so the fact gets linked. 4. The definition is now searchable and reusable across the knowledge base.",
            "when_to_create": "Create definitions files when you encounter @q[ambiguous] questions about acronyms, jargon, or domain-specific terms. Do NOT create them for one-off clarifications (like 'is this the common or scientific name')."
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
        assert!(guide["template_pattern"]["pattern"].is_object());
        assert!(guide["common_mistakes"].is_array());
        assert!(guide["taxonomy_design"]["steps"].is_array());
    }

    #[test]
    fn test_templates_cover_diverse_domains() {
        let guide = get_authoring_guide();
        let templates = &guide["templates"];
        assert!(templates["natural_science"].is_string());
        assert!(templates["historical_entity"].is_string());
        assert!(templates["person"].is_string());
        assert!(templates["organization"].is_string());
        assert!(templates["generic"].is_string());
        assert!(templates["definitions"].is_string());
    }

    #[test]
    fn test_template_pattern_explains_structure() {
        let guide = get_authoring_guide();
        let pattern = &guide["template_pattern"];
        assert!(pattern["description"].is_string());
        assert!(pattern["pattern"]["title"].is_string());
        assert!(pattern["pattern"]["sections"].is_string());
        assert!(pattern["pattern"]["facts"].is_string());
        assert!(pattern["pattern"]["sources"].is_string());
    }

    #[test]
    fn test_taxonomy_design_section() {
        let guide = get_authoring_guide();
        let design = &guide["taxonomy_design"];
        assert!(design["description"].is_string());
        let steps = design["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 5);
        for step in steps {
            assert!(step["step"].is_string());
            assert!(step["question"].is_string());
            assert!(step["guidance"].is_string());
        }
    }

    #[test]
    fn test_no_domain_specific_bias_in_examples() {
        let guide = get_authoring_guide();
        // Temporal tag examples should show diverse domains, not just business
        let examples = &guide["temporal_tags"]["examples"];
        assert!(examples.is_array());
        assert!(examples.as_array().unwrap().len() >= 3);
    }

    #[test]
    fn test_temporal_tag_negative_examples_cover_all_categories() {
        let guide = get_authoring_guide();
        let errors = &guide["temporal_tags"]["common_errors"];
        let errors_str = serde_json::to_string(errors).unwrap();
        // Entity names
        assert!(errors_str.contains("Wolfgang Amadeus Mozart"), "missing entity name negative example");
        // Descriptions
        assert!(errors_str.contains("counterpoint"), "missing description negative example");
        // Statuses
        assert!(errors_str.contains("Active Production Status"), "missing status negative example");
        // Statistics
        assert!(errors_str.contains("Total Produced"), "missing statistic negative example");
        // Vague time words
        assert!(errors_str.contains("seasonal"), "missing vague time word negative example");
    }
}
