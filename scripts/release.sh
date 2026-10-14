#!/usr/bin/env bash
# Cut a release on a channel: pick the next version (scripts/release-version.ts), require a green CI on HEAD,
# bump every crate, commit, tag and push. The tag push runs .github/workflows/release.yml on GitHub, which this
# script watches, then deploys the site.
#
# Usage: bun run release <alpha|beta|rc|stable|patch|X.Y.Z[-(alpha|beta|rc).N]>
#          [--dry-run] [--skip-checks] [--force-soak] [--bump major]
#        bun run release --publish vX.Y.Z[-pre.N]   (re)publish an existing tag
#
#   alpha    from main              1.4.0-alpha.N
#   beta     from main (first beta creates release/X.Y) or release/X.Y   1.4.0-beta.N
#   rc       from release/X.Y       1.4.0-rc.N
#   stable   from release/X.Y       1.4.0 (promotes a soaked beta/rc; diff limited to version files)
#   patch    from release/X.Y       1.4.(Z+1) hotfix
# See docs/release.md and docs/versioning.md.
set -euo pipefail
cd "$(dirname "$0")/.."

request="" dry=0 checks=1 force_soak=0 bump="" publish_tag=""
while (($#)); do
  case "$1" in
    --dry-run) dry=1 ;;
    --publish) shift; publish_tag="${1:?--publish needs a tag}" ;;
    --skip-checks) checks=0 ;;
    --force-soak) force_soak=1 ;;
    --bump)
      shift
      [[ "${1:-}" == major ]] || { echo "✗ --bump only accepts: major" >&2; exit 1; }
      bump=major ;;
    -*) echo "unknown arg: $1" >&2; exit 1 ;;
    *)
      [[ -z "$request" ]] || { echo "unexpected extra argument: $1 (already have '$request')" >&2; exit 1; }
      request="$1" ;;
  esac
  shift
done

die() { echo "✗ $*" >&2; exit 1; }
run() { echo "+ $*"; [[ $dry == 1 ]] || "$@"; }

