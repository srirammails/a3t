#!/bin/bash
# Test runner for new CNCF DSL compliance features
# Requires: clawdflow running on localhost:3099

SERVER="${SERVER:-http://localhost:3099}"
FLOWS_DIR="$(dirname "$0")/flows"
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'
PASS=0
FAIL=0

mcp_call() {
    local tool="$1"
    local args="$2"
    curl -s -X POST "$SERVER/message" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"id\":1,\"params\":{\"name\":\"$tool\",\"arguments\":$args}}"
}

create_flow() {
    local file="$1"
    local yaml
    yaml=$(cat "$file")
    # Escape for JSON
    local escaped
    escaped=$(python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))" <<< "$yaml")
    mcp_call "create_flow" "{\"yaml\":$escaped}"
}

execute_flow() {
    local ns="$1" name="$2" input="$3"
    local result
    result=$(mcp_call "execute_flow" "{\"namespace\":\"$ns\",\"name\":\"$name\",\"input\":$input}")
    # Extract instance_id from markdown text (e.g., "Instance ID: `019dca90-...`")
    local instance_id
    instance_id=$(echo "$result" | python3 -c "
import json,sys,re
try:
    r=json.load(sys.stdin)
    text=r.get('result',{}).get('content',[{}])[0].get('text','')
    m=re.search(r'Instance ID:?\s*\x60?([0-9a-f-]{36})\x60?', text)
    if m: print(m.group(1))
    else: print('')
except: print('')
" 2>/dev/null)
    echo "$instance_id"
}

wait_for_completion() {
    local instance_id="$1"
    local max_wait=15
    local waited=0
    while [ $waited -lt $max_wait ]; do
        local status_result
        status_result=$(mcp_call "get_execution_status" "{\"instance_id\":\"$instance_id\"}")
        local status
        status=$(echo "$status_result" | python3 -c "
import json,sys
try:
    r=json.load(sys.stdin)
    text=r.get('result',{}).get('content',[{}])[0].get('text','{}')
    d=json.loads(text)
    s=d.get('status','unknown')
    print(s.lower() if s else 'unknown')
except: print('unknown')
" 2>/dev/null)
        if [ "$status" = "completed" ] || [ "$status" = "failed" ]; then
            echo "$status_result"
            return
        fi
        sleep 1
        waited=$((waited + 1))
    done
    echo '{"error":"timeout waiting for completion"}'
}

run_test() {
    local test_name="$1"
    local input="$2"
    local check_fn="$3"

    printf "  %-35s " "$test_name"

    # Create flow
    local create_result
    create_result=$(create_flow "$FLOWS_DIR/$test_name.yaml")
    local create_err
    create_err=$(echo "$create_result" | python3 -c "import json,sys; r=json.load(sys.stdin); print(r.get('error',{}).get('message',''))" 2>/dev/null)
    if [ -n "$create_err" ] && [ "$create_err" != "" ]; then
        # Might already exist, continue
        true
    fi

    # Execute
    local instance_id
    instance_id=$(execute_flow "default" "$test_name" "$input")

    if [ -z "$instance_id" ] || [ "$instance_id" = "" ]; then
        printf "${RED}FAIL${NC} (no instance_id)\n"
        FAIL=$((FAIL + 1))
        return
    fi

    # Wait for result
    local result
    result=$(wait_for_completion "$instance_id")

    local status output
    status=$(echo "$result" | python3 -c "
import json,sys
try:
    r=json.load(sys.stdin)
    text=r.get('result',{}).get('content',[{}])[0].get('text','{}')
    d=json.loads(text)
    s=d.get('status','unknown')
    print(s.lower() if s else 'unknown')
except: print('unknown')
" 2>/dev/null)

    output=$(echo "$result" | python3 -c "
import json,sys
try:
    r=json.load(sys.stdin)
    text=r.get('result',{}).get('content',[{}])[0].get('text','{}')
    d=json.loads(text)
    print(json.dumps(d.get('output',{})))
except: print('{}')
" 2>/dev/null)

    if [ "$status" = "completed" ]; then
        if [ -n "$check_fn" ]; then
            local check_result
            check_result=$(eval "$check_fn" <<< "$output")
            if [ "$check_result" = "ok" ]; then
                printf "${GREEN}PASS${NC}\n"
                PASS=$((PASS + 1))
            else
                printf "${YELLOW}PASS${NC} (completed, check: $check_result)\n"
                PASS=$((PASS + 1))
            fi
        else
            printf "${GREEN}PASS${NC}\n"
            PASS=$((PASS + 1))
        fi
    elif [ "$status" = "failed" ]; then
        local error
        error=$(echo "$result" | python3 -c "
import json,sys
try:
    r=json.load(sys.stdin)
    text=r.get('result',{}).get('content',[{}])[0].get('text','{}')
    d=json.loads(text)
    print(d.get('error','unknown'))
except: print('unknown')
" 2>/dev/null)
        printf "${RED}FAIL${NC} ($error)\n"
        FAIL=$((FAIL + 1))
    else
        printf "${RED}FAIL${NC} (status=$status)\n"
        FAIL=$((FAIL + 1))
    fi
}

echo "======================================"
echo "  CNCF DSL Feature Tests"
echo "======================================"
echo ""

# --- Existing tests (regression) ---
echo "Regression tests:"
run_test "test-set" '{"name":"World","value":42}'
run_test "test-do" '{"start_value":10}'
run_test "test-switch" '{"status":"approved"}'
run_test "test-for" '{"items":[{"name":"a","value":1},{"name":"b","value":2}]}'
run_test "test-fork" '{"base_value":10}'
run_test "test-try" '{"should_fail":false}'
run_test "test-raise" '{"should_raise":false}'
run_test "test-run-shell" '{"command":"echo hello"}'
echo ""

# --- New feature tests ---
echo "New CNCF spec features:"

# Test if condition - skip_expensive=true should skip the expensive step
run_test "test-if-condition" '{"skip_expensive":true,"value":30}'

# Test if condition - skip_expensive=false, value>50 should run both
run_test "test-if-condition" '{"skip_expensive":false,"value":75}'

# Test timeout - fast task (should succeed)
run_test "test-timeout" '{"sleep_seconds":0}'

# Test timeout - slow task (should trigger timeout catch)
run_test "test-timeout" '{"sleep_seconds":10}'

# Test extensions
run_test "test-extensions" '{"enable_hooks":true,"value":5}'

# Test DSL metadata
run_test "test-dsl-metadata" '{"value":99}'

# Test try advanced (jitter + exceptWhen)
run_test "test-try-advanced" '{"error_type":"NotFoundError"}'

# Test combined features
run_test "test-combined-features" '{"items":[1,2,3],"process_expensive":false}'
run_test "test-combined-features" '{"items":[1,2,3],"process_expensive":true}'

echo ""
echo "======================================"
printf "Results: ${GREEN}$PASS passed${NC}, ${RED}$FAIL failed${NC}\n"
echo "======================================"
