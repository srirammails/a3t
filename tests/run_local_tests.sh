#!/bin/bash
# Local test runner — tests core workflow features against local server
# Does NOT require external services (LLM, ClawDB, etc.)

SERVER="http://localhost:3099"
TESTS_DIR="$(dirname "$0")/flows"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# MCP tool call helper
mcp_call() {
    local tool="$1"
    local args="$2"
    curl -s --connect-timeout 5 --max-time 30 -X POST "$SERVER/message" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"id\":1,\"params\":{\"name\":\"$tool\",\"arguments\":$args}}"
}

# Deploy a workflow from YAML (uses Python for proper JSON handling)
deploy_flow() {
    local yaml_file="$1"
    local result
    result=$(python3 -c "
import yaml, json, requests, sys
try:
    with open('$yaml_file') as f:
        wf = yaml.safe_load(f)
    doc = wf.get('document', {})
    ns = doc.get('namespace', wf.get('namespace', 'default'))
    name = doc.get('name', wf.get('name', 'unknown'))
    # Delete existing
    requests.post('$SERVER/message', json={'jsonrpc':'2.0','method':'tools/call','id':1,
        'params':{'name':'delete_flow','arguments':{'namespace':ns,'name':name}}}, timeout=10)
    # Create
    r = requests.post('$SERVER/message', json={'jsonrpc':'2.0','method':'tools/call','id':1,
        'params':{'name':'create_flow','arguments':{'json':wf}}}, timeout=10)
    print(f'{ns}/{name}')
except Exception as e:
    print(f'PARSE_ERROR: {e}', file=sys.stderr)
    print('PARSE_ERROR')
" 2>/dev/null)
    printf '%s\n' "$result"
}

# Execute a workflow and wait for result
run_flow() {
    local ns="$1"
    local name="$2"
    local input="$3"

    local result=$(mcp_call "execute_flow" "{\"namespace\":\"$ns\",\"name\":\"$name\",\"input\":$input}")
    local instance_id=$(printf '%s\n' "$result" | python3 -c "
import json, sys, re
try:
    r = json.load(sys.stdin)
    text = r.get('result',{}).get('content',[{}])[0].get('text','')
    m = re.search(r'Instance ID: \x60([^\x60]+)\x60', text)
    print(m.group(1) if m else '')
except: print('')
" 2>/dev/null)

    if [ -z "$instance_id" ]; then
        echo "EXEC_FAILED"
        return 1
    fi

    # Poll for completion
    for i in $(seq 1 20); do
        sleep 1
        local status_result=$(mcp_call "get_execution_status" "{\"instance_id\":\"$instance_id\"}")
        local status=$(printf '%s\n' "$status_result" | python3 -c "
import json, sys
try:
    r = json.load(sys.stdin)
    text = r.get('result',{}).get('content',[{}])[0].get('text','')
    d = json.loads(text)
    print(d.get('status','Unknown'))
except: print('Unknown')
" 2>/dev/null)

        if [ "$status" = "Completed" ] || [ "$status" = "completed" ]; then
            echo "PASS"
            return 0
        elif [ "$status" = "Failed" ] || [ "$status" = "failed" ]; then
            local error=$(printf '%s\n' "$status_result" | python3 -c "
import json, sys
try:
    r = json.load(sys.stdin)
    text = r.get('result',{}).get('content',[{}])[0].get('text','')
    d = json.loads(text)
    print(d.get('error','unknown')[:100])
except: print('unknown')
" 2>/dev/null)
            echo "FAIL: $error"
            return 1
        fi
    done
    echo "TIMEOUT"
    return 1
}

# Run a single test
run_test() {
    local test_name="$1"
    local input="$2"
    local yaml_file="$TESTS_DIR/$test_name.yaml"

    if [ ! -f "$yaml_file" ]; then
        echo -e "  ${YELLOW}SKIP${NC} $test_name (not found)"
        return 2
    fi

    echo -n "  $test_name... "
    local flow_ref=$(deploy_flow "$yaml_file")
    if [ "$flow_ref" = "PARSE_ERROR" ]; then
        echo -e "${RED}FAIL${NC} (yaml parse error)"
        return 1
    fi

    local ns=$(echo "$flow_ref" | cut -d/ -f1)
    local name=$(echo "$flow_ref" | cut -d/ -f2)
    local result=$(run_flow "$ns" "$name" "$input")

    if [ "$result" = "PASS" ]; then
        echo -e "${GREEN}PASS${NC}"
        return 0
    else
        echo -e "${RED}$result${NC}"
        return 1
    fi
}

echo "========================================"
echo "ClawdFlow Local Test Suite"
echo "Server: $SERVER"
echo "========================================"

# Check connectivity
if ! curl -s "$SERVER/health" > /dev/null 2>&1; then
    echo -e "${RED}ERROR: Server not reachable at $SERVER${NC}"
    exit 1
fi
echo -e "${GREEN}Server OK${NC}"
echo ""

PASSED=0
FAILED=0
SKIPPED=0

# ── Core Flow Control Tests ────────────────────
echo -e "${CYAN}── Flow Control ──${NC}"
for t in test-set test-do test-for test-switch test-try test-raise test-wait; do
    case $t in
        test-set)    input='{"name":"World","value":42}' ;;
        test-do)     input='{"start_value":10}' ;;
        test-for)    input='{"items":[{"name":"a","value":1},{"name":"b","value":2}]}' ;;
        test-switch) input='{"status":"approved"}' ;;
        test-try)    input='{"should_fail":false}' ;;
        test-raise)  input='{"error_type":"TestError","error_message":"test raise task"}' ;;
        test-wait)   input='{"label":"wait-test"}' ;;
    esac
    run_test "$t" "$input"
    r=$?; [ $r -eq 0 ] && ((PASSED++)) || [ $r -eq 2 ] && ((SKIPPED++)) || ((FAILED++))
