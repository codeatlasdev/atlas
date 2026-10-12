#!/usr/bin/env bash
# check-skills.sh — validate every skills/<name>/SKILL.md against the Agent Skills spec
# (https://agentskills.io/specification). Usage: scripts/check-skills.sh [skills-dir]
# Exit 0 = all valid, 1 = at least one problem (each printed as "<file>: <problem>").
set -euo pipefail

ROOT="${1:-$(cd "$(dirname "$0")/.." && pwd)/skills}"
ALLOWED="name description license compatibility metadata allowed-tools"
fail=0

err() { echo "$1: $2" >&2; fail=1; }

shopt -s nullglob
dirs=("$ROOT"/*/)
[ ${#dirs[@]} -gt 0 ] || { echo "no skills under $ROOT" >&2; exit 1; }

for stray in "$ROOT"/*.md; do
  err "$stray" "flat skills/*.md are unsupported; use skills/<name>/SKILL.md"
done

for d in "${dirs[@]}"; do
  dir="$(basename "$d")"
  f="$d/SKILL.md"
  [ -f "$f" ] || { err "$d" "missing SKILL.md"; continue; }

  [ "$(sed -n 1p "$f")" = "---" ] || { err "$f" "must start with '---'"; continue; }
  end=$(awk 'NR>1 && $0=="---"{print NR; exit}' "$f")
  [ -n "$end" ] || { err "$f" "unclosed frontmatter"; continue; }
  fm=$(sed -n "2,$((end - 1))p" "$f")

  # Top-level keys (lines starting in column 0) must be spec fields.
  while IFS= read -r key; do
    case " $ALLOWED " in *" $key "*) ;; *) err "$f" "unsupported frontmatter key '$key' (use metadata:)";; esac
  done < <(printf '%s\n' "$fm" | sed -n 's/^\([A-Za-z0-9_-]*\):.*/\1/p')

  name=$(printf '%s\n' "$fm" | sed -n 's/^name:[[:space:]]*//p' | tr -d '"'"'" )
  desc=$(printf '%s\n' "$fm" | sed -n 's/^description:[[:space:]]*//p')

  [ "$name" = "$dir" ] || err "$f" "name '$name' must equal directory '$dir'"
  printf '%s' "$name" | grep -Eq '^[a-z0-9]+(-[a-z0-9]+)*$' || err "$f" "name must be lowercase letters, digits, single hyphens"
  [ ${#name} -le 64 ] || err "$f" "name longer than 64 chars"

  [ -n "$desc" ] || err "$f" "description missing or empty"
  case "$desc" in '>'*|'|'*) err "$f" "description must be a single line";; esac
  [ ${#desc} -le 1024 ] || err "$f" "description is ${#desc} chars (max 1024)"

  # Relative .md links from SKILL.md must exist.
  while IFS= read -r link; do
    [ -f "$d/$link" ] || err "$f" "broken link: $link"
  done < <(sed -n 's/.*](\([A-Za-z0-9_./-]*\.md\)).*/\1/p' "$f")

  lines=$(wc -l < "$f")
  [ "$lines" -le 500 ] || err "$f" "$lines lines (keep SKILL.md under 500)"
done

[ "$fail" -eq 0 ] && echo "skills OK (${#dirs[@]} checked)"
exit "$fail"
