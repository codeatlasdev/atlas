#!/usr/bin/env bash
# ci-apt.sh — install apt packages on a CI runner: bounded in time, from a cache when there is one (#604).
# Usage: scripts/ci-apt.sh <cache-dir> <package>...   (the cache itself is the action .github/actions/apt-packages)
#
# Incident (#604): a slow mirror (minutes per package) kept `apt-get install` busy for 14 min and then 25+ min in two jobs
# until the job's budget ended, before any test ran. So:
# - <cache-dir> holds the .deb files the install needed (what the runner image did not have) and `packages.txt`. When it
#   is there (restored by actions/cache, keyed by runner image version and package list) they are installed with dpkg and
#   the mirror is never asked.
# - otherwise: apt-get update, then the download, then the install from what was downloaded. Each step has its own time
#   limit and one retry (apt resumes a partial download), so a stalled mirror fails in minutes, not at the job's limit.
#   The downloaded files are copied into <cache-dir> for the action to save.
set -euo pipefail

dir="${1:?usage: ci-apt.sh <cache-dir> <package>...}"
shift
[[ $# -gt 0 ]] || { echo "usage: ci-apt.sh <cache-dir> <package>..." >&2; exit 2; }

archives="${CI_APT_ARCHIVES:-/var/cache/apt/archives}"
update_secs="${CI_APT_UPDATE_SECS:-45}"
download_secs="${CI_APT_DOWNLOAD_SECS:-150}"
install_secs="${CI_APT_INSTALL_SECS:-120}"

sudo_cmd=()
if command -v sudo >/dev/null; then sudo_cmd=(sudo); fi
# `Timeout` ends a connection that sends nothing for that long; the limits above end one that only crawls.
apt_opts=(-o Acquire::Retries=2 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20 -o DPkg::Lock::Timeout=60)
log() { echo "ci-apt: $*"; }

# bounded <seconds> <command>...: run it; past the limit it is sent SIGTERM and the status is nonzero.
bounded() {
  local secs=$1 pid watcher status=0
  shift
  "$@" &
  pid=$!
  (
    sleep "$secs" &
    sleeper=$!
    trap 'kill "$sleeper" 2>/dev/null; exit 0' TERM
    wait "$sleeper" && kill -TERM "$pid" 2>/dev/null
  ) &
  watcher=$!
  wait "$pid" || status=$?
  kill -TERM "$watcher" 2>/dev/null || true
  wait "$watcher" 2>/dev/null || true
  return "$status"
}

# attempt <seconds> <what> <command>...: bounded, and once more when it fails or runs out of time. When both tries ran out
# of time the step says so in an annotation that scripts/ci-lost-runner.ts reads (kind `mirror`): the job is rerun, not debugged.
# A command that fails by itself (an unknown package) is a real failure and says nothing of the mirror.
attempt() {
  local secs=$1 what=$2 n status stalled=0
  shift 2
  for n in 1 2; do
    status=0
    bounded "$secs" "$@" || status=$?
    if [[ $status == 0 ]]; then return 0; fi
    if [[ $status == 143 ]]; then stalled=$((stalled + 1)); fi # SIGTERM from `bounded`
    log "$what failed or took longer than ${secs}s (attempt $n of 2)"
  done
  if [[ $stalled == 2 ]]; then echo "::error::ci-apt: the apt mirror is too slow or down ($what did not finish in two tries of ${secs}s); rerun the job"; fi
  return 1
}

shopt -s nullglob
cached=("$dir"/*.deb)
if [[ -f "$dir/packages.txt" ]]; then
  if ((${#cached[@]} == 0)) || "${sudo_cmd[@]}" dpkg -i "${cached[@]}"; then
    log "$* installed from the cache (${#cached[@]} files, no mirror)"
    exit 0
  fi
  log "the cached packages did not install; using the mirror"
  rm -f "$dir/packages.txt" "${cached[@]}"
fi

mkdir -p "$dir"
had=" $(printf '%s ' "$archives"/*.deb) "
attempt "$update_secs" "apt-get update" "${sudo_cmd[@]}" apt-get update -q "${apt_opts[@]}"
attempt "$download_secs" "the download" "${sudo_cmd[@]}" apt-get install -y -q --no-install-recommends --download-only "${apt_opts[@]}" "$@"
# What this download brought: apt keeps the mirror's file times, so the files that were not there before, not the newest.
fresh=()
for f in "$archives"/*.deb; do
  case "$had" in *" $f "*) ;; *) fresh+=("$f") ;; esac
done
attempt "$install_secs" "the install" "${sudo_cmd[@]}" apt-get install -y -q --no-install-recommends "${apt_opts[@]}" "$@"
((${#fresh[@]} == 0)) || cp "${fresh[@]}" "$dir"/
printf '%s\n' "$@" >"$dir/packages.txt"
log "$* installed from the mirror; ${#fresh[@]} files cached"
