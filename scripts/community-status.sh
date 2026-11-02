#!/usr/bin/env bash
# Refresh the community status snapshot a release carries (crates/bm-server/community-status.json):
# the website's full status document of today, without `issues` and `live` (always fetched live). Run it
# before tagging a release. BENCHMESH_ONLINE_URL points at another site (e.g. a local one).
set -euo pipefail
base="${BENCHMESH_ONLINE_URL:-https://benchmesh.online}"
out="$(dirname "$0")/../crates/bm-server/community-status.json"
doc="$(curl -fsSL --max-time 20 "${base%/}/api/v1/status")"
echo "$doc" | jq -e '.schema == "benchmesh.status/v1" and (.generated | type == "string") and (.models | type == "object")' >/dev/null \
  || { echo "not a benchmesh.status/v1 document from $base" >&2; exit 1; }
echo "$doc" | jq --indent 2 'del(.issues, .live)' > "$out"
echo "$out: generated $(jq -r .generated "$out"), $(jq '[.models[] | length] | add // 0' "$out") models"
