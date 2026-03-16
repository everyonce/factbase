# Prompt Fix 3: Mechanical Routing Rules — Test Results

## Fix Summary

Rewrote the `workflow` tool description in `src/mcp/tools/schema.rs` from prose guidance to explicit if/then routing rules. The new description replaces scattered `⚠️ ROUTING:` paragraphs with a single ordered `Routing rules` section that maps user intent directly to workflow calls.

## New Routing Rules (applied in order)

```
- 'build', 'create', 'start', 'new KB' → workflow(create)
- 'add [new topic/entity]' → workflow(add, topic=...)
- 'add [note/flag/tag] to [existing entity]' → workflow(correct)
- 'scan', 'index', 'reindex' → workflow(maintain)
- 'check for new', 'look for updates', 'what's new' → workflow(refresh)
- factual correction about existing entity → workflow(correct) IMMEDIATELY as FIRST action
- change that happened over time → workflow(transition)
- no entity named → ASK one focused clarifying question before acting
```

## Test Results

### Existing tests: all pass (2492 total, 0 failures)

Key existing tests verified:
- `test_workflow_description_is_compact` — ≤17 lines ✅ (new desc: 17 lines)
- `test_workflow_description_has_clarification_instruction` ✅
- `test_workflow_schema_has_decision_rule` ✅
- `test_workflow_schema_has_user_override` ✅
- `test_workflow_schema_has_call_immediately_guidance` ✅
- `test_workflow_description_routes_scan_to_maintain` ✅
- `test_workflow_schema_has_kb_priority_guidance` ✅
- `test_workflow_description_routes_add_note_to_correct` ✅
- `test_refresh_routing_schema_mentions_trigger_phrases` ✅

### New tests added (4)

| Test | What it verifies |
|------|-----------------|
| `test_workflow_description_has_routing_rules_section` | Description contains "Routing rules" header |
| `test_workflow_description_routes_build_to_create` | 'build'/'new KB' → workflow(create) |
| `test_workflow_description_routes_change_over_time_to_transition` | change over time → transition |
| `test_workflow_description_routes_no_entity_to_ask` | no entity named → ASK |

### Build: zero warnings ✅

## Steps 1–6 Target: 6/6

The if/then routing rules cover all 6 routing decisions from the task spec:
1. 'build'/'create'/'start'/'new KB' → create ✅
2. 'add [new topic/entity]' → add ✅
3. 'add [note/flag/tag] to [existing entity]' → correct ✅
4. 'scan'/'index'/'reindex' → maintain ✅
5. 'check for new'/'look for updates'/'what's new' → refresh ✅
6. factual correction → correct IMMEDIATELY / change over time → transition / no entity → ASK ✅
