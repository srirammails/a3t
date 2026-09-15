#!/usr/bin/env bash
# Deploy the built A3T UI (build/) to Bunny.net Storage.
#
# Required env:
#   BUNNY_STORAGE_ZONE   Storage Zone name
#   BUNNY_STORAGE_KEY    Storage Zone password (FTP & API AccessKey)
# Optional env:
#   BUNNY_STORAGE_REGION Region code: ny la sg syd uk se br jh (empty = Falkenstein/default)
#   BUNNY_DEST           Subpath within the zone (default: zone root)
#   BUNNY_PULLZONE_ID + BUNNY_ACCOUNT_KEY   Purge the pull-zone cache after upload
#
# Usage:
#   npm run build
#   BUNNY_STORAGE_ZONE=my-zone BUNNY_STORAGE_KEY=xxxx ./deploy-bunny.sh
#   # or: source .env.bunny && ./deploy-bunny.sh
set -euo pipefail
cd "$(dirname "$0")"

: "${BUNNY_STORAGE_ZONE:?set BUNNY_STORAGE_ZONE}"
: "${BUNNY_STORAGE_KEY:?set BUNNY_STORAGE_KEY}"
REGION="${BUNNY_STORAGE_REGION:-}"
DEST="${BUNNY_DEST:-}"
BUILD_DIR="build"

if [ -n "$REGION" ]; then HOST="${REGION}.storage.bunnycdn.com"; else HOST="storage.bunnycdn.com"; fi
BASE="https://${HOST}/${BUNNY_STORAGE_ZONE}"
DEST="${DEST#/}"; DEST="${DEST%/}"

[ -d "$BUILD_DIR" ] || { echo "No $BUILD_DIR/ — run 'npm run build' first"; exit 1; }

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
