#!/bin/bash
# Wrapper script that supports project selection from projects.md

set -e

# Parse arguments
PROJECT_PATH=""
BRANCH_NAME=""
TASK_DESC=""
WORKSPACE=""
EXTRA_ARGS=()

# Check if first argument is a path (contains /)
if [[ "$1" == */* ]]; then
    PROJECT_PATH="$1"
    shift
fi

BRANCH_NAME="$1"
TASK_DESC="$2"

if [ -z "$BRANCH_NAME" ] || [ -z "$TASK_DESC" ]; then
    echo "Usage: $0 [project-path] <branch-name> <task-description> [--workspace N] [other options...]"
    exit 1
fi

shift 2

# Parse remaining options
while [[ $# -gt 0 ]]; do
    case $1 in
        --workspace)
            WORKSPACE="$2"
            shift 2
            ;;
        *)
            # Pass through any other arguments (--no-terminal, --background, etc.)
            EXTRA_ARGS+=("$1")
            shift
            ;;
    esac
done

# If project path provided, change to it first
if [ -n "$PROJECT_PATH" ]; then
    if [ ! -d "$PROJECT_PATH" ]; then
        echo "Error: Project path does not exist: $PROJECT_PATH"
        exit 1
    fi
    cd "$PROJECT_PATH"
fi

# Find the claude-worktree.sh script
# Look in current directory, or fall back to the one in transcribe-rs-v2
SCRIPT_PATH=""
if [ -f "./claude-worktree.sh" ]; then
    SCRIPT_PATH="./claude-worktree.sh"
# NOTE: Update this fallback path to your actual project location if needed
elif [ -f "$HOME/code/transcribe-rs-v2/claude-worktree.sh" ]; then
    SCRIPT_PATH="$HOME/code/transcribe-rs-v2/claude-worktree.sh"
else
    echo "Error: claude-worktree.sh not found"
    exit 1
fi

# Build arguments for the script
ARGS=("$BRANCH_NAME" "$TASK_DESC")
if [ -n "$WORKSPACE" ]; then
    ARGS+=("--workspace" "$WORKSPACE")
fi
# Add any extra arguments passed through
ARGS+=("${EXTRA_ARGS[@]}")

# Execute the script
exec "$SCRIPT_PATH" "${ARGS[@]}"
