#!/bin/bash
# claude-worktree.sh - Create worktree and launch Claude Code (Hyprland version)

set -e

# Parse arguments
BRANCH_NAME="$1"
TASK_DESC="$2"
WORKSPACE=""
NO_TERMINAL=false
BACKGROUND=false

if [ -z "$BRANCH_NAME" ] || [ -z "$TASK_DESC" ]; then
    echo "Usage: $0 <branch-name> <task-description> [--workspace N] [--no-terminal] [--background]"
    exit 1
fi

shift 2
while [[ $# -gt 0 ]]; do
    case $1 in
        --workspace)
            WORKSPACE="$2"
            shift 2
            ;;
        --no-terminal)
            NO_TERMINAL=true
            shift
            ;;
        --background)
            BACKGROUND=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Check if we're in a git repo
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    echo "Error: Not in a git repository"
    exit 1
fi

# Get repo root and name
REPO_ROOT=$(git rev-parse --show-toplevel)
REPO_NAME=$(basename "$REPO_ROOT")

# Create worktree directory structure
WORKTREE_BASE="${REPO_ROOT}-worktrees"
WORKTREE_PATH="${WORKTREE_BASE}/${BRANCH_NAME}"

# Check if worktree already exists
if [ -d "$WORKTREE_PATH" ]; then
    echo "Error: Worktree already exists at $WORKTREE_PATH"
    exit 1
fi

# Create worktrees base directory if it doesn't exist
mkdir -p "$WORKTREE_BASE"

# Create the worktree
echo "Creating worktree at $WORKTREE_PATH with branch $BRANCH_NAME..."
git worktree add "$WORKTREE_PATH" -b "$BRANCH_NAME"

# Create instruction file for Claude
INSTRUCTION_FILE="${WORKTREE_PATH}/.claude-task.md"
cat > "$INSTRUCTION_FILE" << EOF
# Task

$TASK_DESC

# Instructions

Please complete the task described above. When finished, commit your changes with a descriptive commit message.
EOF

echo "Created task instructions at $INSTRUCTION_FILE"

# Detect terminal emulator (prioritize Wayland-native terminals)
detect_terminal() {
    if command -v kitty &> /dev/null; then
        echo "kitty"
    elif command -v alacritty &> /dev/null; then
        echo "alacritty"
    elif command -v foot &> /dev/null; then
        echo "foot"
    elif command -v wezterm &> /dev/null; then
        echo "wezterm"
    elif command -v gnome-terminal &> /dev/null; then
        echo "gnome-terminal"
    elif command -v konsole &> /dev/null; then
        echo "konsole"
    else
        echo ""
    fi
}

# Switch workspace if specified (Hyprland)
if [ -n "$WORKSPACE" ]; then
    if command -v hyprctl &> /dev/null; then
        hyprctl dispatch workspace "$WORKSPACE"
        # Small delay to ensure workspace switch completes
        sleep 0.1
    else
        echo "Warning: hyprctl not found, cannot switch workspace"
    fi
fi

# Launch based on flags
if [ "$NO_TERMINAL" = true ] || [ "$BACKGROUND" = true ]; then
    # Launch without terminal
    (cd "$WORKTREE_PATH" && claude --dangerously-skip-permissions "$(cat .claude-task.md)" > /dev/null 2>&1 &)
    echo "Claude launched in background"
else
    # Launch in new terminal window
    TERMINAL=$(detect_terminal)

    if [ -z "$TERMINAL" ]; then
        echo "Warning: No terminal emulator detected, launching in current terminal"
        cd "$WORKTREE_PATH"
        echo "Task: $TASK_DESC"
        echo ""
        claude --dangerously-skip-permissions "$(cat .claude-task.md)"
    else
        case $TERMINAL in
            kitty)
                kitty --directory "$WORKTREE_PATH" bash -c "echo 'Task: $TASK_DESC'; echo ''; claude --dangerously-skip-permissions \"\$(cat .claude-task.md)\"; exec bash" &
                ;;
            alacritty)
                alacritty --working-directory "$WORKTREE_PATH" -e bash -c "echo 'Task: $TASK_DESC'; echo ''; claude --dangerously-skip-permissions \"\$(cat .claude-task.md)\"; exec bash" &
                ;;
            foot)
                foot --working-directory "$WORKTREE_PATH" bash -c "echo 'Task: $TASK_DESC'; echo ''; claude --dangerously-skip-permissions \"\$(cat .claude-task.md)\"; exec bash" &
                ;;
            wezterm)
                wezterm start --cwd "$WORKTREE_PATH" bash -c "echo 'Task: $TASK_DESC'; echo ''; claude --dangerously-skip-permissions \"\$(cat .claude-task.md)\"; exec bash" &
                ;;
            gnome-terminal)
                gnome-terminal --working-directory="$WORKTREE_PATH" -- bash -c "echo 'Task: $TASK_DESC'; echo ''; claude --dangerously-skip-permissions \"\$(cat .claude-task.md)\"; exec bash" &
                ;;
            konsole)
                konsole --workdir "$WORKTREE_PATH" -e bash -c "echo 'Task: $TASK_DESC'; echo ''; claude --dangerously-skip-permissions \"\$(cat .claude-task.md)\"; exec bash" &
                ;;
        esac
        echo "Opened new $TERMINAL window with Claude"
    fi
fi

echo "Worktree created: $WORKTREE_PATH"
echo "Branch: $BRANCH_NAME"
echo ""
echo "To clean up when done:"
echo "  git worktree remove $WORKTREE_PATH"