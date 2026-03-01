#!/usr/bin/env bash
set -euo pipefail

# Ralph loop for Claude Code (interactive mode)
# You approve each action. Script pauses between stories for inspection.
# Usage: ./ralph.sh [-f prd.json] [max_iterations]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PRD_FILE="prd.json"
PROGRESS_FILE="$SCRIPT_DIR/progress.txt"
ARCHIVE_DIR="$SCRIPT_DIR/archive"
LAST_BRANCH_FILE="$SCRIPT_DIR/.last-branch"
LOG_FILE="$SCRIPT_DIR/ralph.log"

MAX_ITERATIONS=10
ITERATION=0

# Parse arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
  -f | --file)
    PRD_FILE="$2"
    shift 2
    ;;
  *)
    MAX_ITERATIONS="$1"
    shift
    ;;
  esac
done

# Resolve PRD_FILE to absolute path and derive RALPH_PLAN_DIR
PRD_FILE="$(cd "$(dirname "$PRD_FILE")" && pwd)/$(basename "$PRD_FILE")"
export RALPH_PLAN_DIR
RALPH_PLAN_DIR="$(dirname "$PRD_FILE")"

# If the plan lives under .ralph/plans/, use the plan's own progress.txt
if [[ "$PRD_FILE" == *"/.ralph/plans/"* ]]; then
  PROGRESS_FILE="$RALPH_PLAN_DIR/progress.txt"
fi

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
DIM='\033[2m'
NC='\033[0m'

log() { echo -e "${DIM}$(date '+%H:%M:%S')${NC} $1"; }
log_success() { echo -e "${DIM}$(date '+%H:%M:%S')${NC} ${GREEN}$1${NC}"; }
log_warn() { echo -e "${DIM}$(date '+%H:%M:%S')${NC} ${YELLOW}$1${NC}"; }
log_error() { echo -e "${DIM}$(date '+%H:%M:%S')${NC} ${RED}$1${NC}"; }

# Preflight checks
command -v claude >/dev/null 2>&1 || {
  log_error "Claude Code CLI not found. Install: npm install -g @anthropic-ai/claude-code"
  exit 1
}
command -v jq >/dev/null 2>&1 || {
  log_error "jq not found. Install: brew install jq (macOS) or apt install jq (Linux)"
  exit 1
}
[ -d "$PROJECT_ROOT/.git" ] || {
  log_error "Not a git repository. Run from project root."
  exit 1
}
[ -f "$PRD_FILE" ] || {
  log_error "No prd.json found at $PRD_FILE"
  exit 1
}

# Archive previous run if branch changed
if [ -f "$PRD_FILE" ] && [ -f "$LAST_BRANCH_FILE" ]; then
  CURRENT_BRANCH=$(jq -r '.branchName // empty' "$PRD_FILE" 2>/dev/null || echo "")
  LAST_BRANCH=$(cat "$LAST_BRANCH_FILE" 2>/dev/null || echo "")

  if [ -n "$CURRENT_BRANCH" ] && [ -n "$LAST_BRANCH" ] && [ "$CURRENT_BRANCH" != "$LAST_BRANCH" ]; then
    DATE=$(date +%Y-%m-%d)
    FOLDER_NAME=$(echo "$LAST_BRANCH" | sed 's|^ralph/||; s|^feature/||')
    ARCHIVE_FOLDER="$ARCHIVE_DIR/$DATE-$FOLDER_NAME"

    log "Archiving previous run: $LAST_BRANCH"
    mkdir -p "$ARCHIVE_FOLDER"
    [ -f "$PRD_FILE" ] && cp "$PRD_FILE" "$ARCHIVE_FOLDER/"
    [ -f "$PROGRESS_FILE" ] && cp "$PROGRESS_FILE" "$ARCHIVE_FOLDER/"
    [ -f "$LOG_FILE" ] && cp "$LOG_FILE" "$ARCHIVE_FOLDER/"

    >"$PROGRESS_FILE"
    log_success "Archived to: $ARCHIVE_FOLDER"
  fi
fi

# Save current branch name
jq -r '.branchName // empty' "$PRD_FILE" 2>/dev/null >"$LAST_BRANCH_FILE"

# Status display
show_status() {
  local total=$(jq '.tasks | length' "$PRD_FILE")
  local done=$(jq '[.tasks[] | select(.passes == true)] | length' "$PRD_FILE")
  echo -e "${CYAN}Progress: $done/$total stories complete${NC}"
}

log "Starting Ralph loop (max $MAX_ITERATIONS iterations, interactive mode)"
show_status
echo ""

while [ $ITERATION -lt $MAX_ITERATIONS ]; do
  ITERATION=$((ITERATION + 1))

  # Check if all stories are done before running
  REMAINING=$(jq '[.tasks[] | select(.passes == false)] | length' "$PRD_FILE")
  if [ "$REMAINING" -eq 0 ]; then
    log_success "All stories complete!"
    show_status
    exit 0
  fi

  NEXT_STORY=$(jq -r '[.tasks[] | select(.passes == false)] | sort_by(.priority) | .[0] | "\(.id): \(.title)"' "$PRD_FILE")
  log "Iteration $ITERATION/$MAX_ITERATIONS — Next: $NEXT_STORY"
  echo ""

  # Run the ralph sub-agent interactively (you approve each action)
  cd "$PROJECT_ROOT" && claude --agent ralph "Implement the next user story." || true

  # Log iteration
  echo "=== Iteration $ITERATION - $(date) ===" >>"$LOG_FILE"

  # Check completion
  REMAINING=$(jq '[.tasks[] | select(.passes == false)] | length' "$PRD_FILE")
  if [ "$REMAINING" -eq 0 ]; then
    echo ""
    log_success "RALPH COMPLETE — All stories implemented!"
    show_status
    exit 0
  fi

  echo ""
  show_status
  echo ""

  # Pause between iterations for inspection
  read -rp "$(echo -e "${YELLOW}Continue to next story? [Y/n] ${NC}")" REPLY
  if [[ "$REPLY" =~ ^[Nn]$ ]]; then
    log "Stopped by user after iteration $ITERATION."
    exit 0
  fi
  echo ""
done

log_warn "Reached max iterations ($MAX_ITERATIONS). $REMAINING stories remaining."
show_status
exit 1
