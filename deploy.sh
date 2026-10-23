#!/usr/bin/env bash
set -euo pipefail

# ═══════════════════════════════════════════════════════════════════
# Build, push and roll A3T images (FlowR CE/EE, ADB)
#
# Runs in the code-server pod. Follows the runbook in Dev/.ai/deployment.md:
#   1. cargo build the release binary here
#   2. rebase it onto a runtime image with buildah (from/copy/commit only;
#      `buildah run` is blocked in this pod)
#   3. push to ghcr.io/srirammails/* with the login in
#      ~/.config/containers/auth.json
#   4. roll the Deployment (kubectl with ~/.kube/<env>.kubeconfig, or via the
#      metal box)
#
# Usage:
#   ./deploy.sh release <target> [tag]             build + image + push
#   ./deploy.sh build   <target>
#   ./deploy.sh image   <target> [tag]
#   ./deploy.sh push    <target> [tag]
#   ./deploy.sh roll    <target> <tag> [vc-dev|vc-prod]
#   ./deploy.sh ui                                 build a3t/ui, then restart ui-serve
#   ./deploy.sh ui-serve                           (re)start the dash.dev.a3t.io server
#
#   targets: flowr-ce  flowr-ee  adb
#   default tag: YYYY-MM-DD-ce / YYYY-MM-DD-ee for FlowR, YYYY-MM-DD for ADB
#
# The A3T UI is not an image. dash.dev.a3t.io is the gateway (128.140.26.157)
# forwarding to port 4173 on this pod, where a3t/ui/serve-dev.py serves
# a3t/ui/build/. Nothing restarts it after a pod restart: run `ui-serve`.
# dash.a3t.io is the same build on Bunny (a3t/ui/deploy-bunny.sh).
#
# Don't edit this file while it runs: bash reads it as it goes, and on the
# CIFS home mount a replaced open file gets deleted when it closes.
#
# Replaces the old SSH + docker deploy to 37.27.195.4 (host retired);
# that version is in git history.
# ═══════════════════════════════════════════════════════════════════

FLOWR_DIR="$(cd "$(dirname "$0")" && pwd)"
DEV_DIR="$(dirname "$FLOWR_DIR")"
ADB_DIR="$DEV_DIR/ADB"

# Build output and buildah storage stay off the CIFS home mount (slow, and
# chmod has no effect there).
BUILD_DIR="${BUILD_DIR:-/var/tmp/a3t-build}"
REGISTRY="ghcr.io/srirammails"
# FlowR runtime image (entrypoint /usr/local/bin/flowr, /config). Must be Debian trixie
# like this pod: binaries built here need glibc 2.41.
FLOWR_BASE="${FLOWR_BASE:-$REGISTRY/flowr:2026-10-04-ee-ops9}"
ADB_BASE="${ADB_BASE:-docker.io/library/debian:trixie-slim}"

UI_DIR="$FLOWR_DIR/a3t/ui"
# The repo's node_modules has only the macOS rolldown binding; this tree has Linux's.
UI_BUILD_TREE="${UI_BUILD_TREE:-$HOME/build/Dev/flowr-system/a3t/ui}"
UI_PORT=4173
UI_STATE="$HOME/.local/share/a3t-ui"

METAL_HOST="${METAL_HOST:-root@138.201.138.90}"
SSH_KEY="${SSH_KEY:-$HOME/.ssh/sri_github}"
ADB_NAMESPACE="${ADB_NAMESPACE:-a3t}"

export PATH="$HOME/.cargo/bin:$PATH"
export BUILDAH_ISOLATION=chroot

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

log()  { echo -e "${GREEN}[deploy]${NC} $1"; }
err()  { echo -e "${RED}[deploy]${NC} $1" >&2; }

usage() {
    sed -n '/^# Usage:/,/^#   default tag/p' "$0" | sed 's/^# \{0,1\}//'
    exit 1
}

ba() {
    buildah --storage-driver vfs --storage-opt vfs.ignore_chown_errors=true \
        --root "$BUILD_DIR/buildah/root" --runroot "$BUILD_DIR/buildah/run" "$@" \
        2> >(grep -v 'newuidmap\|newgidmap\|single mapping' >&2)
}

repo_for() {
    case "$1" in
        flowr-ce|flowr-ee) echo "$REGISTRY/flowr" ;;
        adb)               echo "$REGISTRY/adb" ;;
        *) err "Unknown target: $1"; usage ;;
    esac
}

default_tag() {
    case "$1" in
        flowr-ce) echo "$(date +%F)-ce" ;;
        flowr-ee) echo "$(date +%F)-ee" ;;
        adb)      date +%F ;;
    esac
}

