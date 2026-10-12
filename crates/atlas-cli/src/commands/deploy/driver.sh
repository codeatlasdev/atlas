#!/usr/bin/env bash
# atlas deploy driver — runs ON the host, detached from ssh. Owns the whole
# transaction: lock → seal → install → migrate → swap → restart → health →
# rollback. Generated values live in plan.sh next to this file.
# Exit: 0 ok/noop · 1 failed, current restored/unchanged · 2 critical · 3 locked.
# shellcheck disable=SC2046  # compose_args is intentionally word-split
set -Eeuo pipefail

RUN_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=/dev/null
. "$RUN_DIR/plan.sh"

RUN_ID=$(basename "$RUN_DIR")
REL_ROOT="$DIR/releases"
ATLAS="$DIR/.atlas"
STARTED=$(date -u +%FT%TZ)
STARTED_S=$SECONDS
PHASE=init
PREVIOUS=""
HEARTBEAT=""
OS=$(uname -s)
[ "$MODE" = rollback ] && RELEASE=""

log() { printf '%s %s\n' "$(date -u +%FT%TZ)" "$*"; }

finish() {
  local rc=$?
  set +e
  [ -n "$HEARTBEAT" ] && kill "$HEARTBEAT" 2>/dev/null
  [ -n "$PAUSE_MARKER" ] && [ -n "$HEARTBEAT" ] && rm -f "$PAUSE_MARKER"
  printf '%s\n' "$rc" >"$RUN_DIR/exit.tmp" && mv -f "$RUN_DIR/exit.tmp" "$RUN_DIR/exit"
}
trap finish EXIT

