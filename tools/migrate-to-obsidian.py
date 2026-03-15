#!/usr/bin/env python3
"""Migrate a factbase KB from HTML-comment format to Obsidian-compatible format.

Converts:
- <!-- factbase:abc123 --> to YAML frontmatter with factbase_id
- [[hex_id]] links to [[Entity Name]] wikilinks
- Review queue sections to collapsed Obsidian callouts
- Inline <!-- reviewed:YYYY-MM-DD --> markers to frontmatter reviewed: date
- Updates perspective.yaml to add format preset

Usage:
  python3 migrate-to-obsidian.py /path/to/kb [--dry-run]
"""

import os
import re
import sys
import sqlite3
import yaml

def load_id_to_title(kb_path):
    """Load document ID→title mapping from factbase DB."""
    db_path = os.path.join(kb_path, '.factbase', 'factbase.db')
    if not os.path.exists(db_path):
        print(f"ERROR: No factbase DB at {db_path}")
        print("Run 'factbase scan' first to create the database.")
        sys.exit(1)
    
    conn = sqlite3.connect(db_path)
    cursor = conn.execute("SELECT id, title, file_path FROM documents")
    mapping = {}
    for row in cursor:
        doc_id, title, file_path = row
        mapping[doc_id] = {'title': title, 'file_path': file_path}
    conn.close()
    return mapping

def convert_review_to_callout(content):
    """Convert plain review queue section to Obsidian collapsed callout format.
    
    Before:
        ---
        
        ## Review Queue
        
        <!-- factbase:review -->
        - [ ] `@q[temporal]` ...
          > 
    
    After:
        > [!review]- Review Queue
        > - [ ] `@q[temporal]` ...
        >   > 
    """
    marker = '<!-- factbase:review -->'
    callout_header = '> [!review]- Review Queue'
    legacy_callout_header = '> [!info]- Review Queue'
    if marker not in content:
        return content
    # Already in callout format (check for callout header line)
    lines_check = content.split('\n')
    if any(l.strip() in (callout_header, legacy_callout_header) for l in lines_check):
        return content
    
    # Find the review section start (look for --- before ## Review Queue)
    lines = content.split('\n')
    marker_idx = None
    heading_idx = None
    separator_idx = None
    
    for i, line in enumerate(lines):
        if line.strip() == marker:
            marker_idx = i
        if line.strip() == '## Review Queue':
            heading_idx = i
    
    if marker_idx is None:
        return content
    
    # Find the separator (---) before the heading
    start_idx = heading_idx if heading_idx is not None else marker_idx
    sep_idx = start_idx
    while sep_idx > 0:
        prev = lines[sep_idx - 1].strip()
        if prev == '---':
            separator_idx = sep_idx - 1
            sep_idx -= 1
        elif prev == '' or prev == '## Review Queue':
            sep_idx -= 1
        else:
            break
    
    # Determine where the review section starts
    section_start = separator_idx if separator_idx is not None else (heading_idx if heading_idx is not None else marker_idx)
    
    # Strip trailing blank lines from body
    body_end = section_start
    while body_end > 0 and lines[body_end - 1].strip() == '':
        body_end -= 1
    
    # Collect review content lines (from AFTER marker onward — skip the marker itself)
    review_lines = lines[marker_idx + 1:]
    
    # Build result: callout header only, no HTML marker inside
    result_lines = lines[:body_end]
    result_lines.append('')
    result_lines.append(callout_header)
    for line in review_lines:
        if line.strip() == '':
            result_lines.append('>')
        else:
            result_lines.append(f'> {line}')
    
    return '\n'.join(result_lines)

REVIEWED_RE = re.compile(r'<!-- reviewed:(\d{4}-\d{2}-\d{2})\b.*?-->')

def tags_from_path(relative_path):
    """Derive tags from a relative file path.
    
    Uses all directory components; if multiple, skips the first (top-level
    category folder).  Mirrors the Rust tags_from_path() logic.
    
    Examples:
      customers/xsolis/people/zach-evans.md -> ['xsolis', 'people']
      services/amazon-aurora.md             -> ['services']
      doc.md                                -> []
    """
    parts = relative_path.replace('\\', '/').split('/')
    dirs = parts[:-1]  # exclude filename
    if len(dirs) > 1:
        return dirs[1:]
    return dirs


# Mirrors STRUCTURAL_FOLDERS in src/processor/core.rs
STRUCTURAL_FOLDERS = {
    'archive', 'archived', 'old', 'inactive', 'deprecated', 'drafts', 'temp',
}


