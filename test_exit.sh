#!/bin/bash
# Test script to verify exit functionality

echo "Testing REPL exit functionality..."

# Test exit command
echo "exit" | timeout 5s cargo run --bin ae 2>/dev/null
if [ $? -eq 0 ]; then
    echo "✅ REPL 'exit' command works correctly"
else
    echo "❌ REPL 'exit' command failed"
fi

# Test quit command  
echo "quit" | timeout 5s cargo run --bin ae 2>/dev/null
if [ $? -eq 0 ]; then
    echo "✅ REPL 'quit' command works correctly"
else
    echo "❌ REPL 'quit' command failed"
fi

echo ""
echo "Note: TUI exit functionality (q, Esc, Ctrl+C) requires interactive testing"
echo "Launch with: cargo run --bin ae -- --tui"
echo "Then test: q, Esc, or Ctrl+C to exit"
