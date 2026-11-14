#!/usr/bin/env bash
# test-labels.sh — checks .github/labels.yml and scripts/sync-labels.sh.
# Fails on: invalid YAML, duplicate names (case-insensitive: GitHub treats them as one label),
# a color that is not 6 hex digits, a description over 100 characters or missing, a name over 50
# characters, a double quote/$/backtick/backslash in name or description (the dry run prints them
# double-quoted), and a --dry-run whose output does not match the file (one command per label).
# Usage: scripts/test-labels.sh [path/to/labels.yml]   (default: .github/labels.yml)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
file="${1:-$root/.github/labels.yml}"
fail=0
# Python 3 under any of its names (on Windows `python3` is often the Store stub): scripts/py.sh.
python3() { "$root/scripts/py.sh" "$@"; }

python3 - "$file" <<'PY' || fail=1
import re, sys, yaml
path = sys.argv[1]
try:
    labels = yaml.safe_load(open(path))
except yaml.YAMLError as e:
    sys.exit(f"{path}: invalid YAML: {e}")
if not isinstance(labels, list) or not labels:
    sys.exit(f"{path}: expected a non-empty list of labels")
errors, seen = [], {}
for i, l in enumerate(labels, 1):
    if not isinstance(l, dict):
        errors.append(f"#{i}: not a mapping"); continue
    name = l.get("name")
    if not isinstance(name, str) or not name.strip():
        errors.append(f"#{i}: missing name"); continue
    if len(name) > 50:
        errors.append(f"{name!r}: name is {len(name)} chars (max 50)")
    key = name.lower()
    if key in seen:
        errors.append(f"{name!r}: duplicate of #{seen[key]}")
    seen[key] = i
    color = l.get("color")
    if not isinstance(color, str) or not re.fullmatch(r"[0-9a-fA-F]{6}", color):
        errors.append(f"{name!r}: color {color!r} is not 6 hex digits (quote it: YAML reads 123456 as a number)")
    desc = l.get("description")
    if not isinstance(desc, str) or not desc.strip():
        errors.append(f"{name!r}: missing description")
    elif len(desc) > 100:
        errors.append(f"{name!r}: description is {len(desc)} chars (max 100)")
    for k in ("name", "description"):
        if isinstance(l.get(k), str) and re.search(r'["$`\\\\]', l[k]):
            errors.append(f"{name!r}: {k} contains a double quote, $, backtick or backslash (the dry run prints it double-quoted)")
    extra = set(l) - {"name", "color", "description"}
    if extra:
        errors.append(f"{name!r}: unknown keys {sorted(extra)}")
if errors:
    sys.exit("\n".join(f"{path}: {e}" for e in errors))
print(f"labels.yml: {len(labels)} labels, names unique, colors and descriptions valid")
PY

# The dry run must print exactly one `gh label create` per label and touch nothing.
if [[ $fail == 0 ]]; then
  want="$(python3 -c 'import sys,yaml; print(len(yaml.safe_load(open(sys.argv[1]))))' "$file")"
  out="$("$root/scripts/sync-labels.sh" --dry-run --file "$file" 2>/dev/null)" || { echo "sync-labels.sh --dry-run failed" >&2; fail=1; }
  got="$(grep -c '^gh label create ' <<<"$out" || true)"
  if [[ "$got" != "$want" ]]; then
    echo "sync-labels.sh --dry-run printed $got commands, expected $want" >&2; fail=1
  elif grep -qv '^gh label create .* --force$' <<<"$out"; then
    echo "sync-labels.sh --dry-run printed a line that is not a forced gh label create" >&2; fail=1
  else
    echo "sync-labels.sh --dry-run: $got commands, all --force"
  fi
fi

exit $fail
