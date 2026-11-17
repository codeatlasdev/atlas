#!/usr/bin/env bash
# Records getBoundingClientRect() of the crew onboarding footer, its Back slot and its primary button on every step and
# state, at two window sizes, in an isolated agent instance (`bun run desktop:agent -- --name crewos`), and takes a
# screenshot of each state. The flow is driven through `window.__atlas.crewOb` (the Onboarding instance, debug builds).
#   ATLAS_DEBUG_SOCK=/private/tmp/atlas-agent-crewos/debug.sock scripts/onboarding-footer-rects.sh [atlas] [shot-dir]
# Every row of one size must be identical: the footer never moves between steps or states.
set -euo pipefail
A=${1:-target/debug/atlas}
OUT=${2:-/tmp/crewos-rects}
mkdir -p "$OUT"
ev() { "$A" desktop debug eval "$1"; }
RECT='const r=s=>{const e=document.querySelector(s);if(!e)return null;const b=e.getBoundingClientRect();return [b.x,b.y,b.width,b.height].map(n=>Math.round(n*10)/10).join(",")};return "footer="+r("[data-testid=onboarding-footer]")+" back="+r(".back-slot")+" primary="+r("[data-testid=onboarding-primary]")'
snap() { # tag name js-that-sets-the-state
  ev "const ob=window.__atlas.crewOb; $3; return 1" >/dev/null
  sleep 1.3
  "$A" desktop debug shot "$OUT/$1-$2.png" >/dev/null
  printf '%-10s %-20s %s\n' "$1" "$2" "$(ev "$RECT" | tr -d '"')"
}
for size in "1100 720" "980 620"; do
  tag=${size// /x}
  "$A" desktop debug resize $size >/dev/null
  ev 'localStorage.removeItem("crew:onboarding:seen"); window.__atlas.crew.servers=[]; window.__atlas.ws.appMode="crew"; return 1' >/dev/null
  sleep 1.5
  snap $tag intro 'ob.step="intro"'
  snap $tag how 'ob.step="how"'
  snap $tag connect-key 'ob.step="connect"; ob.sshTarget="ubuntu@localhost"; ob.sshPort=22; ob.chooseMethod("key")'
  snap $tag connect-password 'ob.chooseMethod("password"); ob.sshPassword="x"'
  snap $tag connect-installing 'ob.installRunning=true; ob.install={done:new Set(["signin","check"]),active:"install",user:""}'
  snap $tag connect-service 'ob.install={done:new Set(["signin","check","install","user"]),active:"service",user:""}'
  snap $tag connect-live 'ob.install={done:new Set(["signin","check","install","user","service","ssh","done"]),active:null,user:""}'
  snap $tag connect-failed-ssh 'ob.installRunning=false; ob.install={done:new Set(["signin","check"]),active:"install",user:""}; ob.failedPhase="install"'
  # a real failure through the real flow: nothing listens on port 1
  snap $tag connect-error 'ob.failedPhase=null; ob.install={done:new Set(),active:null,user:""}; ob.sshPort=1; ob.sshPassword="x"; await ob.launchServer()'
  snap $tag engine-idle 'ob.sshPort=22; ob.failure=null; ob.failedPhase=null; ob.step="engine"; ob.engineStatus={}'
  snap $tag engine-installing 'const ids=ob.integrations.map(a=>a.id); ob.engineStatus={[ids[0]]:{phase:"installing"},[ids[1]]:{phase:"needs_signin"},[ids[2]]:{phase:"not_installed"},[ids[3]]:{phase:"not_installed"}}; ob.installLog="Downloading claude 2.1…\nverifying checksum"'
  snap $tag engine-ready 'const ids=ob.integrations.map(a=>a.id); ob.installLog=""; ob.engineStatus={[ids[0]]:{phase:"ready"},[ids[1]]:{phase:"needs_signin"},[ids[2]]:{phase:"not_installed"},[ids[3]]:{phase:"not_installed"}}; ob.selectedEngine=ids[0]'
  snap $tag ready 'ob.step="ready"'
done
