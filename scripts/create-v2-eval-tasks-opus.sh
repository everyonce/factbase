#!/usr/bin/env bash
# Create 30 isolated v2 eval tasks for Opus model
set -euo pipefail

VIKUNJA_TOKEN="tk_ff251f3d3512775c71913bc2f8ec0dabbf5016a8"
BASE_URL="https://vikunja.home.everyonce.com/api/v1"
PROJECT_ID=2
BUCKET_ID=4
KB_PATH="/Volumes/dev/factbase-test/prompt-eval-opus"
RESULTS_FILE="/Users/daniel/work/factbase/docs/v2-results-opus.md"
MODEL="claude-opus-4.6"

create_task() {
  local step="$1"
  local title="$2"
  local prompt="$3"

  local desc="Restore baseline: git checkout eval-v2-baseline -- .

${prompt}

Record the FIRST tool call made. Write to ${RESULTS_FILE} row ${step}

Model: ${MODEL}"

  local full_title="[kb:${KB_PATH}] Step ${step}/30 (opus) — ${title}"

  local id
  id=$(curl -sf -H "Authorization: Bearer $VIKUNJA_TOKEN" \
    "$BASE_URL/projects/$PROJECT_ID/tasks" -X PUT \
    -H "Content-Type: application/json" \
    -d "$(jq -n --arg t "$full_title" --arg d "$desc" --argjson b "$BUCKET_ID" \
      '{title: $t, description: $d, bucket_id: $b}')" \
    | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")

  echo "  Step $step: task $id"
}

echo "Creating 30 Sonnet eval tasks..."

# Workflow Routing (1-6)
create_task 1 "workflow routing: create" \
  'Build me a KB about jazz standards history'

create_task 2 "workflow routing: add new entity" \
  'Add Clifford Brown'

create_task 3 "workflow routing: add note to existing" \
  'Add a note to Miles Davis that he was known for his use of the Harmon mute'

create_task 4 "workflow routing: scan" \
  'Scan the KB'

create_task 5 "workflow routing: refresh" \
  'Check for new recordings or releases related to jazz standards'

create_task 6 "workflow routing: correct" \
  'Fix a wrong fact about John Coltrane'

# Correct vs Transition (7-10)
create_task 7 "correct vs transition: false claim" \
  "Miles Davis didn't play trumpet"

create_task 8 "correct vs transition: name change" \
  'Village Vanguard is now called The Village Vanguard Jazz Club'

create_task 9 "correct vs transition: disputed flag" \
  'Add a disputed flag to Kind of Blue'

create_task 10 "correct vs transition: correction with dates" \
  'The claim that Miles Davis formed his first great quintet in 1956 is wrong — it was 1955. Fix it with the correct dates.'

# Clarification (11-13)
create_task 11 "clarification: ambiguous entity" \
  'Fix John'

create_task 12 "clarification: no entity named" \
  'Update it'

create_task 13 "clarification: sensible default" \
  'Make it better'

# Conflict Detection (14-17)
create_task 14 "conflict detection: concurrent facts" \
  'Add to Miles Davis: he was recording Kind of Blue in April 1959 and also performing at the Village Vanguard in April 1959'

create_task 15 "conflict detection: overlapping roles" \
  'Add to Miles Davis: he led the Miles Davis Quintet from 1955 to 1959, and he also led the Miles Davis Quintet from 1957 to 1961'

create_task 16 "conflict detection: join + role same date" \
  'Add to Miles Davis: he signed with Columbia Records in 1955 and started his quintet in 1955'

create_task 17 "conflict detection: contradictory facts" \
  'Add to Miles Davis: he was the primary composer of Kind of Blue in 1959, and also that Bill Evans was the primary composer of Kind of Blue in 1959'

# Citation Quality (18-21)
create_task 18 "citation quality: full URL citation" \
  'Add a fact about Kind of Blue: it was recorded on March 2 and April 22, 1959. Source: https://www.allmusic.com/album/kind-of-blue-mw0000191710'

create_task 19 "citation quality: vague citation" \
  'Add a fact about Kind of Blue: it was the best-selling jazz album of all time. Source: email from jazz historian, 2025'

create_task 20 "citation quality: dismiss valid citation" \
  'Dismiss the citation for the Blue Note Records founding year — it'"'"'s a valid internal source'

create_task 21 "citation quality: phonetool citation" \
  'The source for Miles Davis'"'"'s birth date is from phonetool user mdavis'

# Temporal Questions (22-25)
create_task 22 "temporal questions: fact without @t" \
  'Add a fact about Miles Davis: he was born in Alton, Illinois'

create_task 23 "temporal questions: stable capability" \
  'Add a fact about Kind of Blue: it is available on all major streaming platforms'

create_task 24 "temporal questions: open-ended range" \
  'Add a fact about John Coltrane: he has been playing saxophone since 1943'

create_task 25 "temporal questions: resolve with search" \
  'When did Miles Davis first record with Columbia Records? Find the answer and add it with a source.'

# Glossary (26-28)
create_task 26 "glossary: known acronym" \
  'Add a fact about Kind of Blue: it was recorded at Columbia'"'"'s 30th Street Studio (CBS)'

create_task 27 "glossary: unknown term" \
  'Add a fact about Miles Davis: he used a Harmon mute (HM) in his recordings'

create_task 28 "glossary: create glossary entry" \
  "Add HM to the glossary as 'Harmon mute, a type of mute used in trumpet playing'"

# Authoring Quality (29-30)
create_task 29 "authoring quality: no sources" \
  "Create a document about Thelonious Monk's compositions"

create_task 30 "authoring quality: proper @t + citations" \
  "Create a document about John Coltrane's A Love Supreme with proper dates and citations"

echo "Done."
