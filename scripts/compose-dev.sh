#!/usr/bin/env bash
# Compose the official tree for local testing with this checkout's UNCOMMITTED changes too:
# benchmesh-services compose.sh (committed files at HEAD + its overlay), then the changed and new
# files of this working tree copied over (never the overlay's own paths).
#   scripts/compose-dev.sh [output dir, default ../benchmesh-official]
set -euo pipefail
app=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-$app/../benchmesh-official}
"$app/../benchmesh-services/scripts/compose.sh" "$app" "$out" HEAD
cd "$app"
git ls-files -m -o --exclude-standard | grep -v '^crates/benchmesh-extensions/' | while read -r f; do
  [ -f "$f" ] && mkdir -p "$out/$(dirname "$f")" && cp -a "$f" "$out/$f"
done
echo "  + uncommitted changes of $app"
