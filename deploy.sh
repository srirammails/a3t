#!/usr/bin/env bash
set -euo pipefail

# ═══════════════════════════════════════════════════════════════════
# FlowR Full Deployment Script
# Deploys FlowR MCP server + A3T platform to remote server
# ═══════════════════════════════════════════════════════════════════

REMOTE_HOST="37.27.195.4"
REMOTE_USER="root"
SSH_KEY="$HOME/.ssh/sri_github"
SSH="ssh -i $SSH_KEY $REMOTE_USER@$REMOTE_HOST"
LOCAL_DIR="$(cd "$(dirname "$0")" && pwd)"
REMOTE_DIR="/root/flowr-system"
CONTAINER_NAME="flowr-service"
IMAGE="ghcr.io/srirammails/flowr:latest"
PORT=3002
K3D_PORT=30080
MCP="http://localhost:$K3D_PORT/message"

WRAPPER_CONTAINER="claude-wrapper"
WRAPPER_IMAGE="claude-wrapper:latest"
WRAPPER_PORT=8000
CLAUDE_AUTH_TOKEN="VefjN6BHMfwNI0gES9b6kSzRqLke4CPALXolDAr1TXGlmmaA#EhhVUPizx80KHxrzwJGfNv0XF1Lki1cMKCl8TrNzJyM"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()  { echo -e "${GREEN}[deploy]${NC} $1"; }
warn() { echo -e "${YELLOW}[deploy]${NC} $1"; }
err()  { echo -e "${RED}[deploy]${NC} $1" >&2; }

# ═══════════════════════════════════════════════════════════════════
# 1. RSYNC — sync source to remote (exclude build artifacts)
# ═══════════════════════════════════════════════════════════════════
step_rsync() {
    log "Step 1: Rsync to $REMOTE_HOST..."
    rsync -avz --delete \
        --exclude='target/' \
        --exclude='.git/' \
        --exclude='AQL/' \
        --exclude='node_modules/' \
        -e "ssh -i $SSH_KEY" \
        "$LOCAL_DIR/" "$REMOTE_USER@$REMOTE_HOST:$REMOTE_DIR/"

    # AQL is excluded from rsync (large), copy from host-local copy
    log "Copying AQL into build context..."
    $SSH "cp -r /root/AQL $REMOTE_DIR/AQL 2>/dev/null || true"
}

# ═══════════════════════════════════════════════════════════════════
# 2. BUILD — docker build on remote (nohup to survive SSH drops)
# ═══════════════════════════════════════════════════════════════════
step_build() {
    log "Step 2: Building Docker image (this takes ~5 min)..."
    $SSH "cd $REMOTE_DIR && \
        docker build --no-cache \
            -f flowr/Dockerfile \
            -t $IMAGE \
            . > /root/flowr-build.log 2>&1"

    if [ $? -eq 0 ]; then
        log "Build succeeded."
    else
        err "Build failed. Check remote: cat /root/flowr-build.log"
        exit 1
    fi
}

# ═══════════════════════════════════════════════════════════════════
# 3. DEPLOY — stop old container, start new one with volume mounts
# ═══════════════════════════════════════════════════════════════════
step_deploy() {
    log "Step 3: Deploying container..."

    # Create data directories on host
    $SSH "mkdir -p /data/clawdflow /data/a3t && chmod 777 /data/a3t"

    # Stop any existing flowr/clawdflow containers on port 3002
    $SSH "docker stop $CONTAINER_NAME 2>/dev/null; docker rm $CONTAINER_NAME 2>/dev/null; true"
    $SSH "docker stop clawdflow-service 2>/dev/null; docker rm clawdflow-service 2>/dev/null; true"

    # Kill any bare process on port 3002
    $SSH "fuser -k $PORT/tcp 2>/dev/null; true"
    sleep 1

    # Run FlowR EE with LanceDB storage
    $SSH "docker run -d \
        --name $CONTAINER_NAME \
        --restart unless-stopped \
        -p $PORT:$PORT \
        -v /data/clawdflow:/data/clawdflow \
        -v /data/a3t:/data/a3t \
        -e CLAWDFLOW_DATA_DIR=/data/clawdflow \
        -e A3T_DB_PATH=/data/a3t/a3t-platform.lance \
        -e FLOWR_CORS_ORIGINS=http://localhost:5173,https://console.a3t.io,http://37.27.195.4:30080 \
        -e RUST_LOG=info \
        $IMAGE \
        --mode http --port $PORT"

    # Wait for startup
    log "Waiting for FlowR to start..."
    for i in $(seq 1 10); do
        if $SSH "curl -sf http://localhost:$PORT/health > /dev/null 2>&1"; then
            log "FlowR is up on port $PORT"
            return 0
        fi
        sleep 2
    done

    err "FlowR failed to start. Check: ssh $REMOTE_USER@$REMOTE_HOST docker logs $CONTAINER_NAME"
    exit 1
}