# Waits for the Release run of <tag> started at or after <since> (ISO 8601) and fails unless it succeeded.
watch_release() {
  local tag="$1" since="$2" id="" conclusion
  if [[ $dry == 1 ]]; then echo "+ watch the Release run for $tag"; return 0; fi
  for _ in $(seq 24); do
    id="$(gh run list --repo codeatlasdev/atlas --workflow release.yml --json databaseId,createdAt,displayTitle,headBranch --limit 10 \
      --jq "[.[] | select(.createdAt >= \"$since\" and (.headBranch == \"$tag\" or (.displayTitle | contains(\"$tag\"))))] | sort_by(.createdAt) | last | .databaseId // empty")"
    [[ -z "$id" ]] || break
    sleep 5
  done
  [[ -n "$id" ]] || die "no Release run started for $tag"
  echo "▶ Release run: $(gh run view "$id" --repo codeatlasdev/atlas --json url -q .url)"
  # gh run watch's exit code is unreliable; the conclusion is read afterwards.
  gh run watch "$id" --repo codeatlasdev/atlas --interval 30 >/dev/null || true
  conclusion="$(gh run view "$id" --repo codeatlasdev/atlas --json conclusion -q .conclusion)"
  [[ "$conclusion" == success ]] || die "release run $id: $conclusion (gh run view $id --log-failed)"
}

# Builds and publishes a tag that is already on origin: dispatches release.yml from main with that tag.
publish_existing() {
  local since
  since="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  run gh workflow run release.yml --repo codeatlasdev/atlas --ref main -f tag="$1"
  watch_release "$1" "$since"
  deploy_site "$1"
}

[[ -n "$request" || -n "$publish_tag" ]] || die "usage: bun run release <alpha|beta|rc|stable|patch|X.Y.Z[-pre.N]> [--dry-run] [--skip-checks] [--force-soak] [--bump major] | --publish vX.Y.Z[-pre.N]"

# Belt and braces: /download and /changelog read the latest release at the edge (site/functions), so they are never stale on
# their own, but a deploy also refreshes the build-time fallback and the docs. Never fails the release: it is already published.
deploy_site() {
  if [[ -z "${CLOUDFLARE_API_TOKEN:-}" || -z "${CLOUDFLARE_ACCOUNT_ID:-}" ]]; then
    echo "⚠ site deploy skipped: CLOUDFLARE_API_TOKEN / CLOUDFLARE_ACCOUNT_ID not set (the site still shows $1 within ~1 minute; run 'bun run site:deploy' to refresh the static fallback)"
    return 0
  fi
  echo "▶ deploying the site"
  if [[ $dry == 1 ]]; then echo "+ bun run site:deploy"; return 0; fi
  bun run site:deploy || echo "⚠ site deploy failed; $1 is published and the site picks it up from GitHub within ~1 minute. Retry: bun run site:deploy" >&2
}

if [[ -n "$publish_tag" ]]; then
  git ls-remote --exit-code --tags origin "refs/tags/$publish_tag" >/dev/null 2>&1 || die "tag $publish_tag is not on origin"
  publish_existing "$publish_tag"
  exit 0
fi

crates=(crates/*/Cargo.toml)
current="$(sed -n 's/^version = "\(.*\)"/\1/p' crates/atlas-cli/Cargo.toml | head -1)"
branch="$(git rev-parse --abbrev-ref HEAD)"

# Resume: HEAD already is a pushed release tag whose GitHub Release is missing
# (e.g. publish failed last run) → only publish; never bump again.
head_tag="$(git describe --exact-match --tags --match 'v[0-9]*' HEAD 2>/dev/null || true)"
if [[ -n "$head_tag" ]] && ! gh release view "$head_tag" --repo codeatlasdev/atlas >/dev/null 2>&1; then
  echo "▶ resuming publish of $head_tag (tag exists, release missing)"
  publish_existing "$head_tag"
  exit 0
fi

# Preflight
for t in bun gh cargo git; do command -v "$t" >/dev/null || die "$t not found"; done

# Version, branch and soak rules (unit-tested): also reads tags from origin, so it sees what is already out there.
git fetch --quiet --tags origin
plan_args=(next --request "$request" --json)
[[ $force_soak == 0 ]] || plan_args+=(--force-soak)
[[ -z "$bump" ]] || plan_args+=(--bump "$bump")
plan="$(bun scripts/release-version.ts "${plan_args[@]}" || true)"
[[ -n "$plan" ]] || die "release-version.ts printed nothing"
errors="$(bun -e 'const r = JSON.parse(process.argv[1]); console.log((r.errors ?? []).join("\n"))' "$plan")"
[[ -z "$errors" ]] || die "$errors"
field() { bun -e 'const v = JSON.parse(process.argv[1])[process.argv[2]]; if (v !== undefined) console.log(Array.isArray(v) ? v.join("\n") : v)' "$plan" "$1"; }
next="$(field version)" tag="$(field tag)" channel="$(field channel)" create_branch="$(field createBranch)"
notes="$(field notes)"
[[ -n "$next" && -n "$tag" ]] || die "release-version.ts returned no version: $plan"

# The branch that gets the release commit and the tag.
if [[ -n "$create_branch" ]]; then target="$create_branch"; else target="$branch"; fi
[[ -z "$(git status --porcelain)" ]] || [[ $dry == 1 ]] || die "working tree dirty"
if [[ -z "$create_branch" ]]; then
  git fetch --quiet origin "$branch"
  [[ "$(git rev-parse HEAD)" == "$(git rev-parse "origin/$branch")" ]] || [[ $dry == 1 ]] || die "$branch not in sync with origin/$branch"
else
  git fetch --quiet origin main
  [[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] || [[ $dry == 1 ]] || die "main not in sync with origin/main (the first beta branches $create_branch from it)"
fi
git rev-parse -q --verify "refs/tags/$tag" >/dev/null && die "tag $tag already exists"
git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1 && die "tag $tag already exists on origin"
if ! grep -q "^## \[$next\]" CHANGELOG.md; then
  msg="CHANGELOG.md has no '## [$next]' section (the release notes come from it)"
  if [[ $dry == 1 ]]; then echo "⚠ $msg (a real run stops here)"; else die "$msg"; fi
fi

# Promotion determinism: stable == the soaked prerelease + version metadata.
promoted_from=""
if [[ "$channel" == stable && "$next" == *.*.0 ]]; then
  promoted_from="$(bun scripts/release-version.ts last-prerelease || true)"
  [[ -n "$promoted_from" ]] || die "no beta/rc tag to promote on $branch"
  echo "▶ promotion guard: $promoted_from..HEAD may only touch Cargo.toml files, Cargo.lock and CHANGELOG.md"
  if ! offending="$(bun scripts/release-version.ts diff-guard --from "$promoted_from" 2>&1)"; then
    die "stable must be $promoted_from plus version metadata, but these files changed since it:
$offending
Re-cut a beta/rc that includes them and let it soak, or revert them from $branch."
  fi
fi

echo "▶ release $current → $next ($tag, channel: $channel) on $target$([[ $dry == 1 ]] && echo ' [dry-run]')"
[[ -z "$promoted_from" ]] || echo "  promoted from $promoted_from"
[[ -z "$notes" ]] || echo "  note for the release notes: ${notes//$'\n'/; }"

if [[ $checks == 1 ]]; then
  run scripts/ci-passed.sh codeatlasdev/atlas "$(git rev-parse HEAD)"
fi

if [[ -n "$create_branch" ]]; then
  git ls-remote --exit-code --heads origin "$create_branch" >/dev/null 2>&1 && die "$create_branch already exists on origin"
  run git switch -c "$create_branch"
fi

# A forced soak is written into the release notes (CHANGELOG section) before the commit, so it ships with the tag.
if [[ -n "$notes" ]]; then
  echo "+ add to the '## [$next]' section of CHANGELOG.md: $notes"
  if [[ $dry == 0 ]]; then
    NOTES="$notes" VERSION="$next" bun -e '
      const f = "CHANGELOG.md", head = "## [" + process.env.VERSION + "]";
      const lines = (await Bun.file(f).text()).split("\n");
      const i = lines.findIndex((l) => l.startsWith(head));
      lines.splice(i + 1, 0, "", ...process.env.NOTES.split("\n").map((n) => "> " + n));
      await Bun.write(f, lines.join("\n"));'
  fi
fi

# Bump every crate to the same version (single release version).
for f in "${crates[@]}"; do
  echo "+ bump $f"
  [[ $dry == 1 ]] || sed -i '' "1,/^version = /s/^version = \".*\"/version = \"$next\"/" "$f"
done
run cargo update --workspace --offline --quiet

run git add Cargo.lock CHANGELOG.md "${crates[@]}"
run git commit -m "chore(release): $tag"
tag_msg="Atlas $tag"
[[ -z "$promoted_from" ]] || tag_msg="$tag_msg

Promoted from: $promoted_from"
run git tag -a "$tag" -m "$tag_msg"
since="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
run git push --atomic origin "$target" "$tag"
watch_release "$tag" "$since"
deploy_site "$tag"
