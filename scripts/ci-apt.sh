#!/usr/bin/env bash
# ci-apt.sh — install apt packages on a CI runner: bounded in time, from a cache when there is one, from a fast mirror
# when not (#604). Usage: scripts/ci-apt.sh <cache-dir> <package>...   (the cache itself is the action
# .github/actions/apt-packages)
#
# Incident (#604): a slow mirror (minutes per package) kept `apt-get install` busy for 14 min and then 25+ min in two jobs
# until the job's budget ended, before any test ran. The image's own mirror list picked us-east-2.ec2.archive.ubuntu.com,
# which gave 0.1-2 MB/s (~7 kB/s at its worst) while azure.archive.ubuntu.com, in the runner's own network, gave 30-85 MB/s
# (probe on three ubuntu-latest runners, 2026-11-19). So:
# - <cache-dir> holds the .deb files the install needed (what the runner image did not have), `packages.txt` and
#   `image.txt` (the runner image version they were saved on). Restored by actions/cache, keyed by OS, arch and package list,
#   with the image version only in the key it is saved under, so a new runner image still restores the last cache:
#   - same image version: they are installed with dpkg and no mirror is asked;
#   - older image: they are put in apt's archive directory and the normal path below runs, so apt downloads only what is
#     missing or newer and reuses the rest. The files that are still the installed versions are cached again.
# - otherwise: apt-get update and the download from azure.archive.ubuntu.com, and when that fails or runs out of time the
#   same from the image's own sources; then the install from what was downloaded. Every call has a time limit, so a stalled
#   mirror fails in minutes, not at the job's limit; the downloaded files are copied into <cache-dir> for the action to save.
# - a network failure of both rounds (a connect or DNS error, a failed index or file fetch, a timeout: apt exits 100 for
#   most, not only 143) ends with an annotation that scripts/ci-lost-runner.ts reads (the job is rerun, kind `mirror`). A
#   failure that is not the network (an unknown package, a conflict, a dpkg error) is a real failure: it stops at once, with
#   no annotation and no second round, so the merge queue does not call a code problem an outage (run 44472459923).
set -euo pipefail

