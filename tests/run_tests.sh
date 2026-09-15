#!/bin/bash
# ClawdFlow Test Runner
# Runs all test workflows against the MCP server

SERVER="http://37.27.195.4:3002"
TESTS_DIR="$(dirname "$0")/flows"
RESULTS=()

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "========================================"
echo "ClawdFlow Test Suite"
echo "Server: $SERVER"
echo "========================================"

# Function to create a workflow
create_flow() {
    local yaml_file="$1"
    local json_content=$(python3 -c "
import yaml
import json
import sys
with open('$yaml_file', 'r') as f:
    data = yaml.safe_load(f)
print(json.dumps(data))
")

    local result=$(curl -s --connect-timeout 10 --max-time 30 -X POST "$SERVER/message" \
        -H "Content-Type: application/json" \
        -d "{
            \"jsonrpc\": \"2.0\",
            \"method\": \"tools/call\",
            \"id\": 1,
            \"params\": {
                \"name\": \"create_flow\",
                \"arguments\": {
                    \"namespace\": \"test\",
                    \"name\": \"$(basename $yaml_file .yaml)\",
                    \"json\": $json_content
                }
            }
        }")
    echo "$result"
}

# Function to execute a workflow
execute_flow() {
    local name="$1"
    local input="$2"

    local result=$(curl -s --connect-timeout 10 --max-time 30 -X POST "$SERVER/message" \
        -H "Content-Type: application/json" \
        -d "{
            \"jsonrpc\": \"2.0\",
            \"method\": \"tools/call\",
            \"id\": 2,
            \"params\": {
                \"name\": \"execute_flow\",
                \"arguments\": {
                    \"namespace\": \"test\",
                    \"name\": \"$name\",
                    \"input\": $input
                }
            }
        }")

    # Extract instance_id
    local instance_id=$(echo "$result" | grep -o 'Instance ID: `[^`]*`' | sed 's/Instance ID: `\([^`]*\)`/\1/')
    echo "$instance_id"
}

# Function to get execution status
get_status() {
    local instance_id="$1"

    # Wait for completion
    sleep 2

    local result=$(curl -s --connect-timeout 10 --max-time 30 -X POST "$SERVER/message" \
        -H "Content-Type: application/json" \
        -d "{
            \"jsonrpc\": \"2.0\",
            \"method\": \"tools/call\",
            \"id\": 3,
            \"params\": {
                \"name\": \"get_execution_status\",
                \"arguments\": {
                    \"instance_id\": \"$instance_id\"
                }
            }
        }")
    echo "$result"
}

# Run a single test
run_test() {
    local test_name="$1"
    local input="$2"
    local yaml_file="$TESTS_DIR/$test_name.yaml"

    if [ ! -f "$yaml_file" ]; then
        echo -e "${YELLOW}SKIP${NC} $test_name (file not found)"
        return 2
    fi

    echo -n "Testing $test_name... "

    # Create the flow
    create_result=$(create_flow "$yaml_file" 2>/dev/null)

    # Execute the flow
    instance_id=$(execute_flow "$test_name" "$input" 2>/dev/null)

    if [ -z "$instance_id" ]; then
        echo -e "${RED}FAIL${NC} (could not start execution)"
        return 1
    fi

    # Get status
    status_result=$(get_status "$instance_id" 2>/dev/null)

    # Check status
    if echo "$status_result" | grep -q '"status": "Completed"'; then
        echo -e "${GREEN}PASS${NC}"
        return 0
    elif echo "$status_result" | grep -q '"status": "Failed"'; then
        error=$(echo "$status_result" | grep -o '"error": "[^"]*"' | head -1)
        echo -e "${RED}FAIL${NC} ($error)"
        return 1
    else
        echo -e "${YELLOW}UNKNOWN${NC}"
        return 2
    fi
}

# Test definitions with inputs
declare -A TEST_INPUTS
TEST_INPUTS["test-set"]='{"name": "World", "value": 42}'
TEST_INPUTS["test-do"]='{"start_value": 10}'
TEST_INPUTS["test-for"]='{"items": [{"name": "a", "value": 1}, {"name": "b", "value": 2}]}'
TEST_INPUTS["test-switch"]='{"status": "approved"}'
TEST_INPUTS["test-fork"]='{"value": 10}'
TEST_INPUTS["test-try"]='{"should_fail": false}'
TEST_INPUTS["test-raise"]='{"should_raise": false}'
TEST_INPUTS["test-wait"]='{"seconds": 1}'
TEST_INPUTS["test-call-http"]='{"url": "https://httpbin.org/get"}'
TEST_INPUTS["test-run-shell"]='{"command": "echo hello"}'
TEST_INPUTS["test-inline-lua"]='{"x": 5, "y": 3}'
TEST_INPUTS["test-inline-js"]='{"numbers": [1, 2, 3, 4, 5]}'
TEST_INPUTS["test-call-lancedb"]='{}'

# Run tests
PASSED=0
FAILED=0
SKIPPED=0

for test_file in "$TESTS_DIR"/*.yaml; do
    test_name=$(basename "$test_file" .yaml)
    input="${TEST_INPUTS[$test_name]:-{}}"

    run_test "$test_name" "$input"
    result=$?

    if [ $result -eq 0 ]; then
        ((PASSED++))
    elif [ $result -eq 1 ]; then
        ((FAILED++))
    else
        ((SKIPPED++))
    fi
done

echo ""
echo "========================================"
echo "Results: $PASSED passed, $FAILED failed, $SKIPPED skipped"
echo "========================================"
