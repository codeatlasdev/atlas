#!/usr/bin/env bash
# check-dco.sh — every commit in <base>..<head> must carry a Signed-off-by trailer
# (https://developercertificate.org/) and be authored and committed by a real identity: a placeholder (an e-mail on a
# domain reserved for examples or a machine's own host name, or a template name) cannot be credited or reached.
# Incident (1.5.0-alpha.26): two commits by "First User <first@example.com>", signed off with the same placeholder,
# passed this check. Merge commits and the bots listed below are exempt.
# Usage: scripts/check-dco.sh <base-sha> <head-sha>
set -euo pipefail

base="${1:?base sha}"; head="${2:?head sha}"
fail=0

# A name or e-mail nobody can be found by: RFC 2606/6761 example and test domains, git's guessed user@host.local,
# localhost, and the names templates and tutorials fill in.
placeholder() {
  local name email
  name="$(tr '[:upper:]' '[:lower:]' <<<"$1")"; email="$(tr '[:upper:]' '[:lower:]' <<<"$2")"
  [[ "$email" =~ @(([^@]+\.)?example\.(com|org|net)|.+\.(example|test|invalid|localhost|local|localdomain)|localhost)$ ]] && return 0
  [[ "$name" =~ ^(your[\ _-]?name|first[\ _-]?user|test[\ _-]?user|example[\ _-]?user|user[\ _-]?name)$ ]] && return 0
  return 1
}

while read -r sha; do
  [[ -n "$sha" ]] || continue
  author_email="$(git show -s --format='%ae' "$sha")"
  case "$author_email" in
    *dependabot*|*'[bot]@users.noreply.github.com') continue ;;
  esac
  if ! git show -s --format='%B' "$sha" | grep -Eq '^Signed-off-by: .+ <[^<>[:space:]]+@[^<>[:space:]]+>[[:space:]]*$'; then
    echo "::error::commit $(git show -s --format='%h %s' "$sha") has no Signed-off-by (use: git commit -s / git rebase --signoff)"
    fail=1
  fi
  for who in "%an|%ae|author" "%cn|%ce|committer"; do
    IFS='|' read -r fmt_name fmt_email role <<<"$who"
    name="$(git show -s --format="$fmt_name" "$sha")"; email="$(git show -s --format="$fmt_email" "$sha")"
    if placeholder "$name" "$email"; then
      echo "::error::commit $(git show -s --format='%h %s' "$sha") has a placeholder identity as $role: $name <$email> (set git config user.name/user.email to yours, then git commit --amend --reset-author or git rebase -r --exec 'git commit --amend --no-edit --reset-author')"
      fail=1
    fi
  done
done < <(git rev-list --no-merges "$base..$head")

if [[ $fail == 1 ]]; then
  echo "DCO: sign off every commit, see CONTRIBUTING.md#developer-certificate-of-origin-dco" >&2
  exit 1
fi
echo "DCO: all commits signed off"
