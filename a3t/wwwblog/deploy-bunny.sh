#!/usr/bin/env bash
# Build and deploy the A3T blog (wwwblog/) to Bunny.net Storage. Same flow as www/deploy-bunny.sh.
#
# Required env (or put them in an untracked .env.bunny next to this script):
#   BUNNY_STORAGE_ZONE   Storage Zone name for blog.a3t.io
#   BUNNY_STORAGE_KEY    Storage Zone password (NOT the account API key)
# Optional env:
#   BUNNY_STORAGE_REGION Region code: ny la sg syd uk se br jh (empty = Falkenstein/default)
#   BUNNY_DEST           Subpath within the zone (default: zone root; "blog" for a3t.io/blog)
#   BUNNY_PULLZONE_ID + BUNNY_ACCOUNT_KEY   Purge the pull-zone cache after upload
#
# Usage:
#   ./deploy-bunny.sh            # build + upload
#   ./deploy-bunny.sh --build    # build dist/ only
#   ALLOW_SAMPLES=1 ./deploy-bunny.sh   # deploy even with sample-* content published
set -euo pipefail
cd "$(dirname "$0")"

BUILD_DIR="dist"

build() {
  [ -d node_modules ] || npm ci --no-bin-links  # /home/coder (CIFS) has no symlinks
  npm run build
  echo "Built $BUILD_DIR/ ($(find "$BUILD_DIR" -type f | wc -l) files, $(du -sb "$BUILD_DIR" | cut -f1 | numfmt --to=iec))"
}

build
[ "${1:-}" = "--build" ] && exit 0

# The template ships with sample-* posts; don't publish them by accident.
samples=$(find src/content -name 'sample-*' -exec grep -L '^draft: true' {} + || true)
if [ -n "$samples" ] && [ "${ALLOW_SAMPLES:-}" != "1" ]; then
  echo "Refusing to deploy: sample content is still published:" >&2
  echo "$samples" | sed 's/^/  /' >&2
  echo "Delete them, set 'draft: true', or run with ALLOW_SAMPLES=1." >&2
  exit 1
fi

[ -f .env.bunny ] && source .env.bunny
: "${BUNNY_STORAGE_ZONE:?set BUNNY_STORAGE_ZONE}"
: "${BUNNY_STORAGE_KEY:?set BUNNY_STORAGE_KEY}"
REGION="${BUNNY_STORAGE_REGION:-}"
DEST="${BUNNY_DEST:-}"

if [ -n "$REGION" ]; then HOST="${REGION}.storage.bunnycdn.com"; else HOST="storage.bunnycdn.com"; fi
BASE="https://${HOST}/${BUNNY_STORAGE_ZONE}"
DEST="${DEST#/}"; DEST="${DEST%/}"

echo "Uploading $BUILD_DIR/ -> ${BASE}/${DEST}"
count=0
while IFS= read -r -d '' f; do
  rel="${f#"$BUILD_DIR"/}"
  url="${BASE}/${DEST:+$DEST/}${rel}"
  curl -sf -X PUT "$url" \
    -H "AccessKey: ${BUNNY_STORAGE_KEY}" \
    -H "Content-Type: application/octet-stream" \
    --data-binary "@${f}" > /dev/null
  echo "  ✓ ${rel}"
  count=$((count + 1))
done < <(find "$BUILD_DIR" -type f -print0)
echo "Uploaded $count files."

if [ -n "${BUNNY_PULLZONE_ID:-}" ] && [ -n "${BUNNY_ACCOUNT_KEY:-}" ]; then
  echo "Purging pull zone ${BUNNY_PULLZONE_ID} cache..."
  curl -sf -X POST "https://api.bunny.net/pullzone/${BUNNY_PULLZONE_ID}/purgeCache" \
    -H "AccessKey: ${BUNNY_ACCOUNT_KEY}" > /dev/null && echo "  ✓ purged"
fi
echo "Done."
