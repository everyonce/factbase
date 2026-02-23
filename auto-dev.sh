#!/bin/bash

exec > >(tee -a auto-dev.log) 2>&1

TASKS_DIR="tasks"
MODEL="claude-opus-4.6"
CYCLE_DELAY=10

echo "🚀 Starting automated development loop..."
echo "Press Ctrl+C to exit"

while true; do
    echo ""
    echo "=== Development Cycle $(TZ='America/Chicago' date +%r) ==="

    if [ -d "$TASKS_DIR" ]; then
        echo "📋 Checking for unfinished tasks..."
        if ! kiro-cli chat --trust-all-tools --no-interactive --model "$MODEL" \
            "First, read tasks.md and identify which phases or tasks are marked incomplete ([ ]).
            
            If the main tasks.md file does not have its own tasks, then review ONLY the phase files in tasks/ directory that correspond to incomplete phases.
            Find the next unfinished or broken task in those files.  Additionally, pay attention to recent 
                OUTCOMES details to inform any potential concerns you should have for your own task.
                Consider optional tasks required.

            RULES:
            - Before each tool use, run 'date +%r' to log progress
            - NEVER run tests/processes that might stall without timeouts
            - Always use reasonable time limits on commands (e.g., timeout 60s)
            - NEVER modify/remove auto-dev.sh or remove tasks.md or tasks/ directory

            IF TASK WAS PREVIOUSLY COMPLETED:
            - Briefly note what was done and commit ID
            - Move to next task

            IF COMPLETING A TASK:
            - Mark it complete in the appropriate phase file
            - Add to the 'outcomes' section: summary, key considerations, difficulties encountered and solutions
            - Do not alter other parts of the file
            - Do not move on to next task, only finish one task well.

            IF TASK IS TOO COMPLEX:
            - Do NOT skip to next task
            - Break it down into subtasks (add the new subtasks as Task XXa, XXb, XXc) within the file

            IF ALL TASKS IN A PHASE ARE COMPLETE:
            - Mark that phase as [x] in tasks.md
            - Move to next incomplete phase

            IF ALL PHASES COMPLETE:
            - Analyze codebase and add 2-3 specific tasks to the latest phase prioritizing:
              * Optimize existing code/tests
              * Reduce repetition, refactor out redundant processes to functions
              * Simplify deployment/usage
              * Consolidate tools or service calls"; then
            echo "❌ kiro-cli error detected, waiting 90s..."
            sleep 90
            continue
        fi
    else
        echo "📝 Creating initial tasks directory..."
        if ! kiro-cli chat --trust-all-tools --no-interactive --model "$MODEL" \
            "Create a tasks/ directory with phase1.md file containing 3-5 specific tasks for enhancements, missing functionality, or tests."; then
            echo "❌ kiro-cli error detected, waiting 90s..."
            sleep 90
            continue
        fi
    fi

    echo "🧹 Performing task prep..."
    if ! kiro-cli chat --trust-all-tools --no-interactive --model "$MODEL" \
        "You are a task and context prep agent.  Your job is to ensure that we give task execution agents comprehensive information about their
           task(s) and we don't waste context on irrelevant information.  In order to accomplish that, use these steps:
           1. Are any previous phases or larger tasks actually complete due to all their subtasks being complete?  Update the 
                tasks.md file to show high-level tasks complete if their subtasks are complete.
           2. Any complete previous full phases, move the details of those items appended to the end of tasks_old.md.  (Remove their main and subtasks from tasks.md and append them to tasks_old.md)
           3. Review the full text of tasks_old.md.  Summarize the important learnings, including any details that would be generally helpful for agents completing future tasks.  
                Add those details and general summarizations of past tasks to the top of tasks.md for future tasks to have in context (replace previous summarizations)
           4. Ensure the steering document .kiro/steering/current-state.md is accurate based on what you've leared from the task files."; then
        echo "❌ kiro-cli error detected, waiting 90s..."
        sleep 90
        continue
    fi

    echo "🧹 Performing cleanup and git commit..."
    if ! kiro-cli chat --trust-all-tools --no-interactive --model "$MODEL" \
        "Increment a version number.  If it doesn't exist yet, create one as X.Y.Z  
        X should be the task phase, and Update Y for larger task groupings and Z for each subtask.  
        Clean up temporary files, ensure code formatting is correct, and commit changes.
        NEVER modify/delete auto-dev.sh or tasks/ directory.
        Execute: git add -A && git commit -m '<descriptive-message>'"; then
        echo "❌ kiro-cli error detected, waiting 90s..."
        sleep 90
        continue
    fi

    echo "⏳ Waiting ${CYCLE_DELAY}s before next cycle..."
    sleep "$CYCLE_DELAY"
done