def derive_type_from_path(relative_path):
    """Derive document type from folder structure, mirroring Rust derive_type().

    Skips structural/organizational folder names and uses the grandparent instead.

    Examples:
      customers/xsolis/people/archive/john.md -> 'people' (skips archive)
      services/deprecated/old-api.md          -> 'service' (skips deprecated)
      people/john.md                          -> 'people'
      doc.md                                  -> 'document'
    """
    parts = relative_path.replace('\\', '/').split('/')
    dirs = parts[:-1]   # directory components only
    filename_stem = os.path.splitext(parts[-1])[0]

    if not dirs:
        return 'document'

    parent = dirs[-1]

    # Skip structural folders — use grandparent
    if parent.lower() in STRUCTURAL_FOLDERS:
        if len(dirs) >= 2:
            return _normalize_type(dirs[-2])
        return 'document'

    # Entity-folder convention: xsolis/xsolis.md -> use grandparent
    if parent.lower() == filename_stem.lower() and len(dirs) >= 2:
        return _normalize_type(dirs[-2])

    return _normalize_type(parent)


def _normalize_type(word):
    """Lowercase and strip trailing 's' (naive singularization)."""
    lower = word.lower()
    if lower.endswith('s') and len(lower) > 1:
        return lower[:-1]
    return lower
def merge_path_tags_into_frontmatter(content, path_tags):
    """Merge path-derived tags into existing YAML frontmatter.
    
    Path tags come first; existing user tags are appended if not already present.
    Assumes content starts with '---\\n'.
    """
    fm_end = content.find('\n---\n', 4)
    if fm_end < 0:
        return content
    
    fm_text = content[4:fm_end]
    
    # Parse existing tags
    existing_tags = []
    tags_match = re.search(r'^tags:\s*(.+)$', fm_text, re.MULTILINE)
    if tags_match:
        raw = tags_match.group(1).strip()
        if raw.startswith('[') and raw.endswith(']'):
            existing_tags = [t.strip() for t in raw[1:-1].split(',') if t.strip()]
        elif raw:
            existing_tags = [raw]
    
    # Merge: path tags first, then user tags not already present
    merged = list(path_tags)
    for tag in existing_tags:
        if tag not in merged:
            merged.append(tag)
    
    tags_line = f"tags: [{', '.join(merged)}]"
    
    if tags_match:
        fm_text = re.sub(r'^tags:.*$', tags_line, fm_text, flags=re.MULTILINE)
    else:
        fm_text = fm_text.rstrip('\n') + f'\n{tags_line}'
    
    return f"---\n{fm_text}\n---\n{content[fm_end+5:]}"


def set_frontmatter_type(content, doc_type):
    """Set or update the type: field in YAML frontmatter.
    
    Inserts after factbase_id: if present, otherwise appends.
    Assumes content starts with '---\\n'.
    """
    fm_end = content.find('\n---\n', 4)
    if fm_end < 0:
        return content
    
    fm_text = content[4:fm_end]
    after = content[fm_end + 5:]
    type_line = f"type: {doc_type}"
    
    if re.search(r'^type:', fm_text, re.MULTILINE):
        fm_text = re.sub(r'^type:.*$', type_line, fm_text, flags=re.MULTILINE)
    else:
        lines = fm_text.split('\n')
        insert_pos = next(
            (i + 1 for i, l in enumerate(lines) if l.startswith('factbase_id:')),
            len(lines)
        )
        lines.insert(insert_pos, type_line)
        fm_text = '\n'.join(lines)
    
    return f"---\n{fm_text}\n---\n{after}"

def convert_reviewed_to_frontmatter(content):
    """Strip inline <!-- reviewed:YYYY-MM-DD --> markers and store latest date in frontmatter."""
    dates = REVIEWED_RE.findall(content)
    if not dates:
        return content
    
    # Find the latest date
    latest = max(dates)
    
    # Strip all inline reviewed markers and clean trailing whitespace
    stripped = REVIEWED_RE.sub('', content)
    lines = [line.rstrip() for line in stripped.split('\n')]
    content = '\n'.join(lines)
    
    # Add/update reviewed: in frontmatter
    if content.startswith('---\n'):
        fm_end = content.find('\n---\n', 4)
        if fm_end > 0:
            fm_text = content[4:fm_end]
            # Check if reviewed: already exists
            if re.search(r'^reviewed:', fm_text, re.MULTILINE):
                fm_text = re.sub(r'^reviewed:.*$', f'reviewed: {latest}', fm_text, flags=re.MULTILINE)
            else:
                fm_text += f'\nreviewed: {latest}'
            content = f"---\n{fm_text}\n---\n{content[fm_end+5:]}"
        return content
    
    # No frontmatter — create one
    return f"---\nreviewed: {latest}\n---\n{content}"

