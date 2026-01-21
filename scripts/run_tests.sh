#!/bin/bash
# AetherShell Test Runner Script
# Run all .ae test files and report results

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

TEST_DIR="${1:-tests}"
VERBOSE="${2:-}"

echo -e "${CYAN}=== AetherShell Test Runner ===${NC}"
echo ""

# Build first
echo -e "${CYAN}Building AetherShell...${NC}"
if ! cargo build --bin ae 2>/dev/null; then
    echo -e "${RED}Build failed!${NC}"
    exit 1
fi
echo -e "${GREEN}Build successful${NC}"
echo ""

# Find ae binary
AE_BIN="./target/debug/ae"
if [ ! -f "$AE_BIN" ]; then
    AE_BIN="./target/release/ae"
fi

if [ ! -f "$AE_BIN" ]; then
    echo -e "${RED}Could not find ae binary!${NC}"
    exit 1
fi

# Collect test files
TOTAL=0
PASSED=0
FAILED=0
FAILED_TESTS=""

# Function to run tests from a directory
run_tests() {
    local dir="$1"
    local pattern="$2"
    
    if [ ! -d "$dir" ]; then
        return
    fi
    
    for test_file in "$dir"/$pattern; do
        if [ ! -f "$test_file" ]; then
            continue
        fi
        
        ((TOTAL++))
        test_name=$(basename "$test_file")
        
        # Run test and capture output
        output=$("$AE_BIN" "$test_file" 2>&1) || true
        exit_code=$?
        
        # Check for failures
        if echo "$output" | grep -qE "✗|FAILED|Error:|error:"; then
            has_failure=true
        else
            has_failure=false
        fi
        
        if [ $exit_code -eq 0 ] && [ "$has_failure" = false ]; then
            ((PASSED++))
            echo -e "  ${GREEN}✓${NC} $test_name"
            if [ -n "$VERBOSE" ]; then
                echo "$output"
                echo ""
            fi
        else
            ((FAILED++))
            FAILED_TESTS="$FAILED_TESTS\n  - $test_name"
            echo -e "  ${RED}✗${NC} $test_name"
            if [ -n "$VERBOSE" ]; then
                echo "$output"
                echo ""
            fi
        fi
    done
}

# Run feature tests
echo -e "${CYAN}Running feature tests...${NC}"
run_tests "tests" "feature_*.ae"

# Run coverage tests
echo ""
echo -e "${CYAN}Running coverage tests...${NC}"
run_tests "tests/coverage" "*.ae"

# Run builtin tests
echo ""
echo -e "${CYAN}Running builtin tests...${NC}"
run_tests "tests/scripts/builtins" "*.ae"

# Run integration tests
echo ""
echo -e "${CYAN}Running integration tests...${NC}"
run_tests "tests/scripts/integration" "*.ae"

# Summary
echo ""
echo -e "${CYAN}=== Test Results ===${NC}"
echo -e "${GREEN}Passed: $PASSED${NC}"
if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Failed: $FAILED${NC}"
    echo ""
    echo -e "${CYAN}Failed tests:${NC}"
    echo -e "$FAILED_TESTS"
else
    echo "Failed: 0"
fi

# Calculate percentage
if [ $TOTAL -gt 0 ]; then
    PERCENTAGE=$(echo "scale=1; ($PASSED / $TOTAL) * 100" | bc)
    echo ""
    echo -e "${CYAN}Pass rate: ${PERCENTAGE}% ($PASSED/$TOTAL)${NC}"
fi

# Exit with appropriate code
if [ $FAILED -gt 0 ]; then
    exit 1
else
    exit 0
fi