jstr() {
  local s=$1
  s=${s//\\/\\\\}; s=${s//\"/\\\"}; s=${s//$'\n'/\\n}; s=${s//$'\r'/}; s=${s//$'\t'/ }
  printf '"%s"' "$s"
}

# Read a string/number field from one of our single-line JSON files.
jget() { # file key
  [ -f "$1" ] || return 0
  sed -n "s/.*\"$2\" *: *\"\{0,1\}\([^\",}]*\).*/\1/p" "$1" | head -n1
}

state() { # phase [reason]
  PHASE=$1
  printf '{"release":%s,"previous":%s,"phase":%s,"pid":%s,"ts":%s,"reason":%s,"run":%s}\n' \
    "$(jstr "$RELEASE")" "$(jstr "$PREVIOUS")" "$(jstr "$1")" "$$" \
    "$(jstr "$(date -u +%FT%TZ)")" "$(jstr "${2:-}")" "$(jstr "$RUN_ID")" >"$ATLAS/state.json.tmp"
  mv -f "$ATLAS/state.json.tmp" "$ATLAS/state.json"
  log "phase: $1${2:+ ($2)}"
}

meta() { # id status
  local f="$REL_ROOT/$1/.atlas-release" sha ts user
  [ -d "$REL_ROOT/$1" ] || return 0
  sha=$(jget "$f" sha); ts=$(jget "$f" ts); user=$(jget "$f" user)
  printf '{"id":%s,"sha":%s,"ts":%s,"user":%s,"status":%s}\n' "$(jstr "$1")" \
    "$(jstr "${sha:-$SHA}")" "$(jstr "${ts:-$(date -u +%FT%TZ)}")" \
    "$(jstr "${user:-$DEPLOY_USER}")" "$(jstr "$2")" >"$f.tmp"
  mv -f "$f.tmp" "$f"
}

status_of() { jget "$REL_ROOT/$1/.atlas-release" status; }

release_ids() { # newest first; dot-prefixed partials are skipped by the glob
  local d
  for d in "$REL_ROOT"/*/; do [ -d "$d" ] && basename "$d"; done | sort -r
}

current_id() { [ -L "$DIR/current" ] && basename "$(readlink "$DIR/current")" || true; }

last_healthy() { # [exclude]
  local id
  for id in $(release_ids); do
    [ "$id" = "${1:-}" ] && continue
    [ "$(status_of "$id")" = healthy ] && { echo "$id"; return 0; }
  done
  return 0
}

swap_to() {
  local tmp="$DIR/current.tmp.$1"
  if [ "$OS" = Darwin ]; then
    ln -sfhn "releases/$1" "$DIR/current"
  else
    ln -sfn "releases/$1" "$tmp" && mv -Tf "$tmp" "$DIR/current" && sync -f "$DIR"
  fi
  log "current -> releases/$1"
}

export_env() {
  export ATLAS_RELEASE="$RELEASE" ATLAS_RELEASE_DIR="$REL_ROOT/$RELEASE" ATLAS_PREVIOUS="$PREVIOUS" \
    ATLAS_SHA="$SHA" ATLAS_TARGET="$TARGET" ATLAS_APP="$APP" ATLAS_DIR="$DIR"
}

run_in() { # cwd cmd — fd 9 (the lock) is never leaked into user commands
  export_env
  (cd "$1" && bash -c "$2") 9>&-
}

hook_warn() { # name cmd cwd
  [ -z "$2" ] && return 0
  log "hook $1"
  run_in "$3" "$2" || log "WARNING: hook $1 failed (exit $?)"
}

compose_args() { echo "-p $APP -f $DIR/current/${SVC_COMPOSE[$1]:-compose.yml}"; }

restart_svc() {
  local n=${SVC_NAME[$1]}
  log "restart $n (${SVC_KIND[$1]})"
  case ${SVC_KIND[$1]} in
    systemd) { systemctl reset-failed "$n" || true; systemctl restart "$n"; } 9>&- ;;
    compose) docker compose $(compose_args "$1") up -d --no-deps --build --remove-orphans "$n" 9>&- ;;
    command) run_in "$DIR/current" "${SVC_RESTART[$1]}" ;;
  esac
}

probe() {
  local n=${SVC_NAME[$1]} url=${SVC_HEALTH[$1]}
  if [ -n "$url" ]; then
    curl -fsS -o /dev/null --max-time 5 "$url" 2>/dev/null
    return
  fi
  case ${SVC_KIND[$1]} in
    systemd) systemctl is-active --quiet "$n" ;;
    compose) [ -n "$(docker compose $(compose_args "$1") ps -q --status running "$n" 2>/dev/null)" ] ;;
    command) true ;;
  esac
}

fingerprint() {
  local n=${SVC_NAME[$1]} ids
  case ${SVC_KIND[$1]} in
    systemd) systemctl show -p NRestarts -p ActiveEnterTimestampMonotonic --value "$n" 2>/dev/null | tr '\n' ' ' ;;
    compose)
      ids=$(docker compose $(compose_args "$1") ps -q "$n" 2>/dev/null)
      # shellcheck disable=SC2086
      [ -n "$ids" ] && docker inspect -f '{{.RestartCount}} {{.State.StartedAt}}' $ids 2>/dev/null | tr '\n' ' ' ;;
    command) : ;;
  esac
}

FAIL=""
gate_svc() {
  local n=${SVC_NAME[$1]} passes=0 deadline=$((SECONDS + HEALTH_TIMEOUT)) fp
  while [ "$passes" -lt "$HEALTH_PASSES" ]; do
    if probe "$1"; then passes=$((passes + 1)); else passes=0; fi
    [ "$passes" -ge "$HEALTH_PASSES" ] && break
    if [ "$SECONDS" -ge "$deadline" ]; then FAIL="health timeout: $n"; log "$FAIL"; return 1; fi
    sleep 2
  done
  fp=$(fingerprint "$1" || true)
  sleep "$SOAK"
  if [ "$(fingerprint "$1" || true)" != "$fp" ]; then FAIL="crash loop: $n"; log "$FAIL"; return 1; fi
  if ! probe "$1"; then FAIL="health failed after soak: $n"; log "$FAIL"; return 1; fi
  log "healthy: $n"
}

restart_all() { # stop at first failure (deploy) or keep going (restore) with $1=all
  local i ok=0
  for i in "${!SVC_NAME[@]}"; do
    if ! restart_svc "$i"; then FAIL="restart failed: ${SVC_NAME[$i]}"; log "$FAIL"; ok=1
    elif ! gate_svc "$i"; then ok=1
    else continue; fi
    [ "${1:-}" = all ] || return 1
  done
  return "$ok"
}

audit() { # result reason
  printf '{"release":%s,"sha":%s,"user":%s,"mode":%s,"result":%s,"started":%s,"finished":%s,"duration_s":%s,"reason":%s}\n' \
    "$(jstr "$RELEASE")" "$(jstr "$SHA")" "$(jstr "$DEPLOY_USER")" "$(jstr "$MODE")" "$(jstr "$1")" \
    "$(jstr "$STARTED")" "$(jstr "$(date -u +%FT%TZ)")" "$((SECONDS - STARTED_S))" "$(jstr "${2:-}")" \
    >>"$ATLAS/deploys.jsonl"
}

on_failure() { # reason
  [ -z "$HOOK_ON_FAILURE" ] && return 0
  ATLAS_DEPLOY_REASON="$1" hook_warn on_failure "$HOOK_ON_FAILURE" "$DIR"
}

# Abort before the swap: current never changed.
abort() { # reason
  trap - ERR
  set +e
  if [ -n "$RELEASE" ] && [ "$(current_id)" != "$RELEASE" ]; then
    rm -rf "${REL_ROOT:?}/$RELEASE" "$REL_ROOT/.$RELEASE.partial"
  fi
  state failed "$1"
  on_failure "$1"
  audit failed "$1"
  exit 1
}

# Failure after the swap: put the last healthy release back.
restore() { # reason
  trap - ERR
  set +e
  local reason=$1 prev rc
  state rolling_back "$reason"
  meta "$RELEASE" failed
  prev=$(last_healthy "$RELEASE")
  if [ -z "$prev" ]; then
    reason="$reason; no healthy release to restore"
    state failed "$reason"; rc=2
  else
    swap_to "$prev"
    [ -n "$HOOK_POST_SWAP" ] && hook_warn post_swap "$HOOK_POST_SWAP" "$DIR/current"
    if restart_all all; then
      state rolled_back "$reason"; rc=1
    else
      reason="$reason; rollback unhealthy"
      state failed "$reason"; rc=2
    fi
  fi
  on_failure "$reason"
  audit "$([ $rc = 1 ] && echo rolled_back || echo failed)" "$reason"
  exit "$rc"
}

on_err() {
  local msg="command failed at line $1 (phase $PHASE)"
  case $PHASE in
    swapped | restarting | post_swap) restore "$msg" ;;
    *) abort "$msg" ;;
  esac
}

# ── 1. lock ───────────────────────────────────────────────────────────────
mkdir -p "$ATLAS" "$REL_ROOT"
exec 9>"$ATLAS/lock"
if [ "${LOCK_WAIT:-0}" -gt 0 ]; then lockargs=(-w "$LOCK_WAIT"); else lockargs=(-n); fi
if ! flock "${lockargs[@]}" 9; then
  echo "LOCKED $(cat "$ATLAS/lock.owner" 2>/dev/null || echo unknown)"
  exit 3
fi
printf '%s %s %s %s\n' "$DEPLOY_USER" "$$" "$RUN_ID" "$(date -u +%FT%TZ)" >"$ATLAS/lock.owner"
trap 'on_err $LINENO' ERR
log "atlas deploy $MODE · app $APP · target $TARGET · run $RUN_ID"

if [ -n "$PAUSE_MARKER" ]; then
  mkdir -p "$(dirname "$PAUSE_MARKER")"
  (exec 9>&-; while :; do touch "$PAUSE_MARKER"; sleep 30; done) &
  HEARTBEAT=$!
fi

# ── 2. recovery of an interrupted transaction ─────────────────────────────
old_phase=$(jget "$ATLAS/state.json" phase)
case $old_phase in
  swapped | restarting | post_swap | rolling_back)
    old_rel=$(jget "$ATLAS/state.json" release)
    good=$(last_healthy "$old_rel")
    if [ "$(current_id)" != "$good" ]; then
      trap - ERR
      log "recovering interrupted deploy $old_rel (phase $old_phase)"
      [ -n "$old_rel" ] && [ "$old_rel" != "$good" ] && meta "$old_rel" failed
      if [ -z "$good" ]; then state failed "recovery failed: no healthy release"; exit 2; fi
      PREVIOUS=$(current_id)
      swap_to "$good"
      SAVE=$RELEASE; RELEASE=$good
      state restarting "recovery"
      [ -n "$HOOK_POST_SWAP" ] && hook_warn post_swap "$HOOK_POST_SWAP" "$DIR/current"
      if ! restart_all all; then state failed "recovery failed: $FAIL"; exit 2; fi
      state healthy "recovered from interrupted deploy $old_rel"
      RELEASE=$SAVE; PREVIOUS=""
      trap 'on_err $LINENO' ERR
    fi ;;
esac

# ── 3a. rollback ──────────────────────────────────────────────────────────
if [ "$MODE" = rollback ]; then
  trap - ERR
  PREVIOUS=$(current_id)
  rb=${ROLLBACK_TO:-$(last_healthy "$PREVIOUS")}
  if [ -z "$rb" ]; then state failed "no healthy release to roll back to"; exit 1; fi
  if [ ! -d "$REL_ROOT/$rb" ]; then state failed "release $rb not found"; exit 1; fi
  if [ "$(status_of "$rb")" = failed ]; then state failed "release $rb is marked failed"; exit 1; fi
  RELEASE=$rb
  SHA=$(jget "$REL_ROOT/$rb/.atlas-release" sha)
  swap_to "$rb"
  state post_swap
  [ -n "$HOOK_POST_SWAP" ] && hook_warn post_swap "$HOOK_POST_SWAP" "$DIR/current"
  state restarting
  if ! restart_all all; then
    state failed "rollback unhealthy: $FAIL"; on_failure "rollback unhealthy: $FAIL"
    audit failed "$FAIL"; exit 2
  fi
  meta "$rb" healthy
  state rolled_back "manual rollback from ${PREVIOUS:-none}"
  audit rolled_back ""
  exit 0
fi

# ── 3b. deploy ────────────────────────────────────────────────────────────
PARTIAL="$REL_ROOT/.$RELEASE.partial"
RELEASE_DIR="$REL_ROOT/$RELEASE"
state preflight

free=$(df -Pm "$DIR" | awk 'NR==2 {print $4}')
[ "${free:-0}" -ge "$MIN_FREE_MB" ] || abort "low disk: ${free}MB free < ${MIN_FREE_MB}MB"
need=(curl tar flock sha256sum)
for k in "${SVC_KIND[@]}"; do
  case $k in systemd) need+=(systemctl) ;; compose) need+=(docker) ;; esac
done
for b in "${need[@]}"; do command -v "$b" >/dev/null 2>&1 || abort "missing $b"; done

cur=$(current_id)
if [ -n "$cur" ] && [ -z "$FORCE" ] && [ "$(jget "$REL_ROOT/$cur/.atlas-release" sha)" = "$SHA" ] \
  && [ "$(status_of "$cur")" = healthy ]; then
  rm -rf "$PARTIAL"
  RELEASE=$cur
  state noop "$SHA already current and healthy"
  audit noop ""
  exit 0
fi

state uploaded
[ -f "$PARTIAL/artifact.tar.gz" ] || abort "artifact missing: $PARTIAL/artifact.tar.gz"
[ "$(sha256sum "$PARTIAL/artifact.tar.gz" | awk '{print $1}')" = "$SHA256" ] || abort "checksum mismatch"
[ -e "$RELEASE_DIR" ] && { RELEASE=""; abort "release exists: $RELEASE_DIR"; }
tar xzf "$PARTIAL/artifact.tar.gz" -C "$PARTIAL"
rm -f "$PARTIAL/artifact.tar.gz"
[ "$OS" = Darwin ] || sync -f "$PARTIAL"
mv "$PARTIAL" "$RELEASE_DIR"
meta "$RELEASE" sealed
state sealed

for i in "${!LINK_SRC[@]}"; do
  src=${LINK_SRC[$i]} dst=${LINK_DST[$i]}
  [ -e "$src" ] || abort "link source missing: $src"
  mkdir -p "$(dirname "$RELEASE_DIR/$dst")"
  rm -rf "${RELEASE_DIR:?}/$dst"
  ln -sfn "$src" "$RELEASE_DIR/$dst"
done

PREVIOUS=$cur
for step in installing:INSTALL migrating:MIGRATE pre_swap:HOOK_PRE_SWAP; do
  ph=${step%%:*} var=${step#*:}
  [ -z "${!var}" ] && continue
  state "$ph"
  run_in "$RELEASE_DIR" "${!var}" || abort "$ph failed (exit $?)"
done

swap_to "$RELEASE"
state swapped
if [ -n "$HOOK_POST_SWAP" ]; then
  state post_swap
  run_in "$DIR/current" "$HOOK_POST_SWAP" || restore "post_swap failed (exit $?)"
fi

state restarting
restart_all || restore "$FAIL"

meta "$RELEASE" healthy
state healthy
trap - ERR
hook_warn post_deploy "$HOOK_POST_DEPLOY" "$DIR/current"

# prune
set +e
good=$(last_healthy)
n=0
for id in $(release_ids); do
  n=$((n + 1))
  [ "$n" -le "$KEEP" ] && continue
  case $id in "$RELEASE" | "$PREVIOUS" | "$good") continue ;; esac
  log "prune $id"
  rm -rf "${REL_ROOT:?}/$id"
done
find "$REL_ROOT" -maxdepth 1 -name '.*.partial' -mtime +0 -exec rm -rf {} + 2>/dev/null
for r in "$ATLAS"/run/*/; do
  r=$(basename "$r")
  case $r in rb-* | "$RUN_ID") continue ;; esac
  [ -d "$REL_ROOT/$r" ] || [ -d "$REL_ROOT/.$r.partial" ] || rm -rf "${ATLAS:?}/run/$r"
done
find "$ATLAS/run" -maxdepth 1 -name 'rb-*' -mtime +30 -exec rm -rf {} + 2>/dev/null

audit healthy ""
log "deployed $RELEASE"
exit 0
