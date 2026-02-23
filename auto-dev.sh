#!/bin/bash

exec > >(tee -a auto-dev.log) 2>&1

TASKS_FILE="tasks.md"
MODEL="claude-opus-4.5"
CYCLE_DELAY=30

echo "🚀 Starting automated development loop..."
echo "Press Ctrl+C to exit"

while true; do
    echo ""
    echo "=== Development Cycle $(TZ='America/Chicago' date +%r) ==="

    if [ -f "$TASKS_FILE" ]; then
        echo "📋 Checking for unfinished tasks..."
        if ! kiro-cli chat --trust-all-tools --no-interactive --model "$MODEL" \
            "Review ALL tasks in tasks.md and find the next unfinished or broken task.

            RULES:
            - Before each tool use, run 'date +%r' to log progress
            - NEVER run tests/processes that might stall without timeouts
            - Always use reasonable time limits on commands (e.g., timeout 60s)
            - NEVER modify auto-dev.sh or remove tasks.md

            IF TASK WAS PREVIOUSLY COMPLETED:
            - Briefly note what was done and commit ID
            - Move to next task

            IF COMPLETING A TASK:
            - Mark it complete in tasks.md
            - Add to the 'outcomes' section: summary, key considerations, difficulties encountered and solutions
            - Do not alter other parts of the file

            IF TASK IS TOO COMPLEX:
            - Do NOT skip to next task
            - Break it down into subtasks (add the new subtasks as Task XXa, XXb, XXc) within the file

            IF NO TASKS REMAIN:
            - Analyze codebase and add 2-3 specific tasks prioritizing:
              * Optimize existing code/tests
              * Reduce repetition, refactor out redundant processes to functions
              * Simplify deployment/usage
              * Consolidate tools or service calls"; then
            echo "❌ kiro-cli error detected, waiting 90s..."
            sleep 90
            continue
        fi
    else
        echo "📝 Creating initial tasks..."
        if ! kiro-cli chat --trust-all-tools --no-interactive --model "$MODEL" \
            "Analyze the codebase and create a tasks.md file with 3-5 specific tasks for enhancements, missing functionality, or tests."; then
            echo "❌ kiro-cli error detected, waiting 90s..."
            sleep 90
            continue
        fi
    fi

    echo "🧹 Performing cleanup and git commit..."
    if ! kiro-cli chat --trust-all-tools --no-interactive --model "$MODEL" \
        "Clean up temporary files, ensure code formatting is correct, and commit changes.
        NEVER modify/delete auto-dev.sh or tasks.md.
        Execute: git add -A && git commit -m '<descriptive-message>'"; then
        echo "❌ kiro-cli error detected, waiting 90s..."
        sleep 90
        continue
    fi

    echo "⏳ Waiting ${CYCLE_DELAY}s before next cycle..."
    sleep "$CYCLE_DELAY"
done
