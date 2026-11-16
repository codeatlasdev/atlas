#!/usr/bin/env bash
# crew-e2e.sh — install a crew server on a REAL systemd Linux and run tasks on it, end to end.
#
# The server is a privileged Ubuntu 24.04 container with systemd as PID 1 and sshd (scripts/crew-e2e/Dockerfile),
# reached over ssh on a non-default port with a key, exactly like a VPS. The client is the `atlas` CLI under test
# plus the hostd protocol over an ssh socket tunnel, the way Atlas Desktop talks to a crew server. The only
# fake is the agent: a `claude` script that prints the engine's JSON result (scripts/crew-e2e/fake-claude.sh),
# so no API key is needed.
#
#   1. atlas crew install (twice: a re-install is a normal operation) -> ready, hostd answers
#   2. a crewmate and a task through hostd -> done; an engine that fails -> failed
#   3. a hanging task: cancel ends it and its process tree (#290); pause ends it, holds the queue, resume runs
#      it again; a hostd restart in the middle of a task resumes it (#199)
#   4. atlas crew export, then uninstall: refuses without --yes, then leaves no unit, user, group or file
#
# Any failed check exits 1. The server's journal, hostd log and state are copied to $CREW_E2E_OUT.
# Needs docker (privileged containers), ssh, jq, python3 and a Linux x86_64/aarch64 `atlas`.
#
#   cargo build -p atlas-cli && scripts/crew-e2e.sh        # ATLAS=<path> to test another binary
#
# CREW_E2E_KEEP=1 leaves the container (name atlas-crew-e2e) running to poke at; CREW_E2E_PORT sets the ssh port.
# CI: .github/workflows/crew-e2e.yml (docs/crew.md, "End-to-end test on real systemd").
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD

ATLAS=${ATLAS:-$ROOT/target/debug/atlas}
PORT=${CREW_E2E_PORT:-2222}
NAME=atlas-crew-e2e
IMAGE=atlas-crew-e2e-server
ADMIN=deploy
TARGET=$ADMIN@127.0.0.1
HOME_DIR=/var/lib/atlas-crew
WORK=$(mktemp -d "${TMPDIR:-/tmp}/crew-e2e.XXXXXX")
OUT=${CREW_E2E_OUT:-$WORK/out}
mkdir -p "$OUT"

say() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
fail() {
  printf '\033[31mFAIL: %s\033[0m\n' "$*" >&2
  exit 1
}

for tool in docker ssh ssh-keygen jq python3; do
  command -v "$tool" >/dev/null || fail "$tool is missing"
done
[[ -x $ATLAS ]] || fail "no atlas at $ATLAS (cargo build -p atlas-cli, or ATLAS=<path>)"
REAL_SSH=$(command -v ssh)
REAL_SCP=$(command -v scp)

# --- the ssh client: a key of its own and a config of its own, never the user's ~/.ssh/config ---
ssh-keygen -q -t ed25519 -N '' -C crew-e2e -f "$WORK/id"
cat >"$WORK/ssh_config" <<EOF
Host 127.0.0.1
  IdentityFile $WORK/id
  IdentitiesOnly yes
  StrictHostKeyChecking no
  UserKnownHostsFile /dev/null
  LogLevel ERROR
EOF
# atlas runs `ssh` and `scp` by name: these shims add `-F` and change nothing else.
mkdir -p "$WORK/bin" ~/.ssh
chmod 700 ~/.ssh
printf '#!/bin/sh\nexec %s -F "%s" "$@"\n' "$REAL_SSH" "$WORK/ssh_config" >"$WORK/bin/ssh"
printf '#!/bin/sh\nexec %s -F "%s" "$@"\n' "$REAL_SCP" "$WORK/ssh_config" >"$WORK/bin/scp"
chmod +x "$WORK/bin/ssh" "$WORK/bin/scp"
export PATH="$WORK/bin:$PATH"
# A GUI askpass or an agent must never answer for the test: the key above is the only way in.
unset SSH_ASKPASS SSH_AUTH_SOCK

TUNNEL_PID=
srv() { docker exec "$NAME" "$@"; }
# A shell line on the server as root.
srv_sh() { docker exec "$NAME" sh -c "$1"; }
atlas() { "$ATLAS" "$@"; }

