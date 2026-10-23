# Privacy

Atlas is local-first. It runs on your machine, reads your project, and starts your processes. **The Atlas CLI, TUI, MCP server and Atlas Desktop have no telemetry, analytics, crash reporting or accounts.** Nothing about you, your projects, your logs or your usage is sent to CodeAtlas.

This page lists every network request Atlas itself makes, so you can check it against the code.

## What the software sends

| When | Where | What |
|---|---|---|
| Update checks: `atlas update`, the update prompt before `atlas dev`, the Atlas Desktop update check (on start, on window focus and on a timer, unless you turn automatic checks off in Settings) | `api.github.com` (the public releases of `codeatlasdev/atlas`) | A plain HTTPS request for the latest release. GitHub sees your IP address and a `curl` user agent, as for any request to its API. If `GITHUB_TOKEN`, `GH_TOKEN` or `gh auth token` provides a token, it is sent to GitHub only, to avoid rate limits. |
| Installing or updating (`install.sh`, `atlas update`, `atlas desktop`) | `github.com` release downloads | The release archive and its `.sha256`, which Atlas verifies before installing. |
| Atlas Crew push notifications (when a bot task finishes or needs approval, while `ATLAS_PUSH_RELAY` is not `off`) | `push.codeatlas.com.br` (operated by CodeAtlas) | The FCM device token and the event kind (`task_done` or `needs_you`). No content, no prompt text, no file names. The relay logs only the kind and the HTTP status code. Set `ATLAS_PUSH_RELAY=off` in the bot's systemd unit to disable entirely. |
| Atlas Crew Telegram bridge (when `bot_telegram_set` is called with a bot token) | `api.telegram.org` (Telegram) | Standard Telegram Bot API polling and message sending. Messages include bot task results (truncated to 4 000 chars) and tool approval prompts (compact JSON, ≤ 1 000 chars). Telegram sees your bot token. |

Set `ATLAS_NO_UPDATE_PROMPT=1` to disable the prompt shown before `atlas dev`. The update prompt uses a cache (`~/.atlas/.update_check`) and a 2 second timeout.

Everything else stays on your machine: logs in `.logs/`, settings and projects in `~/.atlas/`, terminal scrollback, agent hook events (a loopback-only HTTP server with a per-run token) and the control sockets (`0600` unix sockets).

Atlas does run the commands **you** put in `atlas.yaml`, your terminal sessions, the pages you open in the embedded browser and the AI agents you launch. Their network traffic is theirs, not Atlas's.

## Atlas Mobile

The phone app ([docs/mobile.md](docs/mobile.md)) has no account, telemetry, analytics or crash reporting either. It talks only to the computers you pair it with.

| Where | What |
|---|---|
| On the phone (iOS Keychain / Android Keystore, through the system's secure storage) | The phone's own key (generated on the phone, never leaves it) and the list of paired computers: name, public key, network addresses. "Forget this computer" deletes a computer. iOS can keep Keychain items after the app is deleted. |
| On your computer (`~/.atlas/mobile/`) | The computer's key, the paired phones (name, public key, permissions) and an audit log of what each phone did, with typed text recorded only as a length. |
| Between the phone and your computer | Requests and replies: project and service state, logs, agent state and conversations, terminal output and input. End-to-end encrypted (QUIC, keys known only to the two devices). |
| n0 public relays (`*.relay.n0.iroh.link`, run by number 0, the makers of iroh) | Used to find your computer and, when a direct connection is blocked, to forward the encrypted traffic. A relay sees both devices' IP addresses and public keys and how much data passes, never the content. |

The camera is used only to scan the pairing code; nothing is recorded. Face ID / biometrics are checked by the system before typing in a terminal; the app only receives yes or no.

## The website

[atlas.codeatlas.com.br](https://atlas.codeatlas.com.br) is a static site on Cloudflare Pages with a strict same-origin Content-Security-Policy: no third-party fonts, scripts, ads or embeds. The host (Cloudflare) keeps standard server logs and, if enabled for the project, its cookieless Web Analytics.

The site has a **release-notes form** ("Get release notes by email"). If you submit it, Atlas stores your email address, the page language and a timestamp in a database (Cloudflare D1) to email you about new releases. The form rejects cross-origin posts, ignores a honeypot field and rate-limits by a salted hash of your IP address; the raw IP is never stored or logged, and the function does not log request data. To have your address removed, email the maintainers or open an issue naming only a way to contact you, never the address itself in public.

## Changes

If Atlas ever adds telemetry it will be opt-in, documented here first, and called out in the changelog. Questions: see [SUPPORT.md](SUPPORT.md). Security issues: [SECURITY.md](SECURITY.md).
