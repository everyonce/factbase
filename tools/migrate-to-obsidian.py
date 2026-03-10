#!/usr/bin/env python3
"""Migrate a factbase KB from HTML-comment format to Obsidian-compatible format.

Converts:
- <!-- factbase:abc123 --> to YAML frontmatter with factbase_id
- [[hex_id]] links to [[Entity Name]] wikilinks
- Review queue sections to collapsed Obsidian callouts
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
        > [!info]- Review Queue
        > <!-- factbase:review -->
        > - [ ] `@q[temporal]` ...
        >   > 
    """
    marker = '<!-- factbase:review -->'
    if marker not in content:
        return content
    # Already in callout format
    if f'> {marker}' in content:
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
    
    # Collect review content lines (from marker onward)
    review_lines = lines[marker_idx:]
    
    # Build result
    result_lines = lines[:body_end]
    result_lines.append('')
    result_lines.append('> [!info]- Review Queue')
    for line in review_lines:
        if line.strip() == '':
            result_lines.append('>')
        else:
            result_lines.append(f'> {line}')
    
    return '\n'.join(result_lines)

def convert_file(filepath, id_map, dry_run=False):
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
    
    # Detect doc_type from file path
    rel_path = os.path.relpath(filepath, os.path.dirname(filepath))
    
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
            if convert_file(filepath, id_map, dry_run):
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