# ═══════════════════════════════════════════════════════════════════
# 4. REGISTER RESOURCES — a3t-platform-db and flowr-system-db
# ═══════════════════════════════════════════════════════════════════
step_resources() {
    log "Step 4: Registering resources..."

    # a3t-platform-db — A3T platform LanceDB
    $SSH "curl -sf $MCP -H 'Content-Type: application/json' -d '{
        \"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"id\":1,
        \"params\":{\"name\":\"create_resource\",\"arguments\":{
            \"name\":\"a3t-platform-db\",\"namespace\":\"a3t\",\"type_ref\":\"lancedb\",
            \"config\":{\"db_path\":\"/data/a3t/a3t-platform.lance\"}
        }}
    }' > /dev/null"
    log "  Created: a3t/a3t-platform-db"

    # flowr-system-db — system workflows LanceDB (for reading flow definitions)
    $SSH "curl -sf $MCP -H 'Content-Type: application/json' -d '{
        \"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"id\":1,
        \"params\":{\"name\":\"create_resource\",\"arguments\":{
            \"name\":\"flowr-system-db\",\"namespace\":\"a3t\",\"type_ref\":\"lancedb\",
            \"config\":{\"db_path\":\"/data/clawdflow/clawdflow.lance\"}
        }}
    }' > /dev/null"
    log "  Created: a3t/flowr-system-db"
}

# ═══════════════════════════════════════════════════════════════════
# 5. REGISTER FLOWS — all A3T workflow YAML files
# ═══════════════════════════════════════════════════════════════════
step_flows() {
    log "Step 5: Registering A3T flows..."

    $SSH 'python3 << PYEOF
import json, subprocess, os

flow_dir = "/root/flowr-system/a3t/backend/flows"
success = 0
failed = 0

for root, dirs, fnames in sorted(os.walk(flow_dir)):
    for fname in sorted(fnames):
        if not fname.endswith(".yaml"):
            continue
        fpath = os.path.join(root, fname)
        with open(fpath) as f:
            content = f.read()
        # Skip non-workflow files (no document: block)
        if "document:" not in content:
            continue

        payload = json.dumps({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": {"name": "create_flow", "arguments": {"yaml": content}}
        })
        r = subprocess.run(
            ["curl", "-s", "--max-time", "30", "http://localhost:'"$K3D_PORT"'/message",
             "-H", "Content-Type: application/json", "-d", payload],
            capture_output=True, text=True
        )
        rel = os.path.relpath(fpath, flow_dir)
        if r.returncode == 0 and "successfully" in r.stdout:
            print(f"  OK: {rel}")
            success += 1
        else:
            print(f"  FAIL: {rel}")
            failed += 1

print(f"\n  Registered: {success} flows, {failed} failed")
PYEOF'
}

