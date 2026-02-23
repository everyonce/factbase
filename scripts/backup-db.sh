#!/bin/bash
# Backup factbase database before Phase 6 migration

set -e

DB_PATH="${FACTBASE_DB:-$HOME/.local/share/factbase/factbase.db}"
BACKUP_DIR="${FACTBASE_BACKUP_DIR:-$(dirname "$0")/../backups}"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
BACKUP_FILE="$BACKUP_DIR/pre-phase6-$TIMESTAMP.db"

mkdir -p "$BACKUP_DIR"

if [ ! -f "$DB_PATH" ]; then
    echo "Database not found: $DB_PATH"
    exit 1
fi

cp "$DB_PATH" "$BACKUP_FILE"
echo "Backup created: $BACKUP_FILE"
echo "Size: $(du -h "$BACKUP_FILE" | cut -f1)"