collect() {
  docker inspect "$NAME" >/dev/null 2>&1 || return 0
  say "collecting the server's logs into $OUT"
  {
    echo "--- systemctl status";   srv systemctl status 'atlas-*' --no-pager -l
    echo "--- failed units";       srv systemctl --failed --no-pager
  } >"$OUT/systemctl.txt" 2>&1 || true
  srv journalctl --no-pager -o short-precise >"$OUT/journal.txt" 2>&1 || true
  srv journalctl --no-pager -o short-precise -u 'atlas-*' >"$OUT/journal-atlas.txt" 2>&1 || true
  srv_sh "cat $HOME_DIR/.atlas/hostd.log" >"$OUT/hostd.log" 2>&1 || true
  srv_sh "ps -eo pid,ppid,user:20,pgid,args --forest" >"$OUT/ps.txt" 2>&1 || true
  srv_sh "id atlas-crew; ls -ldn /run/atlas-crew /run/atlas-crew/* /var/lib/atlas-cm /usr/local/lib/atlas-crew/engines/*; sudo -u atlas-crew stat /run/atlas-crew/spawn.sock" >"$OUT/isolation.txt" 2>&1 || true
  srv_sh "nft list ruleset; cat /etc/atlas-crew/egress.nft" >"$OUT/nft.txt" 2>&1 || true
  srv_sh "tar -C $HOME_DIR/.atlas -cf - crew" 2>/dev/null | tar -C "$OUT" -xf - 2>/dev/null || true
}

finish() {
  rc=$?
  set +e
  [[ -n $TUNNEL_PID ]] && kill "$TUNNEL_PID" 2>/dev/null
  [[ -n ${SPAWN_TUNNEL_PID:-} ]] && kill "$SPAWN_TUNNEL_PID" 2>/dev/null
  collect
  if [[ ${CREW_E2E_KEEP:-} == 1 ]]; then
    echo "container $NAME left running (ssh -p $PORT -F $WORK/ssh_config $TARGET)"
  else
    docker rm -f "$NAME" >/dev/null 2>&1
    rm -rf "$WORK/id" "$WORK/id.pub"
  fi
  [[ $rc == 0 ]] && echo "crew-e2e: every check passed" || echo "crew-e2e: FAILED (exit $rc), logs in $OUT" >&2
  exit "$rc"
}
trap finish EXIT

# `until_true SECONDS WHAT cmd…`: run cmd every second until it succeeds.
until_true() {
  local secs=$1 end=$((SECONDS + $1)) what=$2
  shift 2
  while ((SECONDS < end)); do
    "$@" >/dev/null 2>&1 && return 0
    sleep 1
  done
  fail "timed out after ${secs}s waiting for: $what"
}

# ── the server ────────────────────────────────────────────────────────────────────────────────────
say "building the server image and booting it (systemd as PID 1)"
docker rm -f "$NAME" >/dev/null 2>&1 || true
docker build -q -t "$IMAGE" scripts/crew-e2e >/dev/null
docker run -d --name "$NAME" --privileged --cgroupns=private --tmpfs /run --tmpfs /run/lock --tmpfs /tmp \
  -p "127.0.0.1:$PORT:22" "$IMAGE" >/dev/null
booted() { local s; s=$(srv systemctl is-system-running 2>/dev/null) || true; [[ $s == running || $s == degraded ]]; }
until_true 120 "systemd to finish booting" booted
srv sh -c 'test "$(cat /proc/1/comm)" = systemd' || fail "PID 1 of the server is not systemd"
docker exec -i "$NAME" sh -c "cat > /home/$ADMIN/.ssh/authorized_keys && chown $ADMIN:$ADMIN /home/$ADMIN/.ssh/authorized_keys && chmod 600 /home/$ADMIN/.ssh/authorized_keys" <"$WORK/id.pub"
until_true 60 "sshd to accept the key on port $PORT" ssh -o BatchMode=yes -p "$PORT" "$TARGET" true
echo "server up: $(srv sh -c '. /etc/os-release; echo $PRETTY_NAME, systemd $(systemctl --version | head -n1 | cut -d" " -f2)')"

