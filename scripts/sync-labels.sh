#!/usr/bin/env bash
# sync-labels.sh — make the repository's labels match .github/labels.yml, from your machine
# (no Actions minutes). Idempotent: `gh label create --force` creates a missing label and
# updates the color and description of an existing one. It never deletes anything.
#
# Usage: scripts/sync-labels.sh [--dry-run] [--prune] [--repo OWNER/REPO] [--file PATH]
#   --dry-run  print the `gh label create` commands, run nothing, need no login
#   --prune    after syncing, LIST labels on the repo that are not in the file (never deletes)
#   --repo     default: codeatlasdev/atlas
#   --file     default: .github/labels.yml
set -euo pipefail

repo="codeatlasdev/atlas"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
file="$root/.github/labels.yml"
dry=0
prune=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) dry=1 ;;
    --prune) prune=1 ;;
    --repo) repo="${2:?--repo needs OWNER/REPO}"; shift ;;
    --file) file="${2:?--file needs a path}"; shift ;;
    -h|--help) sed -n '2,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

[[ -f "$file" ]] || { echo "labels file not found: $file" >&2; exit 2; }
# Python 3 under any of its names (on Windows `python3` is often the Store stub): scripts/py.sh.
python3() { "$(dirname "${BASH_SOURCE[0]}")/py.sh" "$@"; }
if [[ $dry == 0 ]]; then
  command -v gh >/dev/null || { echo "gh is required (https://cli.github.com)" >&2; exit 2; }
fi

# One label per line as: name<TAB>color<TAB>description (tabs and newlines cannot occur in them).
rows="$(python3 - "$file" <<'PY'
import sys, yaml
labels = yaml.safe_load(open(sys.argv[1]))
for l in labels:
    for k in ("name", "color", "description"):
        if "\t" in str(l[k]) or "\n" in str(l[k]):
            sys.exit(f"label {l['name']!r}: {k} contains a tab or newline")
    print(f"{l['name']}\t{l['color']}\t{l['description']}")
PY
)" || { echo "could not read $file with Python 3 + PyYAML (scripts/py.sh)" >&2; exit 2; }

names=()
while IFS=$'\t' read -r name color desc; do
  [[ -n "$name" ]] || continue
  names+=("$name")
  if [[ $dry == 1 ]]; then
    # Shown double-quoted for readability; names and descriptions cannot contain a double quote
    # (checked by scripts/test-labels.sh), so the line can be pasted into a shell as is.
    printf 'gh label create --repo %s "%s" --color %s --description "%s" --force\n' "$repo" "$name" "$color" "$desc"
  else
    gh label create --repo "$repo" "$name" --color "$color" --description "$desc" --force
  fi
done <<<"$rows"

if [[ $prune == 1 ]]; then
  if [[ $dry == 1 ]]; then
    echo "# --prune (dry run): would list labels of $repo that are not in $file" >&2
  else
    echo "Labels on $repo that are not in $(basename "$file") (not deleted):"
    gh label list --repo "$repo" --limit 500 --json name --jq '.[].name' | sort > /tmp/atlas-labels-remote.$$
    printf '%s\n' "${names[@]}" | sort > /tmp/atlas-labels-file.$$
    comm -23 /tmp/atlas-labels-remote.$$ /tmp/atlas-labels-file.$$ | sed 's/^/  /'
    rm -f /tmp/atlas-labels-remote.$$ /tmp/atlas-labels-file.$$
  fi
fi

echo "$([[ $dry == 1 ]] && echo 'dry run: ')${#names[@]} labels from $(basename "$file")" >&2