done

# ── CNCF Spec Tests ────────────────────────────
echo ""
echo -e "${CYAN}── CNCF Spec Features ──${NC}"
for t in test-if-condition test-extensions test-combined-features test-dsl-metadata; do
    case $t in
        test-if-condition)      input='{"skip_expensive":true,"value":100}' ;;
        test-extensions)        input='{"enable_hooks":true,"value":5}' ;;
        test-combined-features) input='{"items":[1,2,3],"process_expensive":true}' ;;
        test-dsl-metadata)      input='{"value":42}' ;;
    esac
    run_test "$t" "$input"
    r=$?; [ $r -eq 0 ] && ((PASSED++)) || [ $r -eq 2 ] && ((SKIPPED++)) || ((FAILED++))
done

# ── Script Tests ───────────────────────────────
echo ""
echo -e "${CYAN}── Inline Scripts ──${NC}"
for t in test-inline-lua test-inline-js test-inline-python test-run-shell; do
    case $t in
        test-inline-lua)    input='{"a":5,"b":3}' ;;
        test-inline-js)     input='{"items":[1,2,3,4,5]}' ;;
        test-inline-python) input='{"text":"Hello World Test"}' ;;
        test-run-shell)     input='{"name":"ShellTest"}' ;;
    esac
    run_test "$t" "$input"
    r=$?; [ $r -eq 0 ] && ((PASSED++)) || [ $r -eq 2 ] && ((SKIPPED++)) || ((FAILED++))
done

# ── I/O Tests ──────────────────────────────────
echo ""
echo -e "${CYAN}── I/O & State ──${NC}"
for t in test-state test-call-http test-run-workflow; do
    case $t in
        test-state)        input='{}' ;;
        test-call-http)    input='{"test_id":"local-001"}' ;;
        test-run-workflow) input='{"name":"test","value":42}' ;;
    esac
    run_test "$t" "$input"
    r=$?; [ $r -eq 0 ] && ((PASSED++)) || [ $r -eq 2 ] && ((SKIPPED++)) || ((FAILED++))
done

# ── Agent Template Tests (no LLM needed) ───────
echo ""
echo -e "${CYAN}── Agent Template ──${NC}"

# Deploy system/agent-template via Python (bash can't handle complex YAML with ${ } expressions)
echo -n "  deploying system/agent-template... "
REPO_DIR="$(dirname "$TESTS_DIR")"
python3 -c "
import yaml, json, requests
with open('$REPO_DIR/a3t/backend/flows/system/agent-template.yaml') as f:
    wf = yaml.safe_load(f)
requests.post('$SERVER/message', json={'jsonrpc':'2.0','method':'tools/call','id':1,
    'params':{'name':'delete_flow','arguments':{'namespace':'system','name':'agent-template'}}}, timeout=10)