binary_for() { echo "$BUILD_DIR/bin/$1"; }

# ═══════════════════════════════════════════════════════════════════
# BUILD — release binary into $BUILD_DIR/bin/<target>
# ═══════════════════════════════════════════════════════════════════
step_build() {
    local target="$1"
    mkdir -p "$BUILD_DIR/bin"
    case "$target" in
        flowr-ce|flowr-ee)
            local features=()
            [ "$target" = flowr-ee ] && features=(--features ee)
            log "Building $target (cargo, ~16 min from clean)..."
            (cd "$FLOWR_DIR/flowr" && CARGO_TARGET_DIR="$BUILD_DIR/target-$target" \
                cargo build --release -p flowr-server "${features[@]}")
            cp "$BUILD_DIR/target-$target/release/flowr" "$(binary_for "$target")"
            ;;
        adb)
            # ADB/.cargo/config.toml (gitignored) points the AQL parser at ../AQL.
            log "Building adb (cargo)..."
            (cd "$ADB_DIR" && CARGO_TARGET_DIR="$BUILD_DIR/target-adb" \
                cargo build --release -p adb-cli)
            cp "$BUILD_DIR/target-adb/release/adb" "$(binary_for adb)"
            ;;
    esac
    log "Binary: $(binary_for "$target")"
}

# ═══════════════════════════════════════════════════════════════════
# IMAGE — rebase the binary onto the runtime image
# ═══════════════════════════════════════════════════════════════════
step_image() {
    local target="$1" tag="$2" image ctr
    image="$(repo_for "$target"):$tag"
    [ -x "$(binary_for "$target")" ] || { err "No binary; run: $0 build $target"; exit 1; }

    log "Building image $image..."
    case "$target" in
        flowr-ce|flowr-ee)
            ctr=$(ba from "$FLOWR_BASE")
            ba copy "$ctr" "$(binary_for "$target")" /usr/local/bin/flowr
            ;;
        adb)
            # No apt in the image (buildah run is blocked), so the CA bundle comes
            # from this pod (same Debian release as the base).
            ctr=$(ba from "$ADB_BASE")
            ba copy --chmod 755 "$ctr" "$(binary_for adb)" /usr/local/bin/adb
            ba copy "$ctr" /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
            ba config --port 3000 --entrypoint '[]' \
                --cmd '["adb", "serve-http", "--port", "3000"]' "$ctr"
            ;;
    esac
    ba commit --squash "$ctr" "$image" > /dev/null
    ba rm "$ctr" > /dev/null
    log "Image: $image"
}

# ═══════════════════════════════════════════════════════════════════
# PUSH — to GHCR (login: ~/.config/containers/auth.json; if it 403s,
# refresh it from the cluster's registry-credentials secret, see
# Dev/.ai/deployment.md)
# ═══════════════════════════════════════════════════════════════════
step_push() {
    local image
    image="$(repo_for "$1"):$2"
    log "Pushing $image..."
    ba push "$image"
    log "Pushed: $image"
}

# ═══════════════════════════════════════════════════════════════════
# ROLL — set the new image on the Deployment
# ═══════════════════════════════════════════════════════════════════
step_roll() {
    local target="$1" tag="$2" env="${3:-vc-dev}" image kubeconfig ns remote
    image="$(repo_for "$target"):$tag"
    case "$env" in
        vc-dev)  kubeconfig=/root/vc-dev.kubeconfig ;;
        vc-prod) kubeconfig=/root/vprod.kubeconfig ;;
        *) err "Unknown environment: $env"; usage ;;
    esac

    case "$target" in
        flowr-ce|flowr-ee)
            local hook=allow_all
            ns=a3t-ce
            if [ "$target" = flowr-ee ]; then hook=cedar; ns=a3t; fi
            # deploy/flowr runs the image twice: init container fix-perms + flowr.
            remote="kubectl -n $ns set image deploy/flowr flowr=$image fix-perms=$image && \
                kubectl -n $ns set env deploy/flowr FLOWR_SECURITY_POLICY_HOOK=$hook && \
                kubectl -n $ns rollout status deploy/flowr --timeout=300s && sleep 5 && \
                IP=\$(kubectl -n $ns get pod -l app=flowr -o jsonpath='{.items[?(@.status.phase==\"Running\")].status.podIP}' | awk '{print \$1}') && \
                curl -s http://\$IP:3002/edition; echo"
            ;;
        adb)
            ns="$ADB_NAMESPACE"
            remote="kubectl -n $ns set image deploy/adb adb=$image && \
                kubectl -n $ns rollout status deploy/adb --timeout=180s"
            ;;
    esac

    log "Rolling $image to $env (ns $ns)..."
    # A Rancher kubeconfig at ~/.kube/<env>.kubeconfig works from this pod; otherwise
    # go through the metal box (the SSH key is passphrase-protected; ssh prompts).
    if [ -f "$HOME/.kube/$env.kubeconfig" ]; then
        KUBECONFIG="$HOME/.kube/$env.kubeconfig" bash -c "$remote"
        log "Rolled. Undo with: KUBECONFIG=~/.kube/$env.kubeconfig kubectl -n $ns rollout undo deploy/${target%%-*}"
    else
        ssh -i "$SSH_KEY" "$METAL_HOST" "export KUBECONFIG=$kubeconfig; $remote"
        log "Rolled. Undo with: ssh $METAL_HOST KUBECONFIG=$kubeconfig kubectl -n $ns rollout undo deploy/${target%%-*}"
    fi
}

