#!/usr/bin/env bash
# Verifies wiki pages: template sections, link targets, source anchors, mermaid types.
# Usage: ./scripts/check-wiki.sh [page.md ...]   (no args = all pages except README/TEMPLATE)
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

CANONICAL="README.md TEMPLATE.md core.md storage.md ingestion.md pipeline.md retrieval.md evidence.md engine.md interfaces.md evaluation.md"
SECTIONS=("## Purpose" "## Position in the System" "## Architecture" "## Runtime Flows" "## Key Decisions" "## Implementation Notes" "## Source Anchors" "## Related Pages")
MERMAID_TYPES="graph|flowchart|sequenceDiagram|classDiagram|stateDiagram|erDiagram"

pages=("$@")
if [ ${#pages[@]} -eq 0 ]; then
  pages=()
  for f in wiki/*.md; do
    b=$(basename "$f")
    [ "$b" = "README.md" ] || [ "$b" = "TEMPLATE.md" ] || pages+=("$b")
  done
fi

fail=0
for p in "${pages[@]}"; do
  f="wiki/$(basename "$p")"
  [ -f "$f" ] || { echo "FAIL: $f missing"; fail=1; continue; }

  for s in "${SECTIONS[@]}"; do
    grep -qxF "$s" "$f" || { echo "FAIL: $f missing section '$s'"; fail=1; }
  done

  # Relative .md links must target canonical pages
  while IFS= read -r target; do
    base=$(basename "$target")
    echo "$CANONICAL" | tr ' ' '\n' | grep -qxF "$base" \
      || { echo "FAIL: $f links to non-canonical page '$target'"; fail=1; }
  done < <(grep -oE '\]\([a-z-]+\.md\)' "$f" | sed 's/](\(.*\))/\1/' || true)

  # No links into an untracked directory
  if grep -qE '\]\((\.\./)?docs/' "$f"; then echo "FAIL: $f links into an untracked directory"; fail=1; fi

  # Source anchors must exist in the repo
  while IFS= read -r anchor; do
    a="${anchor%%/ (crate)*}"; a="${a% (crate)}"
    [ -e "$a" ] || { echo "FAIL: $f anchor does not exist: $a"; fail=1; }
  done < <(awk '/^## Source Anchors/,/^## Related Pages/' "$f" | grep -oE '`[^`]+`' | tr -d '\`' || true)

  # Mermaid fences must open with a known diagram type
  while IFS= read -r first; do
    echo "$first" | grep -qE "^($MERMAID_TYPES)" \
      || { echo "FAIL: $f mermaid block starts with unknown type: '$first'"; fail=1; }
  done < <(awk '/^```mermaid/{getline; print}' "$f" || true)

  # No line-number references (e.g. foo.rs:123, foo.py:45); scheme URLs (https://host:port) are exempt
  if grep -nE '\.[a-zA-Z]+:[0-9]+' "$f" | grep -vE '://'; then echo "FAIL: $f contains line-number references"; fail=1; fi
done

[ $fail -eq 0 ] && echo "check-wiki: OK (${#pages[@]} page(s))"
exit $fail