# ── 1. install ────────────────────────────────────────────────────────────────────────────────────
say "1. atlas crew install $TARGET --port $PORT"
# --binary: the build under test, not the published release (this version may not be published yet).
atlas crew install "$TARGET" --port "$PORT" --yes --binary "$ATLAS" 2>&1 | tee "$OUT/install-1.log"
grep -q 'Crew server ready' "$OUT/install-1.log" || fail "install did not report the crew server ready"
[[ $(srv systemctl is-active atlas-crew) == active ]] || fail "atlas-crew.service is not active"
for u in atlas-crew-spawn.socket atlas-crew-provision.socket; do
  [[ $(srv systemctl is-active "$u") == active ]] || fail "$u (the per-crewmate isolation helper) is not active"
done
srv id atlas-crew >/dev/null || fail "no atlas-crew user"
srv test -x /usr/local/lib/atlas-crew/atlas || fail "no binary in /usr/local/lib/atlas-crew"

say "1b. atlas crew status, and a second install over the first"
atlas crew status "$TARGET" --port "$PORT" 2>&1 | tee "$OUT/status-1.log"
grep -q 'Active: active (running)' "$OUT/status-1.log" || fail "status does not show the service running"
grep -q '(up to date)' "$OUT/status-1.log" || fail "status does not say the server's atlas is up to date"
# A different binary (the same one with a byte appended: it runs the same), so the first one must be kept.
cp "$ATLAS" "$WORK/atlas-second"
printf '\n' >>"$WORK/atlas-second"
atlas crew install "$TARGET" --port "$PORT" --yes --binary "$WORK/atlas-second" 2>&1 | tee "$OUT/install-2.log"
grep -q 'Crew server ready' "$OUT/install-2.log" || fail "a second install did not report the crew server ready"
srv test -x /usr/local/lib/atlas-crew/atlas.prev || fail "the second install did not keep the first binary as atlas.prev"
[[ $(srv stat -c %s /usr/local/lib/atlas-crew/atlas.prev) == "$(stat -c %s "$ATLAS")" ]] || fail "atlas.prev is not the first binary"
[[ $(srv stat -c %s /usr/local/lib/atlas-crew/atlas) == "$(stat -c %s "$WORK/atlas-second")" ]] || fail "atlas is not the second binary"
[[ $(srv systemctl is-active atlas-crew) == active ]] || fail "atlas-crew.service is not active after the second install"

# ── the client: hostd's socket through an ssh tunnel, as the crew user the install made ──────────────
say "2. connecting to hostd as atlas-crew over ssh (the desktop's path)"
TOKEN=$(ssh -o BatchMode=yes -p "$PORT" atlas-crew@127.0.0.1 'cat "$HOME/.atlas/hostd.token"') ||
  fail "cannot ssh as atlas-crew: the install did not leave the caller's key in its authorized_keys"
SOCK=$WORK/hostd.sock
ssh -N -o BatchMode=yes -o ExitOnForwardFailure=yes -o StreamLocalBindUnlink=yes -o ServerAliveInterval=15 \
  -L "$SOCK:$HOME_DIR/.atlas/hostd.sock" -p "$PORT" atlas-crew@127.0.0.1 &
TUNNEL_PID=$!
until_true 20 "the hostd socket tunnel" test -S "$SOCK"
hostd() { python3 "$ROOT/scripts/crew-e2e/hostd-call.py" "$SOCK" "$TOKEN" "$@"; }
until_true 30 "hostd to answer info" hostd info
hostd info | jq -e '.caps | index("crew")' >/dev/null || fail "hostd does not advertise the crew capability"
echo "hostd answers: $(hostd info | jq -c '{version, caps: (.caps | length)}')"

# The fake engine goes where the isolation helper looks first (/usr/local/lib/atlas-crew/engines), replacing
# the link the install left to the real claude. `install` would write through that link, so remove it first.
say "2b. installing the fake claude on the server"
srv rm -f /usr/local/lib/atlas-crew/engines/claude
docker exec -i "$NAME" sh -c 'cat > /usr/local/lib/atlas-crew/engines/claude && chmod 0755 /usr/local/lib/atlas-crew/engines/claude' <scripts/crew-e2e/fake-claude.sh