# ═══════════════════════════════════════════════════════════════════
# UI — build a3t/ui in the Linux build tree, copy build/ back
# ═══════════════════════════════════════════════════════════════════
step_ui_build() {
    log "Building A3T UI in $UI_BUILD_TREE..."
    [ -d "$UI_BUILD_TREE/node_modules" ] || { err "No build tree with node_modules at $UI_BUILD_TREE"; exit 1; }
    (cd "$UI_DIR" && tar --exclude=./node_modules --exclude=./build --exclude='./build.prev-*' \
        --exclude=./.svelte-kit --exclude='./.env*' -cf - .) | tar -xf - -C "$UI_BUILD_TREE"
    (cd "$UI_BUILD_TREE" && rm -rf build && node node_modules/vite/bin/vite.js build)
    rm -rf "$UI_DIR/build"
    cp -r "$UI_BUILD_TREE/build" "$UI_DIR/build"
    log "UI build: $UI_DIR/build ($(find "$UI_DIR/build" -type f | wc -l) files)"
}

# ═══════════════════════════════════════════════════════════════════
# UI-SERVE — (re)start the static server behind dash.dev.a3t.io
# ═══════════════════════════════════════════════════════════════════
step_ui_serve() {
    local pid
    mkdir -p "$UI_STATE"
    pid=$(ss -ltnp 2>/dev/null | grep ":$UI_PORT " | grep -oE 'pid=[0-9]+' | cut -d= -f2 | head -1 || true)
    if [ -n "$pid" ]; then
        log "Stopping server on :$UI_PORT (pid $pid)..."
        kill "$pid"; sleep 1
    fi
    setsid nohup python3 -I "$UI_DIR/serve-dev.py" "$UI_DIR/build" "$UI_PORT" \
        >> "$UI_STATE/serve.log" 2>&1 < /dev/null &
    sleep 2
    pid=$(ss -ltnp 2>/dev/null | grep ":$UI_PORT " | grep -oE 'pid=[0-9]+' | cut -d= -f2 | head -1 || true)
    [ -n "$pid" ] || { err "Server did not start; see $UI_STATE/serve.log"; exit 1; }
    echo "$pid" > "$UI_STATE/serve.pid"
    log "Serving $UI_DIR/build on :$UI_PORT (pid $pid, log $UI_STATE/serve.log)"
    log "  https://dash.dev.a3t.io -> $(curl -sS -m 10 -o /dev/null -w '%{http_code}' https://dash.dev.a3t.io/ || echo unreachable)"
}

# ═══════════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════════
STEP="${1:-}"
case "$STEP" in
    ui)       step_ui_build; step_ui_serve; exit 0 ;;
    ui-serve) step_ui_serve; exit 0 ;;
esac
TARGET="${2:-}"
[ -n "$STEP" ] && [ -n "$TARGET" ] || usage
repo_for "$TARGET" > /dev/null
TAG="${3:-$(default_tag "$TARGET")}"

case "$STEP" in
    build)   step_build "$TARGET" ;;
    image)   step_image "$TARGET" "$TAG" ;;
    push)    step_push "$TARGET" "$TAG" ;;
    release)
        step_build "$TARGET"
        step_image "$TARGET" "$TAG"
        step_push "$TARGET" "$TAG"
        ;;
    roll)
        [ -n "${3:-}" ] || { err "roll needs an explicit tag"; usage; }
        step_roll "$TARGET" "$TAG" "${4:-vc-dev}"
        ;;
    *) usage ;;
esac
