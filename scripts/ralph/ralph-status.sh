#!/usr/bin/env bash
set -euo pipefail

# Show Ralph progress status
# Usage: ./ralph-status.sh

PRD_FILE="prd.json"

[ -f "$PRD_FILE" ] || {
  echo "No prd.json found."
  exit 1
}
command -v jq >/dev/null 2>&1 || {
  echo "jq required."
  exit 1
}

echo "=== Ralph Status ==="
echo ""

TOTAL=$(jq '.userStories | length' "$PRD_FILE")
DONE=$(jq '[.userStories[] | select(.passes == true)] | length' "$PRD_FILE")
BRANCH=$(jq -r '.branchName // "unknown"' "$PRD_FILE")
echo "Branch:   $BRANCH"
echo "Progress: $DONE/$TOTAL stories"
echo ""

jq -r '.userStories[] | (if .passes then "  ✓" else "  ✗" end) + " [\(.priority)] \(.id): \(.title)"' "$PRD_FILE"

echo ""
if [ "$DONE" -eq "$TOTAL" ]; then
  echo "Status: COMPLETE"
else
  NEXT=$(jq -r '[.userStories[] | select(.passes == false)] | sort_by(.priority) | .[0] | "\(.id): \(.title)"' "$PRD_FILE")
  echo "Next: $NEXT"
fi
