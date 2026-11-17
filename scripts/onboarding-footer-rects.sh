#!/usr/bin/env bash
# Records getBoundingClientRect() of the crew onboarding footer, its Back slot and its primary button on every step
# and state, at two window sizes, in an isolated agent instance (`bun run desktop:agent -- --name crewos`):
#   ATLAS_DEBUG_SOCK=/private/tmp/atlas-agent-crewos/debug.sock scripts/onboarding-footer-rects.sh [atlas] [shot-dir]
# Every row of one size must be identical: the footer never moves between steps or states.
set -euo pipefail
A=${1:-target/debug/atlas}
OUT=${2:-/tmp/crewos-rects}
mkdir -p "$OUT"
ev() { "$A" desktop debug eval "$1"; }
rect='const r=s=>{const e=document.querySelector(s);if(!e)return null;const b=e.getBoundingClientRect();return [b.x,b.y,b.width,b.height].map(n=>Math.round(n*10)/10).join(",")};return "footer="+r("[data-testid=onboarding-footer]")+" back="+r(".back-slot")+" primary="+r("[data-testid=onboarding-primary]")'
snap() { # size name
  sleep 0.9
  "$A" desktop debug shot "$OUT/$1-$2.png" >/dev/null
  printf '%-10s %-18s %s\n' "$1" "$2" "$(ev "$rect" | tr -d '"')"
}
click() { "$A" desktop debug click "$1" >/dev/null; }
for size in "1100 720" "980 620"; do
  tag=${size// /x}
  "$A" desktop debug resize $size >/dev/null
  ev 'localStorage.removeItem("crew:onboarding:seen"); window.__atlas.ws.appMode="crew"; return 1' >/dev/null
  sleep 1.5
  snap "$tag" intro
  click '[data-testid=onboarding-primary]'; snap "$tag" how
  click '[data-testid=onboarding-primary]'; sleep 0.5
  "$A" desktop debug type '[name="ssh-target"]' 'ubuntu@203.0.113.7' >/dev/null
  ev 'document.querySelector("[role=radiogroup] button, [role=radio]")?.click(); return 1' >/dev/null
  snap "$tag" connect-key
  ev 'const b=[...document.querySelectorAll("button")].find(x=>x.textContent.trim()==="Password"); b&&b.click(); return 1' >/dev/null
  snap "$tag" connect-password
  # an error: the sandbox has no route to the address, so the first contact fails for real
  ev 'const i=document.querySelector("[name=ssh-password]"); i.value="x"; i.dispatchEvent(new Event("input",{bubbles:true})); return 1' >/dev/null
  click '[data-testid=onboarding-primary]'
  snap "$tag" installing
  sleep 20
  snap "$tag" connect-error
  ev 'const b=[...document.querySelectorAll("button")].find(x=>x.textContent.includes("Back")); b&&b.click(); return 1' >/dev/null
  ev 'window.__atlas && 0; return 1' >/dev/null
done