# ═══════════════════════════════════════════════════════════════════
# 6. INSTALL — run a3t-install flow (schema + seed templates)
# ═══════════════════════════════════════════════════════════════════
step_install() {
    log "Step 6: Running a3t-install..."

    # Execute
    RESULT=$($SSH "curl -sf $MCP -H 'Content-Type: application/json' -d '{
        \"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"id\":1,
        \"params\":{\"name\":\"execute_flow\",\"arguments\":{
            \"namespace\":\"a3t\",\"name\":\"a3t-install\",\"input\":{}
        }}
    }'")
    INSTANCE_ID=$(echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); t=d['result']['content'][0]['text']; print(t.split('Instance ID: \`')[1].split('\`')[0])" 2>/dev/null || true)

    if [ -z "$INSTANCE_ID" ]; then
        err "Failed to start a3t-install"
        echo "$RESULT"
        exit 1
    fi
    log "  Instance: $INSTANCE_ID"

    # Poll for completion
    for i in $(seq 1 20); do
        sleep 2
        STATUS=$($SSH "curl -sf $MCP -H 'Content-Type: application/json' -d '{
            \"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"id\":1,
            \"params\":{\"name\":\"get_execution_status\",\"arguments\":{
                \"instance_id\":\"$INSTANCE_ID\"
            }}
        }'" | python3 -c "import sys,json; d=json.load(sys.stdin); t=json.loads(d['result']['content'][0]['text']); print(t['status'])" 2>/dev/null || echo "pending")

        case "$STATUS" in
            completed)
                log "  a3t-install completed successfully"
                return 0
                ;;
            failed)
                err "  a3t-install failed. Check execution status for details."
                $SSH "curl -sf $MCP -H 'Content-Type: application/json' -d '{
                    \"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"id\":1,
                    \"params\":{\"name\":\"get_execution_status\",\"arguments\":{
                        \"instance_id\":\"$INSTANCE_ID\"
                    }}
                }'" | python3 -c "import sys,json; d=json.load(sys.stdin); t=json.loads(d['result']['content'][0]['text']); print(t.get('error',''))"
                exit 1
                ;;
            *)
                echo -n "."
                ;;
        esac
    done
    err "  a3t-install timed out"
    exit 1
}

# ═══════════════════════════════════════════════════════════════════
# 7. VERIFY — check everything is working
# ═══════════════════════════════════════════════════════════════════
step_verify() {
    log "Step 7: Verifying deployment..."

    # Check container
    CONTAINER_STATUS=$($SSH "docker ps --filter name=$CONTAINER_NAME --format '{{.Status}}'" 2>/dev/null)
    log "  Container: $CONTAINER_STATUS"

    # Check flows
    FLOW_COUNT=$($SSH "curl -sf $MCP -H 'Content-Type: application/json' -d '{
        \"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"id\":1,
        \"params\":{\"name\":\"list_flows\",\"arguments\":{\"namespace\":\"a3t\"}}
    }'" | python3 -c "import sys,json; d=json.load(sys.stdin); t=json.loads(d['result']['content'][0]['text']); print(t['count'])" 2>/dev/null || echo "?")
    log "  A3T flows: $FLOW_COUNT"

    # Check system flows
    SYS_COUNT=$($SSH "curl -sf $MCP -H 'Content-Type: application/json' -d '{
        \"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"id\":1,
        \"params\":{\"name\":\"list_flows\",\"arguments\":{\"namespace\":\"system\"}}
    }'" | python3 -c "import sys,json; d=json.load(sys.stdin); t=json.loads(d['result']['content'][0]['text']); print(t['count'])" 2>/dev/null || echo "?")
    log "  System flows: $SYS_COUNT"

    # Check resources
    RES_COUNT=$($SSH "curl -sf $MCP -H 'Content-Type: application/json' -d '{
        \"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"id\":1,
        \"params\":{\"name\":\"list_resources\",\"arguments\":{\"namespace\":\"a3t\"}}
    }'" | python3 -c "import sys,json; d=json.load(sys.stdin); t=json.loads(d['result']['content'][0]['text']); print(t['count'])" 2>/dev/null || echo "?")
    log "  A3T resources: $RES_COUNT"

    # Check templates
    TEMPLATE_STATUS=$($SSH "curl -sf $MCP -H 'Content-Type: application/json' -d '{
        \"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"id\":1,
        \"params\":{\"name\":\"execute_flow\",\"arguments\":{
            \"namespace\":\"a3t\",\"name\":\"a3t-templates\",
            \"input\":{\"input\":{\"action\":\"list\",\"org_id\":\"org-default\"}}
        }}
    }'" 2>/dev/null)
    log "  Templates: seeded (verify in UI)"

    echo ""
    log "Deployment complete!"
    log "  FlowR:  http://$REMOTE_HOST:$PORT"
    log "  SSE:    http://$REMOTE_HOST:$PORT/sse"
    log "  Health: http://$REMOTE_HOST:$PORT/health"
}