def convert_file(filepath, id_map, kb_path, dry_run=False):
    """Convert a single markdown file to Obsidian format."""
    with open(filepath, 'r') as f:
        content = f.read()
    
    original = content
    
    # 1. Convert <!-- factbase:ID --> to YAML frontmatter
    match = re.match(r'^<!-- factbase:(\w+) -->\s*\n', content)
    if not match:
        return False  # Not a factbase doc
    
    factbase_id = match.group(1)
    content = content[match.end():]
    
    # Check if there's already YAML frontmatter
    if content.startswith('---\n'):
        # Merge factbase_id into existing frontmatter
        fm_end = content.find('\n---\n', 4)
        if fm_end > 0:
            fm_text = content[4:fm_end]
            fm = yaml.safe_load(fm_text) or {}
            fm['factbase_id'] = factbase_id
            content = f"---\n{yaml.dump(fm, default_flow_style=False).strip()}\n---\n{content[fm_end+5:]}"
    else:
        # Add new YAML frontmatter
        content = f"---\nfactbase_id: {factbase_id}\n---\n{content}"
    
    # 2. Convert [[hex_id]] links to [[Entity Name]] wikilinks
    def replace_link(m):
        link_id = m.group(1)
        if link_id in id_map:
            title = id_map[link_id]['title']
            return f"[[{title}]]"
        return m.group(0)  # Leave unchanged if not found
    
    content = re.sub(r'\[\[([a-f0-9]{6})\]\]', replace_link, content)
    
    # 3. Convert Links: block references
    # Pattern: - [[hex_id]] Title  →  - [[Title]]
    def replace_link_line(m):
        link_id = m.group(1)
        if link_id in id_map:
            title = id_map[link_id]['title']
            return f"- [[{title}]]"
        return m.group(0)
    
    content = re.sub(r'^- \[\[([a-f0-9]{6})\]\]\s+.*$', replace_link_line, content, flags=re.MULTILINE)
    
    # 4. Convert review queue section to collapsed callout
    content = convert_review_to_callout(content)
    
    # 5. Convert inline reviewed markers to frontmatter
    content = convert_reviewed_to_frontmatter(content)
    
    # 6. Add path-derived tags to frontmatter (merge with existing)
    rel_path = os.path.relpath(filepath, kb_path)
    path_tags = tags_from_path(rel_path)
    if path_tags and content.startswith('---\n'):
        content = merge_path_tags_into_frontmatter(content, path_tags)
    
    # 7. Write derived type to frontmatter
    if content.startswith('---\n'):
        doc_type = derive_type_from_path(rel_path)
        content = set_frontmatter_type(content, doc_type)
    
    if content == original:
        return False
    
    if dry_run:
        print(f"  WOULD convert: {filepath}")
        return True
    
    with open(filepath, 'w') as f:
        f.write(content)
    return True

def update_perspective(kb_path, dry_run=False):
    """Add format preset to perspective.yaml."""
    persp_path = os.path.join(kb_path, 'perspective.yaml')
    if not os.path.exists(persp_path):
        print(f"WARNING: No perspective.yaml at {persp_path}")
        return
    
    with open(persp_path, 'r') as f:
        persp = yaml.safe_load(f) or {}
    
    if 'format' in persp:
        print(f"  perspective.yaml already has format config, skipping")
        return
    
    persp['format'] = {'preset': 'obsidian'}
    
    if dry_run:
        print(f"  WOULD update: {persp_path} (add format.preset: obsidian)")
        return
    
    with open(persp_path, 'w') as f:
        yaml.dump(persp, f, default_flow_style=False, sort_keys=False)
    print(f"  Updated: {persp_path}")

def migrate_kb(kb_path, dry_run=False):
    """Migrate a KB to Obsidian format."""
    print(f"\n{'DRY RUN: ' if dry_run else ''}Migrating: {kb_path}")
    
    id_map = load_id_to_title(kb_path)
    print(f"  Loaded {len(id_map)} document IDs from DB")
    
    converted = 0
    skipped = 0
    
    for root, dirs, files in os.walk(kb_path):
        # Skip hidden dirs
        dirs[:] = [d for d in dirs if not d.startswith('.')]
        for f in files:
            if not f.endswith('.md'):
                continue
            filepath = os.path.join(root, f)
            if convert_file(filepath, id_map, kb_path, dry_run):
                converted += 1
            else:
                skipped += 1
    
    print(f"  Converted: {converted} files")
    print(f"  Skipped: {skipped} files (no factbase header or unchanged)")
    
    update_perspective(kb_path, dry_run)
    
    return converted

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: migrate-to-obsidian.py /path/to/kb [--dry-run]")
        sys.exit(1)
    
    kb_path = sys.argv[1]
    dry_run = '--dry-run' in sys.argv
    
    if not os.path.isdir(kb_path):
        print(f"ERROR: {kb_path} is not a directory")
        sys.exit(1)
    
    migrate_kb(kb_path, dry_run)
    
    if not dry_run:
        print("\nDone! Run 'factbase scan' to re-index the migrated KB.")
