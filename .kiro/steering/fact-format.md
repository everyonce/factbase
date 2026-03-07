# Fact Document Format (Internal Reference)

See the docs folder for complete specifications:
- [docs/fact-document-format.md](../../docs/fact-document-format.md) - Technical specification
- [docs/authoring-guide.md](../../docs/authoring-guide.md) - Human authoring guide  
- [docs/agent-authoring-guide.md](../../docs/agent-authoring-guide.md) - AI agent guide (comprehensive)

## Quick Reference

### Temporal Tags `@t[...]`

```
@t[=2024-03]     Point in time / as of
@t[~2024-03]     Last seen / last known  
@t[2020..2022]   Date range
@t[2020..]       Started, ongoing
@t[..2022]       Unknown start, ended
@t[?]            Unknown / unverified
@t[=331 BCE]     BCE suffix notation (→ -0331)
@t[=-330]        Negative year (→ -0330)
@t[-490..-479]   BCE range
```

### Source Footnotes

```markdown
- Fact here @t[2020..2022] [^1]

---
[^1]: LinkedIn profile, scraped 2024-01-15
```

## Implementation Notes

### Parsing Temporal Tags

Regex pattern for extraction:
```
@t\[([=~])?(\d{4}(?:-(?:Q[1-4]|\d{2}(?:-\d{2})?))?)?(?:\.\.(\d{4}(?:-(?:Q[1-4]|\d{2}(?:-\d{2})?))?)?)?\]|@t\[\?\]
```

### Lint Integration

The `factbase check` command should check:
- Percentage of facts with temporal tags
- Valid date formats in tags
- Footnote reference/definition matching
- Source type standardization

### Search Considerations

Future enhancement: temporal-aware search that can filter results by:
- Overlapping date ranges
- Recency of `@t[~...]` dates
- Excluding `@t[?]` for high-confidence queries

## Review System

See [docs/review-system.md](../../docs/review-system.md) for full design.

### Quick Reference

Question types: `@q[temporal]`, `@q[conflict]`, `@q[missing]`, `@q[ambiguous]`, `@q[stale]`, `@q[duplicate]`, `@q[corruption]`, `@q[precision]`, `@q[weak-source]`

Commands:
- `factbase check` - Generate questions
- `factbase review --apply` - Process answered questions
- `factbase review --status` - Show pending question summary
