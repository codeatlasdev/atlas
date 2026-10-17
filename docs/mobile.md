---
title: "Atlas Mobile"
description: "Control the Atlas on your Mac from your phone: projects, services, agents, approvals and terminals. No account."
---

# Atlas Mobile

Atlas Mobile (`apps/mobile`, iOS and Android, preview) is a remote control for the Atlas running on your Mac. There is no account and no Atlas server: the phone connects straight to `atlas hostd` on your Mac over [iroh](https://www.iroh.computer) (QUIC dialed by public key). It goes direct when the network allows and otherwise through n0's public relays. Traffic is end-to-end encrypted either way, and a relay can't read it.

It works while the Atlas window is closed, because hostd keeps running. The Mac has to be awake.

## Pairing

1. On the Mac: **Settings → Mobile → Pair a phone**. A QR code and a 6-digit code appear for two minutes.
2. In Atlas Mobile: **Add a Mac**, then scan the code.

The code works once. Five wrong attempts cancel it. After pairing, the phone is recognised by its own key. Each paired phone is listed under **Paired phones** with what it may do:

| Permission | Allows |
|---|---|
| See projects, logs, agents | project and service state, logs and search, agent tabs and replies, terminal list |
| Restart services, run actions, send prompts | restart/start/stop a service, start a stack, run an action (its confirm step is shown on the phone), send a prompt to an agent that is idle or done |
| Answer agent permission prompts | allow or deny a Claude Code / Codex tool call |
| Watch terminals | live terminal output |
| Type in terminals | keystrokes; the app also asks for Face ID / biometrics before each session |

**Revoke** cuts the phone off immediately and forgets it. A phone can never run shell commands through `exec`, read or write files, change its own permissions or manage devices.

## Approvals

Claude Code and Codex run `atlas hook … permission-request` before showing their own prompt. While a phone is connected, hostd shows the prompt on the phone and waits up to 55 s for Allow or Deny. With no phone connected, it answers nothing at once, and the agent asks in the terminal as usual. Re-run `atlas setup claude-code --apply` / `atlas setup codex --apply` once so the hook is registered with its 60 s timeout. omp approvals are shown as "needs you" but answered on the Mac.

## Files

Under `~/.atlas/mobile/` (`$ATLAS_HOSTD_DIR/mobile/`): `key` (the Mac's iroh identity, 0600), `devices.json` (paired phones and their permissions), `audit.log` (one JSON line per non-read request from a phone, with typed text logged as a length only). `ATLAS_MOBILE_RELAY=0` turns relays and address lookup off (LAN only).

## Building the app

Needs Flutter stable and a Rust toolchain (the transport is the `apps/mobile/native` crate, bound through flutter_rust_bridge). `scripts/mobile-check.sh` (part of `bun run check`) runs codegen, `flutter analyze` and `flutter test`. It is skipped with a notice when Flutter is not installed.
