#!/usr/bin/env bash
# ci-apt.sh — install apt packages on a CI runner: bounded in time, from a cache when there is one, from a fast mirror
# when not (#604). Usage: scripts/ci-apt.sh <cache-dir> <package>...   (the cache itself is the action
# .github/actions/apt-packages)
#
# Incident (#604): a slow mirror (minutes per package) kept `apt-get install` busy for 14 min and then 25+ min in two jobs
# until the job's budget ended, before any test ran. The image's own mirror list picked us-east-2.ec2.archive.ubuntu.com,
# which gave 0.1-2 MB/s (~7 kB/s at its worst) while azure.archive.ubuntu.com, in the runner's own network, gave 30-85 MB/s
# (probe on three ubuntu-latest runners, 2026-11-19). So:
# - <cache-dir> holds the .deb files the install needed (what the runner image did not have) and `packages.txt`. When it
#   is there (restored by actions/cache, keyed by runner image version and package list) they are installed with dpkg and
#   no mirror is asked.
# - otherwise: apt-get update and the download from azure.archive.ubuntu.com, and when that fails or runs out of time the
#   same from the image's own sources; then the install from what was downloaded. Every call has a time limit, so a stalled
#   mirror fails in minutes, not at the job's limit; the downloaded files are copied into <cache-dir> for the action to save.
# - when both rounds ran out of time the step ends with an annotation that scripts/ci-lost-runner.ts reads (the job is
#   rerun, kind `mirror`). A command that fails by itself (an unknown package) is a real failure and says nothing of a mirror.
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
cached=("$dir"/*.deb)
if [[ -f "$dir/packages.txt" ]]; then
  if ((${#cached[@]} == 0)) || ${sudo_cmd[@]+"${sudo_cmd[@]}"} dpkg -i "${cached[@]}"; then
    log "${pkgs[*]} installed from the cache (${#cached[@]} files, no mirror)"
    exit 0
  fi
  log "the cached packages did not install; using a mirror"
  rm -f "$dir/packages.txt" "${cached[@]}"
fi

# The mirrors to try, in order; "" is the image's own sources. The fast one needs Ubuntu on amd64 (arm64 is on ports.ubuntu.com).
codename=""
# shellcheck source=/dev/null
if [[ -r "$os_release" ]]; then codename="$(. "$os_release" && echo "${VERSION_CODENAME:-}")"; fi
rounds=("" "")
if [[ -n "$fast_mirror" && -n "$codename" && "$arch" == x86_64 ]]; then rounds=("$fast_mirror" ""); fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
timeouts=0 last=0 src_opts=()
# step <seconds> <what> <command>...: bounded; the status is returned, a timeout counted.
step() {
  local secs=$1 what=$2
  shift 2
  last=0
  bounded "$secs" "$@" || last=$?
  if [[ $last == 143 ]]; then timeouts=$((timeouts + 1)); fi
  if [[ $last != 0 ]]; then log "$what failed or took longer than ${secs}s${round_mirror:+ ($round_mirror)}"; fi
  return "$last"
}

# round <mirror>: the package lists and the download from one mirror, into src_opts what the install needs to see the same lists.
round() {
  round_mirror=$1
  src_opts=()
  if [[ -n "$round_mirror" ]]; then
    mkdir -p "$tmp/sources"
    printf 'Types: deb\nURIs: %s\nSuites: %s %s-updates %s-backports %s-security\nComponents: main universe multiverse restricted\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n' \
      "$round_mirror" "$codename" "$codename" "$codename" "$codename" >"$tmp/sources/mirror.sources"
    src_opts=(-o Dir::Etc::SourceList=/dev/null -o "Dir::Etc::SourceParts=$tmp/sources")
  fi
  step "$update_secs" "apt-get update" ${sudo_cmd[@]+"${sudo_cmd[@]}"} apt-get update -q "${apt_opts[@]}" ${src_opts[@]+"${src_opts[@]}"} &&
    step "$download_secs" "the download" ${sudo_cmd[@]+"${sudo_cmd[@]}"} apt-get install -y -q --no-install-recommends --download-only "${apt_opts[@]}" ${src_opts[@]+"${src_opts[@]}"} "${pkgs[@]}"
}

mkdir -p "$dir"
had=" $(printf '%s ' "$archives"/*.deb) "
downloaded=0
for m in "${rounds[@]}"; do
  if round "$m"; then
    downloaded=1
    break
  fi
done
if [[ $downloaded == 0 ]]; then
  if [[ $last == 143 && $timeouts -ge 2 ]]; then echo "::error::ci-apt: the apt mirror is too slow or down (two rounds ran out of time); rerun the job"; fi
  exit 1
fi

# What the download brought: apt keeps the mirror's file times, so the files that were not there before, not the newest.
fresh=()
for f in "$archives"/*.deb; do
  case "$had" in *" $f "*) ;; *) fresh+=("$f") ;; esac
done
step "$install_secs" "the install" ${sudo_cmd[@]+"${sudo_cmd[@]}"} apt-get install -y -q --no-install-recommends "${apt_opts[@]}" ${src_opts[@]+"${src_opts[@]}"} "${pkgs[@]}"
((${#fresh[@]} == 0)) || cp "${fresh[@]}" "$dir"/
printf '%s\n' "${pkgs[@]}" >"$dir/packages.txt"
log "${pkgs[*]} installed from ${round_mirror:-the image sources}; ${#fresh[@]} files cached"
