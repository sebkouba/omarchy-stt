#!/bin/bash
# Test script to verify the claude worktree tool works

# Set PROJECT_ROOT to the directory containing this script's parent
# NOTE: Update this to your actual project location if running from elsewhere
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Testing claude_worktree_wrapper.sh..."
echo "PROJECT_ROOT: $PROJECT_ROOT"
echo ""

# Test 1: Simple call without workspace
echo "Test 1: Creating test worktree without workspace..."
"$PROJECT_ROOT/tools/claude_worktree_wrapper.sh" \
    "$PROJECT_ROOT" \
    "test-feature-$(date +%s)" \
    "This is a test feature for verifying the tool works" \
    --no-terminal

if [ $? -eq 0 ]; then
    echo "✓ Test 1 passed"
else
    echo "✗ Test 1 failed"
    exit 1
fi

echo ""
echo "Test 2: Simulating with empty workspace (tests optional param handling)..."
# This simulates what happens when the LLM passes empty string for optional param
WORKSPACE=""
BRANCH="test-empty-workspace-$(date +%s)"
TASK="Test with empty workspace parameter"

"$PROJECT_ROOT/tools/claude_worktree_wrapper.sh" \
    "$PROJECT_ROOT" \
    "$BRANCH" \
    "$TASK" \
    --workspace "$WORKSPACE" \
    --no-terminal

if [ $? -eq 0 ]; then
    echo "✓ Test 2 passed"
else
    echo "✗ Test 2 failed"
    exit 1
fi

echo ""
echo "✓ All tests passed!"
echo ""
echo "Created worktrees in: $PROJECT_ROOT-worktrees/"
echo ""
echo "To clean up test worktrees:"
echo "  cd $PROJECT_ROOT"
echo "  git worktree list"
echo "  git worktree remove <path>"