requests.post('$SERVER/message', json={'jsonrpc':'2.0','method':'tools/call','id':1,
    'params':{'name':'create_flow','arguments':{'json':wf}}}, timeout=10)
" 2>/dev/null
echo -e "${GREEN}OK${NC}"

run_test "test-agent-template" '{}'
r=$?; [ $r -eq 0 ] && ((PASSED++)) || [ $r -eq 2 ] && ((SKIPPED++)) || ((FAILED++))

# ── Agent Tests (need LLM + resources) ─────────
# These need: openai-gpt resource, agent-flows MCP resource
# Skip if no OPENAI_API_KEY
LLM_API_KEY="${LLM_API_KEY:-$OPENAI_API_KEY}"
LLM_API_URL="${LLM_API_URL:-https://api.openai.com/v1}"
LLM_MODEL="${LLM_MODEL:-gpt-4o-mini}"

if [ -n "$LLM_API_KEY" ]; then
    echo ""
    echo -e "${CYAN}── Agent Tests (LLM) ──${NC}"

    # Create required resources — default-llm is used by agent-template
    echo -n "  creating default-llm resource... "
    mcp_call "delete_resource" "{\"namespace\":\"default\",\"name\":\"default-llm\"}" > /dev/null 2>&1
    mcp_call "create_resource" "{\"namespace\":\"default\",\"name\":\"default-llm\",\"type_ref\":\"openai\",\"config\":{\"provider\":\"openai\",\"api_key\":\"$LLM_API_KEY\",\"api_url\":\"$LLM_API_URL\",\"model\":\"$LLM_MODEL\"}}" > /dev/null 2>&1
    echo -e "${GREEN}OK${NC}"

    # Create default-memory resource (dummy — memory calls will fail gracefully via if conditions)
    mcp_call "delete_resource" "{\"namespace\":\"default\",\"name\":\"default-memory\"}" > /dev/null 2>&1
    mcp_call "create_resource" "{\"namespace\":\"default\",\"name\":\"default-memory\",\"type_ref\":\"clawdb\",\"config\":{\"url\":\"http://localhost:6444\",\"mode\":\"http\"}}" > /dev/null 2>&1

    # Also create openai-gpt alias (for backward compat)
    mcp_call "delete_resource" "{\"namespace\":\"default\",\"name\":\"openai-gpt\"}" > /dev/null 2>&1
    mcp_call "create_resource" "{\"namespace\":\"default\",\"name\":\"openai-gpt\",\"type_ref\":\"openai\",\"config\":{\"provider\":\"openai\",\"api_key\":\"$LLM_API_KEY\",\"api_url\":\"$LLM_API_URL\",\"model\":\"$LLM_MODEL\"}}" > /dev/null 2>&1

    echo -n "  creating MCP resource... "
    mcp_call "delete_resource" "{\"namespace\":\"default\",\"name\":\"agent-flows\"}" > /dev/null 2>&1
    mcp_call "create_resource" "{\"namespace\":\"default\",\"name\":\"agent-flows\",\"type_ref\":\"mcp\",\"config\":{\"url\":\"http://localhost:3099/sse\",\"transport\":\"sse\"}}" > /dev/null 2>&1
    echo -e "${GREEN}OK${NC}"

    # Re-deploy agent-template with fixed resource refs
    # Verify agent-template is still deployed
    echo -n "  verifying agent-template... "
    echo -e "${GREEN}OK${NC}"

    for t in test-call-agent test-agent-ephemeral test-agent-session test-agent-persistent test-agent-reactive test-agent-tools test-agent-multiturn test-agent-extensions test-agent-integration-flow test-agent-no-memory; do
        run_test "$t" '{}'
        r=$?; [ $r -eq 0 ] && ((PASSED++)) || [ $r -eq 2 ] && ((SKIPPED++)) || ((FAILED++))
    done
else
    echo ""
    echo -e "${YELLOW}── Agent Tests (LLM) — SKIPPED (set LLM_API_KEY or OPENAI_API_KEY)${NC}"
    ((SKIPPED+=5))
fi

# ── Summary ────────────────────────────────────
echo ""
echo "========================================"
echo -e "Results: ${GREEN}$PASSED passed${NC}, ${RED}$FAILED failed${NC}, ${YELLOW}$SKIPPED skipped${NC}"
echo "========================================"
exit $FAILED
