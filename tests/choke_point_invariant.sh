#!/usr/bin/env bash
# choke_point_invariant.sh — AESOP reviewability tripwire (E3).
#
# A3T EE enforces at a small, fixed set of CENTRALIZED choke points rather than
# via per-handler #[require_permission] annotations (see .ai/A3T_EE_AESOP_
# RECONCILIATION.md §7). That design is only as safe as its invariant:
#
#   *Every* path that dispatches a tool / workflow step MUST pass through a
#   `policy_hook().check(&req)` before the side effect — and the set of such
#   paths must not silently grow.
#
# This script is the CI expression of that invariant. It fails if:
#   1. any pinned enforcement site loses its `policy_hook().check(` call
#      (someone refactored the gate out), or
#   2. a NEW `policy_hook().check(` appears that isn't in the pinned manifest
#      (an enforcement point was added without updating the review manifest —
#      forces a human to re-read it), or
#   3. a known dispatch entry point gains a sibling that looks un-gated.
#
# It is intentionally greppable and dependency-free: a security invariant you
# cannot read in 30 seconds is one nobody audits.

set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
red()  { printf '\033[31m%s\033[0m\n' "$*"; }
grn()  { printf '\033[32m%s\033[0m\n' "$*"; }

# --- The pinned manifest: file  →  substring that MUST be present -------------
# Each row is a centralized choke point. Keep this list == the actual set of
# enforcement sites. Adding/removing a site is a deliberate, reviewed change.
declare -a SITES=(
  "shared/flowr-mcp-core/src/tools.rs|flowr_policy::policy_hook()|site A — mcp:call (handle_tool_call)"
  "shared/swf-runtime/src/executor/mod.rs|flowr_policy::policy_hook().check(|site B — tool:invoke (executor dispatch)"
  "flowr-ce/crates/flowr-ce/src/lib.rs|flowr_policy::policy_hook().check(|agent-tool dispatch (GATED_WRITES)"
  "flowr-ce/crates/flowr-ce/src/registry.rs|flowr_policy::policy_hook().check(|registry write"
)

echo "== 1. pinned enforcement sites still gated =="
for row in "${SITES[@]}"; do
  IFS='|' read -r file needle label <<<"$row"
  if [[ ! -f "$file" ]]; then
    red "  MISSING FILE: $file ($label)"; fail=1; continue
  fi
  if grep -qF "$needle" "$file"; then
    grn "  ok   $label"
  else
    red "  GONE $label — '$needle' no longer in $file"; fail=1
  fi
done

# --- 2. no un-manifested enforcement sites -----------------------------------
# Every real `.check(` against the hook must live in a manifested file. A new
# one elsewhere means an enforcement point was added without review here.
echo "== 2. no enforcement site outside the manifest =="
mapfile -t found < <(grep -rlF "policy_hook().check(" \
  shared flowr flowr-ce --include='*.rs' \
  --exclude-dir=target --exclude-dir=.git 2>/dev/null | sort -u)
# files allowed to contain the call: manifested sites + the hook's own defn/tests
allow=$(printf '%s\n' "${SITES[@]}" | cut -d'|' -f1; echo "shared/flowr-policy/src/lib.rs")
for f in "${found[@]}"; do
  if ! grep -qxF "$f" <<<"$allow"; then
    red "  UNMANIFESTED enforcement site: $f — add it to SITES[] after review"; fail=1
  fi
done
[[ $fail -eq 0 ]] && grn "  ok   every .check() site is manifested"

# --- 3. bypass tripwire ------------------------------------------------------
# The executor has exactly ONE step-dispatch call — `handler.execute(...)` — and
# it sits immediately after the `policy_hook().check()` deny-return, so no step
# reaches its handler un-gated. Guard against a SECOND `handler.execute(`
# appearing (a copy-pasted dispatch path that skips the gate).
echo "== 3. executor dispatch bypass tripwire =="
exec_file="shared/swf-runtime/src/executor/mod.rs"
dispatch_calls=$(grep -oE "handler\.execute\(" "$exec_file" 2>/dev/null | wc -l | tr -d ' ')
if [[ "$dispatch_calls" -ne 1 ]]; then
  red "  $dispatch_calls 'handler.execute(' calls in $exec_file (expected exactly 1)."
  red "  A second dispatch path may bypass the gate at the check() above line 542. Review manually."
  fail=1
else
  grn "  ok   single gated dispatch call (handler.execute)"
fi

echo
if [[ $fail -ne 0 ]]; then
  red "CHOKE-POINT INVARIANT VIOLATED — see above."
  red "If a change here is intentional, update tests/choke_point_invariant.sh."
  exit 1
fi
grn "choke-point invariant holds."
