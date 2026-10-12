# Privacy

Atlas is local-first. It runs on your machine, reads your project, and starts your processes. **The Atlas CLI, TUI, MCP server and Atlas Desktop have no telemetry, analytics, crash reporting or accounts.** Nothing about you, your projects, your logs or your usage is sent to CodeAtlas.

This page lists every network request Atlas itself makes, so you can check it against the code.

## What the software sends

| When | Where | What |
|---|---|---|
| Update checks: `atlas update`, the update prompt before `atlas dev`, the Atlas Desktop update check (on start, on window focus and on a timer, unless you turn automatic checks off in Settings) | `api.github.com` (the public releases of `codeatlasdev/atlas`) | A plain HTTPS request for the latest release. GitHub sees your IP address and a `curl` user agent, as for any request to its API. If `GITHUB_TOKEN`, `GH_TOKEN` or `gh auth token` provides a token, it is sent to GitHub only, to avoid rate limits. |
| Installing or updating (`install.sh`, `atlas update`, `atlas desktop`) | `github.com` release downloads | The release archive and its `.sha256`, which Atlas verifies before installing. |

Set `ATLAS_NO_UPDATE_PROMPT=1` to disable the prompt shown before `atlas dev`. The update prompt uses a cache (`~/.atlas/.update_check`) and a 2 second timeout.

Everything else stays on your machine: logs in `.logs/`, settings and projects in `~/.atlas/`, terminal scrollback, agent hook events (a loopback-only HTTP server with a per-run token) and the control sockets (`0600` unix sockets).

Atlas does run the commands **you** put in `atlas.yaml`, your terminal sessions, the pages you open in the embedded browser and the AI agents you launch. Their network traffic is theirs, not Atlas's.

## The website

[atlas.codeatlas.com.br](https://atlas.codeatlas.com.br) is a static site on Cloudflare Pages with a strict same-origin Content-Security-Policy: no third-party fonts, scripts, ads or embeds. The host (Cloudflare) keeps standard server logs and, if enabled for the project, its cookieless Web Analytics.

The site has a **release-notes form** ("Get release notes by email"). If you submit it, Atlas stores your email address, the page language and a timestamp in a database (Cloudflare D1) to email you about new releases. The form rejects cross-origin posts, ignores a honeypot field and rate-limits by a salted hash of your IP address; the raw IP is never stored or logged, and the function does not log request data. To have your address removed, email the maintainers or open an issue naming only a way to contact you, never the address itself in public.

## Changes

If Atlas ever adds telemetry it will be opt-in, documented here first, and called out in the changelog. Questions: see [SUPPORT.md](SUPPORT.md). Security issues: [SECURITY.md](SECURITY.md).