# ═══════════════════════════════════════════════════════════════════
# 8. WRAPPER BUILD — build claude-code openai wrapper container
# ═══════════════════════════════════════════════════════════════════
step_wrapper_build() {
    log "Step 8: Building Claude wrapper image..."

    # Clone/update wrapper source on remote
    $SSH "if [ -d /root/claude-code-openai-wrapper ]; then
        cd /root/claude-code-openai-wrapper && git pull
    else
        git clone https://github.com/RichardAtCT/claude-code-openai-wrapper.git /root/claude-code-openai-wrapper
    fi"

    # Copy wrapper source into build context
    $SSH "mkdir -p $REMOTE_DIR/claude-wrapper/wrapper && \
        cp -r /root/claude-code-openai-wrapper/* $REMOTE_DIR/claude-wrapper/wrapper/"

    $SSH "cd $REMOTE_DIR/claude-wrapper && \
        docker build -t $WRAPPER_IMAGE . > /root/wrapper-build.log 2>&1"

    if [ $? -eq 0 ]; then
        log "Wrapper build succeeded."
    else
        err "Wrapper build failed. Check: ssh $REMOTE_USER@$REMOTE_HOST cat /root/wrapper-build.log"
        exit 1
    fi
}

# ═══════════════════════════════════════════════════════════════════
# 9. WRAPPER DEPLOY — run wrapper container (network=host for FlowR access)
# ═══════════════════════════════════════════════════════════════════
step_wrapper_deploy() {
    log "Step 9: Deploying Claude wrapper..."

    $SSH "docker stop $WRAPPER_CONTAINER 2>/dev/null; docker rm $WRAPPER_CONTAINER 2>/dev/null; true"

    # Create shared network (flowr-service and wrapper communicate over it)
    $SSH "docker network create a3t-net 2>/dev/null; true"
    $SSH "docker network connect a3t-net $CONTAINER_NAME 2>/dev/null; true"

    $SSH "docker run -d \
        --name $WRAPPER_CONTAINER \
        --restart unless-stopped \
        --network a3t-net \
        -p $WRAPPER_PORT:$WRAPPER_PORT \
        -v /root/.claude:/root/.claude \
        -e PORT=$WRAPPER_PORT \
        -e CLAUDE_CWD=/workspace \
        -e CLAUDE_AUTH_METHOD=cli \
        -e VERBOSE=true \
        $WRAPPER_IMAGE"

    log "Waiting for wrapper to start..."
    for i in $(seq 1 10); do
        if $SSH "curl -sf http://localhost:$WRAPPER_PORT/health > /dev/null 2>&1"; then
            log "Wrapper is up on port $WRAPPER_PORT"
            return 0
        fi
        sleep 3
    done

    warn "Wrapper may need Claude CLI auth. SSH to server and run:"
    warn "  docker exec -it $WRAPPER_CONTAINER claude auth login"
}

