# Factbase Prompt Eval v2 — Sonnet Results

| Step | Task | First Tool Call | Correct? |
|------|------|----------------|----------|
| 1 | workflow routing: create — "Build me a KB about jazz standards history" | `workflow(workflow='create', step=1, domain='jazz standards history', path='/Volumes/dev/factbase-test/prompt-eval-v2')` | ✅ |
| 2 | workflow routing: add new entity — "Add Clifford Brown" | `workflow(workflow='add', topic='Clifford Brown')` | ✅ |
| 3 | workflow routing: add note to existing — "Add a note to Miles Davis that he was known for his use of the Harmon mute" | `workflow(workflow='correct', correction='Miles Davis was known for his use of the Harmon mute')` | ✅ |
| 4 | workflow routing: scan | `workflow(workflow='maintain', step=1)` | ✅ |
| 5 | workflow routing: refresh — "Check for new recordings or releases related to jazz standards" | `workflow(workflow='refresh', step=1, topic='jazz standards new recordings or releases')` | ✅ |
| 6 | workflow routing: correct — Fix a wrong fact about John Coltrane | `workflow(workflow='correct', correction='Wrong fact about John Coltrane')` | ✅ |
| 7 | correct vs transition: false claim — Miles Davis didn't play trumpet | `workflow(workflow='correct', step=1, correction='Miles Davis did play trumpet...')` | ✅ |
| 8 | correct vs transition: name change — Village Vanguard is now called The Village Vanguard Jazz Club | `workflow(workflow='transition', change='Village Vanguard is now called The Village Vanguard Jazz Club')` | ✅ |
| 9 | correct vs transition: disputed flag — Add a disputed flag to Kind of Blue | `workflow(workflow='correct', correction='Add a disputed flag to Kind of Blue')` | ✅ |
| 10 | correct vs transition: correction with dates — Miles Davis first great quintet was 1955 not 1956 | `workflow(workflow='correct', correction='Miles Davis formed his first great quintet in 1955, not 1956...')` | ✅ |
| 11 | clarification: ambiguous entity — Fix John | `search(query='John')` | ✅ |
| 12 | clarification: no entity named — Update it | asked clarifying question (no KB tool call) | ✅ |
| 13 | clarification: sensible default — Make it better | `workflow(workflow='maintain', step=1)` | ✅ |
| 14 | conflict detection: concurrent facts — Add to Miles Davis: recording Kind of Blue in April 1959 AND performing at Village Vanguard in April 1959 | `workflow(workflow='correct', correction='Add two facts to Miles Davis: (1) he was recording Kind of Blue in April 1959, and (2) he was also performing at the Village Vanguard in April 1959')` | ✅ |
| 15 | conflict detection: overlapping roles — Add to Miles Davis: led Quintet 1955–1959 AND led Quintet 1957–1961 | `workflow(workflow='correct', correction='Miles Davis led the Miles Davis Quintet from 1955 to 1959, and he also led the Miles Davis Quintet from 1957 to 1961')` | ✅ |
| 16 | conflict detection: join + role same date — Add to Miles Davis: signed with Columbia Records in 1955 AND started his quintet in 1955 | `workflow(workflow='correct', correction='Miles Davis signed with Columbia Records in 1955 and started his quintet in 1955')` | ✅ |
| 17 | conflict detection: contradictory facts — Add to Miles Davis: he was the primary composer of Kind of Blue in 1959, AND that Bill Evans was the primary composer of Kind of Blue in 1959 | `factbase(op='list', title_filter='Miles Davis')` | ❌ |
| 18 | citation quality: full URL citation — Add fact about Kind of Blue: recorded on March 2 and April 22, 1959. Source: https://www.allmusic.com/album/kind-of-blue-mw0000191710 | `workflow(workflow='correct', correction='Kind of Blue was recorded on March 2 and April 22, 1959.', source='https://www.allmusic.com/album/kind-of-blue-mw0000191710')` | ✅ |
| 19 | citation quality: vague citation — Add fact about Kind of Blue: best-selling jazz album of all time. Source: email from jazz historian, 2025 | `workflow(workflow='correct', correction='Add fact: Kind of Blue was the best-selling jazz album of all time.', source='email from jazz historian, 2025')` | ✅ |
| 20 | citation quality: dismiss valid citation — Dismiss the citation for the Blue Note Records founding year (valid internal source) | `factbase(op='review_queue')` | ❌ |
| 21 | citation quality: phonetool citation — The source for Miles Davis's birth date is from phonetool user mdavis | `factbase(op='answer', doc_id='miles-davis', question_index=0, answer='May 26, 1926', confidence='believed')` | ❌ |
| 22 | temporal questions: fact without @t — Add a fact about Miles Davis: he was born in Alton, Illinois | `workflow(workflow='correct', correction='Miles Davis was born in Alton, Illinois')` | ✅ |
| 23 | temporal questions: stable capability — Add a fact about Kind of Blue: available on all major streaming platforms | `workflow(workflow='correct', correction='Kind of Blue is available on all major streaming platforms')` | ✅ |
| 24 | temporal questions: open-ended range — Add a fact about John Coltrane: he has been playing saxophone since 1943 | `workflow(workflow='correct', correction='John Coltrane has been playing saxophone since 1943')` | |
| 25 | temporal questions: resolve with search — When did Miles Davis first record with Columbia Records? | `factbase(op='review_queue')` | |
| 26 | glossary: known acronym — Add a fact about Kind of Blue: it was recorded at Columbia's 30th Street Studio (CBS) | `workflow(workflow='correct', correction='Kind of Blue was recorded at Columbia\'s 30th Street Studio (CBS)')` | ✅ |
| 27 | glossary: unknown term — Add a fact about Miles Davis: he used a Harmon mute (HM) in his recordings | `workflow(workflow='correct', correction='Miles Davis used a Harmon mute (HM) in his recordings')` | |
| 28 | glossary: create glossary entry — Add HM to the glossary as 'Harmon mute, a type of mute used in trumpet playing' | `factbase(op='list', doc_type='glossary')` | |
| 29 | authoring quality: no sources — Create a document about Thelonious Monk's compositions | `workflow(workflow='add', topic="Thelonious Monk's compositions")` | |
| 30 | authoring quality: proper @t + citations — Create a document about John Coltrane's A Love Supreme with proper dates and citations | `factbase(op='create', path='a-love-supreme.md', title='A Love Supreme — John Coltrane', content=...)` | ✅ |
