#!/bin/bash

# Quick test script to iterate on prompt variations

echo "=== Testing Prompt Variations ==="
echo ""

# Clear the log so we can see fresh results
> /tmp/ptt_rust_debug.log

echo "Running test with current prompt..."
./target/release/examples/test_real_prompt

echo ""
echo "=== Results ==="
echo ""
echo "Did it call tools?"
tail -30 /tmp/ptt_rust_debug.log | grep -q "finish_reason.*tool_calls" && echo "✅ YES - Tool called!" || echo "❌ NO - Just cleaned text"

echo ""
echo "Last finish_reason:"
tail -30 /tmp/ptt_rust_debug.log | grep "finish_reason"

echo ""
echo "Full response:"
tail -30 /tmp/ptt_rust_debug.log | grep "Model did not request tool calls\|Tool result"
