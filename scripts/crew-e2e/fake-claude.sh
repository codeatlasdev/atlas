#!/bin/bash
# A fake `claude` for scripts/crew-e2e.sh, installed on the crew server where the real engine would be.
# It speaks the contract of `claude -p --output-format json`: one `{"type":"result",...}` line on stdout.
# The prompt (the last argument; hostd puts the role and the task text in it) says what to do:
#   E2E_OK <word>            answers `fake-claude done <word>`
#   E2E_FAIL                 exits 1 with an error, like an engine whose sign-in expired
#   E2E_PROBE                answers what its own process looks like: open descriptors, NoNewPrivs, cgroup, and
#                            whether `node` (a JIT) runs
#   E2E_SERVE <port>         serves `B-secret` on 127.0.0.1:<port> (a crewmate's dev server) and waits
#   E2E_CAPTURE <port>       asks hostd's `capture` tool, as the agent's MCP client does, for 127.0.0.1:<port> (another
#                            crewmate's server), the cloud metadata address, and a server of its own, and answers each
#                            reply: what an isolated crewmate's capture may reach
#   E2E_HANG <n>             starts `sleep <n>` as a child and waits for it: a tool the agent started
#   E2E_HANG_ONCE <n>        like E2E_HANG, but a run that hostd resumed after a restart ("was interrupted")
#                            finishes at once
# `sleep <n>` is how the script finds the process tree on the server.
if [ "$1" = auth ]; then
  # Like the real engine, it runs the hooks of the user's own config at startup (a SessionStart hook).
  [ -x "$HOME/.claude/e2e-hook.sh" ] && "$HOME/.claude/e2e-hook.sh"
  echo "Logged in (fake claude)"
  exit 0
fi
for a; do prompt=$a; done

reply() {
  printf '{"type":"result","subtype":"success","is_error":false,"result":"%s","session_id":"fake-session","usage":{"input_tokens":1,"output_tokens":1}}\n' "$1"
}
# The word after `$1` in the prompt.
arg_of() {
  printf '%s\n' "$prompt" | tr ' ' '\n' | grep -A1 -x "$1" | tail -n 1 | tr -cd 'A-Za-z0-9'
}

case "$prompt" in
  *E2E_FAIL*)
    echo "fake-claude: the engine failed on purpose" >&2
    exit 1
    ;;
  *E2E_PROBE*)
    # A child `sh`, listing the fds of ITS shell with a redirect that only the `ls` child sees (a pipe or a
    # command substitution would put a descriptor of its own in the shell).
    bash -c 'ls -l /proc/$$/fd >"$HOME/e2e-probe-l"; ls /proc/$$/fd >"$HOME/e2e-probe"; grep NoNewPrivs /proc/$$/status >>"$HOME/e2e-probe"; cat /proc/$$/cgroup >>"$HOME/e2e-probe"'
    # Node's V8 aborts at start under a seccomp MemoryDenyWriteExecute= that the agent inherited from the helper's unit.
    node=$(node -e 'let s=0;for(let i=0;i<1e6;i++)s+=i;console.log(s)' 2>&1 | tail -n 1 | tr -cd 'A-Za-z0-9 ')
    # Another crewmate's command line, from /proc: `ProtectProc=invisible` in the helper's unit hides it.
    others=$(for d in /proc/[0-9]*; do tr '\0' ' ' <"$d/cmdline" 2>/dev/null; echo; done | grep -c 'sleep 3134[1]')
    reply "probe: $(tr '\n\t' ' ;' <"$HOME/e2e-probe" | tr -d '"\\') node=$node others=$others extra=$(awk '$9>2{printf "%s=%s ", $9, $11}' "$HOME/e2e-probe-l" | tr -d '"\\')"
    exit 0
    ;;
  *E2E_SERVE*)
    n=$(arg_of E2E_SERVE)
    node -e "require('http').createServer((q,s)=>s.end('B-secret')).listen($n,'127.0.0.1')" &
    wait
    reply "fake-claude served"
    exit 0
    ;;
  *E2E_CAPTURE*)
    bport=$(arg_of E2E_CAPTURE)
    # The capture tool's request, the way the agent's MCP client makes it (crew/mcp.rs `post_task_tool`); the one
    # browser that needs no install of ours is chromium, and the refusals come before any browser starts.
    cap() {
      curl -s -X POST -H "X-Atlas-Hook-Token: $ATLAS_HOOK_TOKEN" -H 'Content-Type: application/json' \
        -d "{\"pane_key\":\"$ATLAS_PANE_KEY\",\"args\":{\"url\":\"$1\",\"engines\":[\"chromium\"],\"widths\":[390],\"themes\":[\"light\"]}}" \
        "http://127.0.0.1:$ATLAS_HOOK_PORT/crew/$ATLAS_CREWMATE_ID/capture" | tr -d '"\\\n' | cut -c1-260
    }
    # A server of this agent's own on 127.0.0.1:47777 (0xBAA1); its egress chain drops loopback, so wait for it in /proc.
    node -e "require('http').createServer((q,s)=>s.end('A-own')).listen(47777,'127.0.0.1')" &
    srv=$!
    for _ in $(seq 25); do grep -q ':BAA1 00000000:0000 0A' /proc/net/tcp && break; sleep 0.2; done
    b=$(cap "http://127.0.0.1:$bport/src/App.tsx")
    meta=$(cap "http://169.254.169.254/latest/meta-data/")
    own=$(cap "http://127.0.0.1:47777/")
    kill "$srv"
    reply "capture b=[$b] meta=[$meta] own=[$own]"
    exit 0
    ;;
  *E2E_HANG_ONCE*)
    case "$prompt" in *"was interrupted"*) reply "fake-claude resumed"; exit 0 ;; esac
    n=$(arg_of E2E_HANG_ONCE)
    ;;
  *E2E_HANG*)
    n=$(arg_of E2E_HANG)
    ;;
  *E2E_OK*)
    reply "fake-claude done $(arg_of E2E_OK)"
    exit 0
    ;;
  *)
    reply "fake-claude: nothing to do"
    exit 0
    ;;
esac
sleep "$n" &
wait
reply "fake-claude finished the hang"
