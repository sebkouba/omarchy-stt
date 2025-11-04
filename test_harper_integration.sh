#!/bin/bash
# Quick test of Harper integration

echo "=== Testing Harper Integration ==="
echo

# Create a test dictionary
mkdir -p ~/.config/transcribe-rs
cat > ~/.config/transcribe-rs/harper_dictionary.txt <<EOF
# Custom dictionary for testing
LLM
Parakeet
Automattic
EOF

echo "✅ Created test dictionary"
echo

# Create a test transcription with intentional errors
echo "Test transcription: 'This is a teh test of the Harper intergration.'"
echo

# Run Harper processor directly (not through full pipeline since we'd need daemon)
cargo run --example test-harper-direct

echo
echo "=== Binary Sizes ==="
ls -lh target/release/transcribe* | awk '{print $5, $9}'
echo

echo "=== To review corrections ==="
echo "Check: ~/.config/transcribe-rs/harper_corrections/"
echo
