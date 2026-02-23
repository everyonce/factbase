#!/bin/bash
# Rollback Phase 6 migration by restoring database backup

set -e

DB_PATH="${FACTBASE_DB:-$HOME/.local/share/factbase/factbase.db}"
BACKUP_DIR="${FACTBASE_BACKUP_DIR:-$(dirname "$0")/../backups}"

# Find most recent backup
LATEST_BACKUP=$(ls -t "$BACKUP_DIR"/pre-phase6-*.db 2>/dev/null | head -1)

if [ -z "$LATEST_BACKUP" ]; then
    echo "No backup found in $BACKUP_DIR"
    exit 1
fi

echo "Restoring from: $LATEST_BACKUP"
echo "Target: $DB_PATH"
read -p "Continue? [y/N] " confirm

if [ "$confirm" != "y" ] && [ "$confirm" != "Y" ]; then
    echo "Aborted"
    exit 0
fi

cp "$LATEST_BACKUP" "$DB_PATH"
echo "Database restored. Run 'factbase scan' to verify."