# ── 2. a crewmate and its tasks ───────────────────────────────────────────────────────────────────
task_json() { hostd crewmate_tasks "$(jq -nc --arg id "$1" '{id: $id, limit: 50}')" | jq -c --arg t "$2" '.tasks[] | select(.id == $t)'; }
crewmate_json() { hostd crewmate_list '{}' | jq -c --arg id "$1" '.crewmates[] | select(.id == $id)'; }
send_task() {
  hostd crewmate_task "$(jq -nc --arg id "$1" --arg p "$2" '{id: $id, prompt: $p, source: "desktop"}')" | jq -r .task.id
}
# wait_task CREWMATE TASK STATE SECONDS: a task that ends in another final state is a failure at once.
wait_task() {
  local end=$((SECONDS + $4)) st=
  while ((SECONDS < end)); do
    st=$(task_json "$1" "$2" | jq -r .state)
    [[ $st == "$3" ]] && return 0
    case $st in done | failed | cancelled) fail "task $2 ended $st, not $3: $(task_json "$1" "$2")" ;; esac
    sleep 1
  done
  fail "task $2 never became $3 in $4s (last: ${st:-none}): $(task_json "$1" "$2")"
}
crewmate_idle() { [[ $(crewmate_json "$1" | jq -r .state) == idle ]]; }
wait_idle() { until_true 30 "crewmate $1 to go idle" crewmate_idle "$1"; }
# A process whose command line is exactly `sleep N`: what the fake engine's "tool" runs. No `grep -q`: with
# pipefail an early exit would turn into a SIGPIPE failure of `ps`.
has_sleeper() { srv ps -eo args | grep -x "sleep $1" >/dev/null; }
no_sleeper() { ! has_sleeper "$1"; }
owner_of_sleeper() { srv ps -eo user:30,args | grep -E " sleep $1\$" | head -n1 | awk '{print $1}'; }