dir="${1:?usage: ci-apt.sh <cache-dir> <package>...}"
shift
[[ $# -gt 0 ]] || { echo "usage: ci-apt.sh <cache-dir> <package>..." >&2; exit 2; }
pkgs=("$@")

archives="${CI_APT_ARCHIVES:-/var/cache/apt/archives}"
update_secs="${CI_APT_UPDATE_SECS:-45}"
download_secs="${CI_APT_DOWNLOAD_SECS:-150}"
install_secs="${CI_APT_INSTALL_SECS:-120}"
fast_mirror="${CI_APT_MIRROR-http://azure.archive.ubuntu.com/ubuntu/}" # empty: only the image's own sources
os_release="${CI_APT_OS_RELEASE:-/etc/os-release}"
arch="${CI_APT_ARCH:-$(uname -m)}"

sudo_cmd=()
if command -v sudo >/dev/null; then sudo_cmd=(sudo); fi
# `Timeout` ends a connection that sends nothing for that long; the limits above end one that only crawls.
apt_opts=(-o Acquire::Retries=2 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20 -o DPkg::Lock::Timeout=60)
log() { echo "ci-apt: $*"; }

# An empty array is "unbound" under `set -u` in bash < 4.4 (macOS): `${a[@]+"${a[@]}"}` is the form that works everywhere.
# bounded <seconds> <command>...: run it; past the limit it is sent SIGTERM and the status is 143.
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

shopt -s nullglob
image="${CI_APT_IMAGE:-${ImageVersion:-}}"
cached=("$dir"/*.deb)
if [[ -f "$dir/packages.txt" && -f "$dir/image.txt" && "$(cat "$dir/image.txt")" == "$image" && -n "$image" ]]; then
  if ((${#cached[@]} == 0)) || ${sudo_cmd[@]+"${sudo_cmd[@]}"} dpkg -i "${cached[@]}"; then
    log "${pkgs[*]} installed from the cache (${#cached[@]} files, no mirror)"
    exit 0
  fi
  log "the cached packages did not install; using a mirror"
  rm -f "$dir/packages.txt" "$dir/image.txt" "${cached[@]}"
  cached=()
fi

# The mirrors to try, in order; "" is the image's own sources. The fast one needs Ubuntu on amd64 (arm64 is on ports.ubuntu.com).
codename=""
# shellcheck source=/dev/null
if [[ -r "$os_release" ]]; then codename="$(. "$os_release" && echo "${VERSION_CODENAME:-}")"; fi
rounds=("" "")
if [[ -n "$fast_mirror" && -n "$codename" && "$arch" == x86_64 ]]; then rounds=("$fast_mirror" ""); fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
src_opts=() net_failures=0 round_net=0 round_mirror=""
# What a failure of the network says: apt's own words for a refused or timed out connection, a name that does not
# resolve, an index or a file that could not be fetched, a mirror that answered with an error or garbage.
NETWORK='Could not connect|Could not resolve|Temporary failure resolving|Connection (timed out|failed|refused|reset)|Failed to fetch|Unable to connect|Cannot initiate the connection|Hash Sum mismatch|Bad header line|Undetermined Error|Some index files failed|Err:[0-9]+ https?://|Network is unreachable|Operation timed out|Timeout was reached|Bad Gateway|Service Unavailable'
# step <seconds> <what> <command>...: bounded, output shown and kept; the status is returned. A timeout or a failure whose
# output is the network's counts as a network failure of this round; any other failure is the caller's to stop on.
step() {
  local secs=$1 what=$2 out status kind
  shift 2
  out="$tmp/out.$$.$RANDOM"
  set +e
  bounded "$secs" "$@" 2>&1 | tee "$out"
  status=${PIPESTATUS[0]}
  set -e
  step_net=0
  if [[ $status == 143 ]] || { [[ $status != 0 ]] && grep -Eq "$NETWORK" "$out"; }; then step_net=1; fi
  # An update that did not fail but could not reach a source leaves lists apt cannot resolve against: remember it, so the
  # "Unable to locate package" that follows is read as the network's doing.
  if [[ $what == "apt-get update" ]] && grep -Eq "$NETWORK" "$out"; then round_net=1; fi
  if [[ $status != 0 && $step_net == 0 && $round_net == 1 ]] && grep -q 'Unable to locate package' "$out"; then step_net=1; fi
  if [[ $status != 0 ]]; then
    kind="not the network"
    if [[ $step_net == 1 ]]; then kind="the network"; fi
    log "$what failed (exit $status, $kind)${round_mirror:+ ($round_mirror)}"
  fi
  return "$status"
}

real_failure() { exit "${1:-1}"; }
network_gave_up() {
  echo "::error::ci-apt: the apt mirror is too slow or down (the network failed on every round: ${1}); rerun the job"
  exit 1
}

# round <mirror>: the package lists and the download from one mirror, into src_opts what the install needs to see the same lists.
# Returns 0 when the files are in apt's archives, 1 on a network failure (the next round may do better); a failure that is
# not the network ends the script here.
round() {
  round_mirror=$1
  round_net=0
  src_opts=()
  if [[ -n "$round_mirror" ]]; then
    mkdir -p "$tmp/sources"
    printf 'Types: deb\nURIs: %s\nSuites: %s %s-updates %s-backports %s-security\nComponents: main universe multiverse restricted\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n' \
      "$round_mirror" "$codename" "$codename" "$codename" "$codename" >"$tmp/sources/mirror.sources"
    src_opts=(-o Dir::Etc::SourceList=/dev/null -o "Dir::Etc::SourceParts=$tmp/sources")
  fi
  local st=0
  step "$update_secs" "apt-get update" ${sudo_cmd[@]+"${sudo_cmd[@]}"} apt-get update -q "${apt_opts[@]}" ${src_opts[@]+"${src_opts[@]}"} || st=$?
  if [[ $st != 0 ]]; then
    if [[ $step_net == 1 ]]; then return 1; fi
    real_failure "$st"
  fi
  step "$download_secs" "the download" ${sudo_cmd[@]+"${sudo_cmd[@]}"} apt-get install -y -q --no-install-recommends --download-only "${apt_opts[@]}" ${src_opts[@]+"${src_opts[@]}"} "${pkgs[@]}" || st=$?
  if [[ $st != 0 ]]; then
    if [[ $step_net == 1 ]]; then return 1; fi
    real_failure "$st"
  fi
  return 0
}

mkdir -p "$dir"
had=" $(printf '%s ' "$archives"/*.deb) "
# An older image's cache: its files are what apt may reuse, so put them where apt looks.
if ((${#cached[@]} > 0)); then
  ${sudo_cmd[@]+"${sudo_cmd[@]}"} cp "${cached[@]}" "$archives"/ || log "could not seed apt's archives from the cache; downloading everything"
  log "${#cached[@]} cached files from an older runner image put in apt's archives"
fi
downloaded=0
for m in "${rounds[@]}"; do
  if round "$m"; then
    downloaded=1
    break
  fi
  net_failures=$((net_failures + 1))
done
if [[ $downloaded == 0 ]]; then network_gave_up "$net_failures rounds"; fi

st=0
step "$install_secs" "the install" ${sudo_cmd[@]+"${sudo_cmd[@]}"} apt-get install -y -q --no-install-recommends "${apt_opts[@]}" ${src_opts[@]+"${src_opts[@]}"} "${pkgs[@]}" || st=$?
if [[ $st != 0 ]]; then
  if [[ $step_net == 1 ]]; then network_gave_up "the install"; fi
  real_failure "$st"
fi

# The cache is what the image does not have and apt put in its archives (downloaded now, or seeded from an older image) that
# is still the installed version: a file a newer one replaced is dropped, so the cache does not grow with every image.
rm -f "$dir"/*.deb
kept=0
for f in "$archives"/*.deb; do
  case "$had" in *" $f "*) continue ;; esac
  base="$(basename "$f" .deb)"
  name="${base%%_*}"
  rest="${base#*_}"
  version="${rest%_*}"
  version="${version//%3a/:}"
  installed="$(dpkg-query -W -f='${Version}' "$name" 2>/dev/null || true)"
  if [[ "$installed" == "$version" ]]; then
    cp "$f" "$dir"/
    kept=$((kept + 1))
  fi
done
printf '%s\n' "${pkgs[@]}" >"$dir/packages.txt"
printf '%s\n' "$image" >"$dir/image.txt"
log "${pkgs[*]} installed from ${round_mirror:-the image sources}; $kept files cached"
