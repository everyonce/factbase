# Domain-Agnostic Design — Mandatory Constraint

factbase is a **general-purpose knowledge base tool**. It must work equally well for any domain: sales accounts, ancient history, botany, law, music, medicine — anything. This is not aspirational; it is a hard constraint on every change.

## The Rule

**No logic branch, keyword list, enum variant, heuristic, or prompt may assume a specific domain.**

If you find yourself writing code that checks for "person", "company", "career", "promotion", "employee", "product", or any other domain-specific term in application logic — stop. You're solving the wrong problem.

## What This Means in Practice

### ❌ Never Do This
- `if doc_type == "person"` in filtering/validation logic
- Keyword lists like `["promotion", "hired", "resigned", "terminated"]`
- Enum variants named after domain concepts (`Promotion`, `ConcurrentRoles`)
- LLM prompts that reference "careers", "companies", or "employees" as examples
- Heuristics that only work for one entity type

### ✅ Do This Instead
- Use structural properties: temporal overlap, entity co-reference, date precision, section headings
- Use generic names: `SameEntityTransition` not `Promotion`, `ParallelOverlap` not `ConcurrentRoles`
- Write LLM prompts using neutral terms: "entity", "document", "temporal fact", "section"
- If you need examples in prompts, use diverse ones (a person AND a company AND a historical event)
- Test with non-people entities: plants, battles, products, legislation

### ⚠️ Gray Areas
- **Test data**: Using domain-specific names in test fixtures is fine (tests need concrete examples). But test diverse domains, not just people/companies.
- **Built-in glossary terms**: A default acronym list (AWS, ECS, etc.) is OK as a convenience — it suppresses false positives, doesn't gate logic.
- **Source type parsing**: Recognizing "LinkedIn" as a source type is fine — that's a real source name, not a domain assumption.
- **Config-driven fields**: If something is in `factbase.toml` and the user chose it, that's user domain, not hardcoded domain.
- **Documentation/comments**: Explaining behavior with domain examples is fine. Just don't let the examples become the implementation.

## Why This Matters

Other agents will build knowledge bases about:
- **Ancient history** — BCE dates, disputed timelines, fuzzy temporal precision
- **Botany** — species classifications, seasonal cycles, geographic distributions  
- **Law** — statutes, case citations, jurisdictional hierarchies
- **Music** — discographies, collaborations, label deals

If factbase assumes "entity = person with a career", none of these work. Every domain-specific hack we ship is technical debt that blocks adoption.

## When You're Unsure

Ask yourself: **"Would this code work correctly if every entity in the repo was a species of mushroom?"**

If no — generalize it.  
If you can't generalize it — back out the change and document the underlying problem as a TODO. A known gap is better than a domain-specific hack.

## Review Checklist

Before committing any change, grep your diff for these patterns:
- `"person"` / `"people"` / `"company"` / `"product"` / `"employee"` in logic (not test data)
- `career` / `promotion` / `employment` / `job` / `hire` in variable names or match arms
- Any `doc_type ==` comparison that branches on a specific type name
- LLM prompt text that assumes the domain

If any match: refactor to be generic, or don't ship it.
