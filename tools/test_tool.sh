#!/bin/bash
# Test script to verify the claude worktree tool works

echo "Testing claude_worktree_wrapper.sh..."
echo ""

# Test 1: Simple call without workspace
echo "Test 1: Creating test worktree without workspace..."
/home/seb/code/cloned/transcribe-rs-v2/tools/claude_worktree_wrapper.sh \
    /home/seb/code/cloned/transcribe-rs-v2 \
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

/home/seb/code/cloned/transcribe-rs-v2/tools/claude_worktree_wrapper.sh \
    /home/seb/code/cloned/transcribe-rs-v2 \
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
echo "Created worktrees in: /home/seb/code/cloned/transcribe-rs-v2-worktrees/"
echo ""
echo "To clean up test worktrees:"
echo "  cd /home/seb/code/cloned/transcribe-rs-v2"
echo "  git worktree list"
echo "  git worktree remove <path>"