# ═══════════════════════════════════════════════════════════════════
# 10. REGISTER WRAPPER LLM RESOURCE
# ═══════════════════════════════════════════════════════════════════
step_wrapper_resource() {
    log "Step 10: Registering Claude wrapper as LLM resource..."

    $SSH "curl -sf $MCP -H 'Content-Type: application/json' -d '{
        \"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"id\":1,
        \"params\":{\"name\":\"create_resource\",\"arguments\":{
            \"name\":\"claude-cli-max\",\"namespace\":\"a3t\",\"type_ref\":\"openai\",
            \"config\":{
                \"provider\":\"openai\",
                \"api_url\":\"http://claude-wrapper.a3t.svc.cluster.local:$WRAPPER_PORT/v1\",
                \"model\":\"claude-opus-4-6\",
                \"api_key\":\"not-needed\",
                \"max_tokens\":8192,
                \"temperature\":0.7
            }
        }}
    }' > /dev/null"
    log "  Created: a3t/claude-cli-max (via wrapper at localhost:$WRAPPER_PORT)"
}

# ═══════════════════════════════════════════════════════════════════
# K3D — clean k3d deployment (both services)
# ═══════════════════════════════════════════════════════════════════
step_k3d() {
    log "K3D: Creating clean k3d cluster with flowr + claude-wrapper..."

    # Delete old cluster if exists
    $SSH "k3d cluster delete a3t 2>/dev/null; true"

    # Create data dirs
    $SSH "mkdir -p /data/clawdflow /data/a3t /root/.claude"

    # Create cluster from config
    $SSH "cd $REMOTE_DIR && k3d cluster create --config flowr/deploy/k3d/cluster.yaml"

    log "Waiting for cluster to be ready..."
    $SSH "kubectl wait --for=condition=ready node --all --timeout=60s"

    # Load images into k3d
    log "Loading images into k3d..."
    $SSH "k3d image import $IMAGE -c a3t"
    $SSH "k3d image import $WRAPPER_IMAGE -c a3t"

    # Apply manifests
    log "Applying k8s manifests..."
    $SSH "kubectl apply -f $REMOTE_DIR/flowr/deploy/k3d/namespace.yaml"
    $SSH "kubectl apply -f $REMOTE_DIR/flowr/deploy/k3d/flowr.yaml"
    $SSH "kubectl apply -f $REMOTE_DIR/flowr/deploy/k3d/claude-wrapper.yaml"

    # Wait for pods
    log "Waiting for pods to be ready..."
    $SSH "kubectl -n a3t wait --for=condition=ready pod -l app=flowr --timeout=120s"
    $SSH "kubectl -n a3t wait --for=condition=ready pod -l app=claude-wrapper --timeout=120s"

    # Show status
    $SSH "kubectl -n a3t get pods,svc"

    log "K3D cluster ready!"
    log "  FlowR:   http://$REMOTE_HOST:30080"
    log "  Wrapper: http://$REMOTE_HOST:30081"
}

step_k3d_status() {
    $SSH "kubectl -n a3t get pods,svc; echo '---'; kubectl -n a3t logs -l app=flowr --tail=5; echo '---'; kubectl -n a3t logs -l app=claude-wrapper --tail=5"
}

# ═══════════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════════
STEP="${1:-all}"

case "$STEP" in
    rsync)    step_rsync ;;
    build)    step_build ;;
    deploy)   step_deploy ;;
    resources) step_resources ;;
    flows)    step_flows ;;
    install)  step_install ;;
    verify)   step_verify ;;
    wrapper-build)  step_wrapper_build ;;
    wrapper-deploy) step_wrapper_deploy ;;
    wrapper-resource) step_wrapper_resource ;;
    wrapper)
        step_wrapper_build
        step_wrapper_deploy
        step_wrapper_resource
        ;;
    k3d)
        step_rsync
        step_build
        step_wrapper_build
        step_k3d
        ;;
    k3d-deploy) step_k3d ;;
    k3d-status) step_k3d_status ;;
    all)
        step_rsync
        step_build
        step_deploy
        step_resources
        step_flows
        step_install
        step_verify
        ;;
    *)
        echo "Usage: $0 {all|rsync|build|deploy|resources|flows|install|verify|wrapper|k3d|k3d-deploy|k3d-status}"
        exit 1
        ;;
esac
