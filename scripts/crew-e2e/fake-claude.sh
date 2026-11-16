#!/bin/bash
# A fake `claude` for scripts/crew-e2e.sh, installed on the crew server where the real engine would be.
# It speaks the contract of `claude -p --output-format json`: one `{"type":"result",...}` line on stdout.
# The prompt (the last argument; hostd puts the role and the task text in it) says what to do:
#   E2E_OK <word>            answers `fake-claude done <word>`
#   E2E_FAIL                 exits 1 with an error, like an engine whose sign-in expired
#   E2E_PROBE                answers what its own process looks like: open descriptors, NoNewPrivs, cgroup, and
#                            whether `node` (a JIT) runs
#   E2E_HANG <n>             starts `sleep <n>` as a child and waits for it: a tool the agent started
#   E2E_HANG_ONCE <n>        like E2E_HANG, but a run that hostd resumed after a restart ("was interrupted")
#                            finishes at once
# `sleep <n>` is how the script finds the process tree on the server.
if [ "$1" = auth ]; then
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
    sh -c 'ls /proc/$$/fd >"$HOME/e2e-probe"; grep NoNewPrivs /proc/$$/status >>"$HOME/e2e-probe"; cat /proc/$$/cgroup >>"$HOME/e2e-probe"'
    # Node's V8 aborts at start under a seccomp MemoryDenyWriteExecute= that the agent inherited from the helper's unit.
    node=$(node -e 'let s=0;for(let i=0;i<1e6;i++)s+=i;console.log(s)' 2>&1 | tail -n 1 | tr -cd 'A-Za-z0-9 ')
    # Another crewmate's command line, from /proc: `ProtectProc=invisible` in the helper's unit hides it.
    others=$(for d in /proc/[0-9]*; do tr '\0' ' ' <"$d/cmdline" 2>/dev/null; echo; done | grep -c 'sleep 31341')
    reply "probe: $(tr '\n\t' ' ;' <"$HOME/e2e-probe" | tr -d '"\\') node=$node others=$others"
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