say "3. a crewmate, a task, its log"
CM=$(hostd crewmate_create '{"name":"Echo","role":"You are the crew-e2e crewmate.","agent":"claude","approval":"auto","engine":{"kind":"claude"}}' | jq -r .id)
[[ -n $CM && $CM != null ]] || fail "crewmate_create returned no id"
echo "crewmate $CM"
# Isolation: the crewmate has a Linux user of its own under /var/lib/atlas-cm.
SLUG=${CM//-/}
SLUG=${SLUG:0:12}
CMUSER=atlas-cm-$SLUG
srv id "$CMUSER" >/dev/null || fail "the crewmate has no Linux user $CMUSER (per-crewmate isolation did not provision it)"
srv test -d "/var/lib/atlas-cm/$SLUG/work" || fail "the crewmate has no work dir under /var/lib/atlas-cm"

T=$(send_task "$CM" "E2E_OK nonce$RANDOM")
wait_task "$CM" "$T" "done" 60
RESULT=$(task_json "$CM" "$T" | jq -r .result)
[[ $RESULT == "fake-claude done nonce"* ]] || fail "the task's result is not what the fake engine printed: $RESULT"
task_json "$CM" "$T" | jq -e '.error == null and .started_at != null and .finished_at != null' >/dev/null ||
  fail "a done task has an error or no timestamps: $(task_json "$CM" "$T")"
# The log: the server's own record of the task and the audit trail.
srv_sh "grep -q 'fake-claude done' $HOME_DIR/.atlas/crew/$CM/tasks.jsonl" || fail "tasks.jsonl on the server does not hold the result"
hostd crewmate_audit "$(jq -nc --arg id "$CM" '{id: $id}')" | jq -e '.entries | length > 0' >/dev/null || fail "the crewmate's audit log is empty"
echo "done: $RESULT"

say "3b. an engine that fails ends the task failed (and not done)"
T=$(send_task "$CM" "E2E_FAIL")
wait_task "$CM" "$T" failed 60
task_json "$CM" "$T" | jq -e '.error | tostring | length > 0' >/dev/null || fail "a failed task has no error"
echo "failed: $(task_json "$CM" "$T" | jq -r .error | head -n1)"
wait_idle "$CM"

# ── 3. cancel ends the process tree (#290) ────────────────────────────────────────────────────────
say "4. a hanging task: cancel ends it and everything it started (#290)"
T=$(send_task "$CM" "E2E_HANG 31337")
wait_task "$CM" "$T" running 60
until_true 20 "the engine's child process (sleep 31337) to exist" has_sleeper 31337
OWNER=$(owner_of_sleeper 31337)
[[ $OWNER == "$CMUSER" ]] || fail "the engine's child runs as '$OWNER', not as the crewmate's own user $CMUSER"
hostd crewmate_cancel "$(jq -nc --arg id "$CM" --arg t "$T" '{id: $id, task: $t}')" >/dev/null
wait_task "$CM" "$T" cancelled 30
until_true 20 "the cancelled task's process tree to be gone" no_sleeper 31337
wait_idle "$CM"
echo "cancelled; no sleep 31337 left"

# ── pause ─────────────────────────────────────────────────────────────────────────────────────────
say "5. atlas crew pause ends the running task and its tree, holds the queue; resume runs it"
T=$(send_task "$CM" "E2E_HANG 31338")
wait_task "$CM" "$T" running 60
until_true 20 "sleep 31338 to exist" has_sleeper 31338
atlas crew pause "$TARGET" --port "$PORT"
wait_task "$CM" "$T" cancelled 30
[[ $(task_json "$CM" "$T" | jq -r .error) == paused ]] || fail "a paused task's error is not 'paused': $(task_json "$CM" "$T")"
until_true 20 "the paused task's process tree to be gone" no_sleeper 31338
[[ $(hostd crewmate_list '{}' | jq -r .paused) == true ]] || fail "crewmate_list does not say the crew is paused"
T2=$(send_task "$CM" "E2E_OK queued$RANDOM")
sleep 5
[[ $(task_json "$CM" "$T2" | jq -r .state) == queued ]] || fail "a task sent while paused did not stay queued: $(task_json "$CM" "$T2")"
atlas crew resume "$TARGET" --port "$PORT"
wait_task "$CM" "$T2" "done" 60
[[ $(hostd crewmate_list '{}' | jq -r .paused) == false ]] || fail "the crew is still paused after resume"
echo "paused: task cancelled, tree gone, queue held; resumed: queued task done"

# ── restart in the middle of a task (#199) ────────────────────────────────────────────────────────
say "6. restarting hostd in the middle of a task resumes it (#199)"
T=$(send_task "$CM" "E2E_HANG_ONCE 31339")
wait_task "$CM" "$T" running 60
until_true 20 "sleep 31339 to exist" has_sleeper 31339
srv systemctl restart atlas-crew
until_true 60 "hostd to answer again" hostd info
wait_task "$CM" "$T" "done" 90
task_json "$CM" "$T" | jq -e '.resumed == true and .restarts == 1' >/dev/null ||
  fail "the task finished but is not marked resumed once: $(task_json "$CM" "$T")"
[[ $(task_json "$CM" "$T" | jq -r .result) == "fake-claude resumed" ]] || fail "the resumed run did not answer: $(task_json "$CM" "$T")"
# The run the restart cut must not be left running next to its replacement.
until_true 30 "the run the restart interrupted to be gone" no_sleeper 31339
wait_idle "$CM"
echo "resumed after the restart: $(task_json "$CM" "$T" | jq -c '{state, resumed, restarts}')"

# ── isolation: what one crewmate cannot do to the server or to another crewmate ───────────────────
denied() { # denied WHAT cmd…: the command must fail
  local what=$1
  shift
  if "$@" >/dev/null 2>&1; then fail "$what was allowed"; fi
}
as_user() { local u=$1; shift; srv setpriv --reuid="$u" --regid="$u" --clear-groups "$@"; }
mode_of() { srv stat -c '%U:%G %a' "$1"; }

say "8. a second crewmate B: what crewmate A cannot do (isolation)"
CMB=$(hostd crewmate_create '{"name":"Bee","role":"You are the second crew-e2e crewmate.","agent":"claude","approval":"auto","engine":{"kind":"claude"}}' | jq -r .id)
SLUGB=${CMB//-/}
SLUGB=${SLUGB:0:12}
hostd crewmate_secret_set "$(jq -nc --arg id "$CMB" '{id: $id, key: "E2E_SECRET", value: "bee-secret"}')" >/dev/null
TB=$(send_task "$CMB" "E2E_OK bee$RANDOM")
wait_task "$CMB" "$TB" "done" 60
DIRA=/var/lib/atlas-cm/$SLUG
DIRB=/var/lib/atlas-cm/$SLUGB
WORKA=$DIRA/work
AUID=$(srv id -u "$CMUSER")

# (1) a crewmate's directory belongs to root and A cannot change or list it
for d in "$DIRA" "$DIRB"; do
  [[ $(mode_of "$d") == "root:root 711" ]] || fail "$d is $(mode_of "$d"), not root:root 711"
done
denied "A listing its own crewmate directory" as_user "$CMUSER" ls "$DIRA"
denied "A renaming work/ in its directory" as_user "$CMUSER" mv "$WORKA" "$DIRA/work.moved"
denied "A planting a link in its directory" as_user "$CMUSER" ln -s /etc/passwd "$DIRA/memory.md.link"
denied "A listing B's directory" as_user "$CMUSER" ls "$DIRB"
denied "A listing the crewmates' parent" as_user "$CMUSER" ls /var/lib/atlas-cm

# (3) A cannot read B's files or the crew user's, nor reach the helpers
denied "A reading B's memory.md" as_user "$CMUSER" cat "$DIRB/memory.md"
denied "A reading hostd.token" as_user "$CMUSER" cat "$HOME_DIR/.atlas/hostd.token"
denied "A reading B's secrets.json" as_user "$CMUSER" cat "$HOME_DIR/.atlas/crew/$CMB/secrets.json"
srv setpriv --reuid=atlas-crew --regid=atlas-crew --init-groups cat "$HOME_DIR/.atlas/hostd.token" >/dev/null ||
  fail "control: the crew user cannot read hostd.token either, so the checks above prove nothing"
for sock in spawn provision; do
  as_user "$CMUSER" curl -s --unix-socket "/run/atlas-crew/$sock.sock" http://localhost/ >/dev/null 2>&1 && rc=0 || rc=$?
  [[ $rc == 7 ]] || fail "A connecting to $sock.sock: curl exited $rc, not 7 (could not connect)"
  srv setpriv --reuid=atlas-crew --regid=atlas-crew --init-groups curl -s --unix-socket "/run/atlas-crew/$sock.sock" http://localhost/ >/dev/null 2>&1 && rc=0 || rc=$?
  [[ $rc != 7 ]] || fail "control: the crew user cannot connect to $sock.sock either"
done

# (2) a link where the provisioning service would chown or chmod: the task fails, the target is untouched.
# The link is made as root here: it stands for whatever a bug would let a crewmate plant.
link_target_untouched() { # NAME PATH-IN-A'S-DIR TARGET
  local name=$1 rel=$2 target=$3 before after
  before=$(mode_of "$target")
  srv sh -c "mv '$DIRA/$rel' '$DIRA/$rel.real' && ln -s '$target' '$DIRA/$rel'"
  local t
  t=$(send_task "$CM" "E2E_OK link$RANDOM")
  wait_task "$CM" "$t" failed 60
  task_json "$CM" "$t" | jq -r .error | grep -q 'is a symbolic link' ||
    fail "a link at $name did not fail the task with 'is a symbolic link': $(task_json "$CM" "$t" | jq -r .error)"
  after=$(mode_of "$target")
  [[ $before == "$after" ]] || fail "a link at $name changed $target from '$before' to '$after'"
  srv sh -c "rm '$DIRA/$rel' && mv '$DIRA/$rel.real' '$DIRA/$rel'"
  wait_idle "$CM"
  echo "link at $name: task failed, $target still '$after'"
}
link_target_untouched memory.md memory.md /etc/passwd
link_target_untouched home home "$DIRB/home"
link_target_untouched work work /etc/ssh
link_target_untouched crew-runtime crew-runtime /etc/sudoers.d
T=$(send_task "$CM" "E2E_OK recovered$RANDOM")
wait_task "$CM" "$T" "done" 60

# (4)(5) A's git config is A's own code: hostd's and the helper's git run it as A, never as the crew user
as_user "$CMUSER" sh -c "cd '$WORKA' && git init -q . && git -c user.name=a -c user.email=a@example.com commit -q --allow-empty -m init &&
  printf '#!/bin/sh\nid -u >> \"%s/marker\"\ncp \"%s/.atlas/hostd.token\" \"%s/stolen\" 2>/dev/null\nexit 0\n' '$WORKA' '$HOME_DIR' '$WORKA' > '$WORKA/fsm.sh' && chmod +x '$WORKA/fsm.sh' &&
  git config core.fsmonitor '$WORKA/fsm.sh'" || fail "A cannot set up a git checkout of its own in work/"
[[ $(srv stat -c %U "$WORKA/.git") == "$CMUSER" ]] || fail "work/.git is not owned by A"
T=$(send_task "$CM" "E2E_OK fsmonitor$RANDOM")
wait_task "$CM" "$T" "done" 60
# The same through the helper, which is how hostd runs git in A's tree (checked below as (7) for refusals).
ssh -N -o BatchMode=yes -o ExitOnForwardFailure=yes -o StreamLocalBindUnlink=yes -L "$WORK/spawn.sock:/run/atlas-crew/spawn.sock" -p "$PORT" atlas-crew@127.0.0.1 &
SPAWN_TUNNEL_PID=$!
until_true 20 "the helper socket tunnel" test -S "$WORK/spawn.sock"
helper() { python3 "$ROOT/scripts/crew-e2e/helper-call.py" "$WORK/spawn.sock" "$1"; }
git_req() { jq -nc --arg u "$CMUSER" --arg cwd "$2" --argjson args "$1" '{op: "git", user: $u, args: $args, cwd: $cwd, timeout_secs: 20}'; }
r=$(helper "$(git_req '["status"]' "$WORKA")")
[[ $(jq -r .status <<<"$r") != error ]] || fail "the helper refused a plain git status in A's tree: $r"
srv test -s "$WORKA/marker" || fail "no core.fsmonitor line: git never ran the checkout's command, so this check proved nothing"
bad=$(srv cat "$WORKA/marker" | grep -vx "$AUID" || true)
[[ -z $bad ]] || fail "core.fsmonitor ran as another uid than A's ($AUID): $bad"
srv test ! -e "$WORKA/stolen" || fail "core.fsmonitor copied hostd.token: it ran as the crew user"
echo "fsmonitor ran $(srv wc -l <"$WORKA/marker") time(s), always as uid $AUID, hostd.token not copied"
as_user "$CMUSER" sh -c "cd '$WORKA' && echo one > f && git add f && git -c user.name=a -c user.email=a@example.com commit -q -m one && echo two >> f && git -c user.name=a -c user.email=a@example.com commit -q -am two" ||
  fail "A cannot edit a tracked file and commit in its own checkout"

# (7) the helper's git request is narrow
for case in "config|[\"config\",\"core.fsmonitor\",\"x\"]|$WORKA|not allowed" \
  "-c first|[\"-c\",\"core.fsmonitor=x\",\"status\"]|$WORKA|not allowed" \
  "cwd outside A's directory|[\"status\"]|/etc|cwd outside" \
  "cwd in B's directory|[\"status\"]|$DIRB/work|cwd outside"; do
  IFS='|' read -r name args cwd want <<<"$case"
  r=$(helper "$(git_req "$args" "$cwd")")
  [[ $(jq -r .status <<<"$r") == error && $(jq -r .reason <<<"$r") == *"$want"* ]] || fail "the helper did not refuse a git request ($name) with '$want': $r"
done
echo "the helper refuses git config, -c first, and a cwd outside A's directory"
kill "$SPAWN_TUNNEL_PID"
wait "$SPAWN_TUNNEL_PID" 2>/dev/null || true

# (6) isolation installed but its helper gone: tasks are refused, nothing runs as the crew user
srv systemctl stop atlas-crew-spawn.socket atlas-crew-spawn.service
srv rm -f /run/atlas-crew/spawn.sock
srv systemctl restart atlas-crew
until_true 60 "hostd to answer again" hostd info
T=$(send_task "$CM" "E2E_HANG 31340")
wait_task "$CM" "$T" failed 30
task_json "$CM" "$T" | jq -r .error | grep -q 'spawn helper' || fail "a task with the helper gone did not say why: $(task_json "$CM" "$T" | jq -r .error)"
no_sleeper 31340 || fail "a task ran with the spawn helper gone"
[[ -z $(srv ps -eo user:30,args | grep -E '^atlas-crew +sleep' || true) ]] || fail "something ran as the crew user"
srv systemctl start atlas-crew-spawn.socket
srv systemctl restart atlas-crew
until_true 60 "hostd to answer again" hostd info
T=$(send_task "$CM" "E2E_OK helper-back$RANDOM")
wait_task "$CM" "$T" "done" 60
echo "helper gone: task refused, nothing ran as the crew user; helper back: task done"

# ── 4. export, uninstall ──────────────────────────────────────────────────────────────────────────
say "7. atlas crew export, then uninstall"
atlas crew export "$TARGET" --port "$PORT" -o "$WORK/export.tar.gz" 2>&1 | tee "$OUT/export.log"
tar -tzf "$WORK/export.tar.gz" | tee "$OUT/export.list"
[[ $(tar -tzf "$WORK/export.tar.gz" | head -n1) == manifest.json ]] || fail "the archive does not start with manifest.json"
grep -q "^crew/$CM/crewmate.json\$" "$OUT/export.list" || fail "the archive does not hold the crewmate"
grep -q "^crew/$CM/tasks.jsonl\$" "$OUT/export.list" || fail "the archive does not hold the task history"
grep -q 'secrets.json' "$OUT/export.list" && fail "an export without --with-secrets holds secrets.json"
tar -xzOf "$WORK/export.tar.gz" "crew/$CM/tasks.jsonl" | grep -q 'fake-claude done' || fail "the archived task history lacks the finished task"

# Uninstall holds no ssh session of the crew user: close the tunnel (the desktop's, here).
kill "$TUNNEL_PID"
wait "$TUNNEL_PID" 2>/dev/null || true
TUNNEL_PID=
no_crew_session() { ! srv pgrep -u atlas-crew -x sshd >/dev/null; }
until_true 20 "the crew user's ssh session to end" no_crew_session

# No terminal and no --yes: it refuses and changes nothing.
if atlas crew uninstall "$TARGET" --port "$PORT" </dev/null >"$OUT/uninstall-refused.log" 2>&1; then
  fail "uninstall without --yes and without a terminal went ahead"
fi
cat "$OUT/uninstall-refused.log"
grep -q 'needs a decision' "$OUT/uninstall-refused.log" || fail "the refusal does not say why"
[[ $(srv systemctl is-active atlas-crew) == active ]] || fail "a refused uninstall stopped the service"
srv id atlas-crew >/dev/null || fail "a refused uninstall removed the crew user"

atlas crew uninstall "$TARGET" --port "$PORT" --yes 2>&1 | tee "$OUT/uninstall.log"
grep -q 'crew server removed' "$OUT/uninstall.log" || fail "uninstall did not say it removed the crew server"
grep -q "Deleted $HOME_DIR on $TARGET (2 crewmate(s))" "$OUT/uninstall.log" || fail "uninstall did not report what it deleted"

say "7b. nothing of the crew server is left"
left=$(srv_sh "
  getent passwd | cut -d: -f1 | grep -E '^atlas-(crew|bot|cm-)' | sed 's/^/user: /'
  getent group  | cut -d: -f1 | grep -E '^atlas-(crew|bot|cm-)' | sed 's/^/group: /'
  systemctl list-unit-files --no-legend 'atlas-*' | sed 's/^/unit file: /'
  systemctl list-units --all --no-legend 'atlas-*' | sed 's/^/unit: /'
  for p in /var/lib/atlas-crew /var/lib/atlas-cm /usr/local/lib/atlas-crew /etc/atlas-crew /etc/systemd/system/atlas-* /run/atlas-crew /home/atlas-crew; do
    [ -e \"\$p\" ] && echo \"path: \$p\"
  done
  nft list tables 2>/dev/null | grep atlas | sed 's/^/nft: /'
  ps -eo user:30,args | grep -E '^atlas-(crew|cm-)' | sed 's/^/process: /'
  true")
if [[ -n $left ]]; then
  printf '%s\n' "$left" >&2
  fail "uninstall left things behind (above)"
fi
echo "clean: no user, group, unit, file, firewall table or process of the crew server remains"
