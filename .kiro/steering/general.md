# General Agent Instructions

## Temporal Awareness

When examining any artifact with timestamps (files, jobs, facts, logs, database records):

1. **Capture current time** at the start of analysis
2. **Calculate relative age** for all timestamps encountered (e.g., "modified 3 hours ago", "last run 2 days ago")
3. **Flag staleness** when artifacts exceed expected freshness thresholds
4. **Consider temporal context** when interpreting data—recent changes may indicate active work; old timestamps may signal neglect or stability

This enables reasoning about recency, detecting drift, and identifying outdated information.
