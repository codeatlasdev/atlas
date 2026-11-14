---
title: "Atlas Crew"
description: "Run named AI agents (crewmates) 24/7 on your own Linux server, driven from Atlas Desktop, Atlas Mobile, Telegram or a webhook."
---

# Atlas Crew

Atlas Crew runs AI agents around the clock on a Linux server you own, without your laptop. You install `atlas hostd` as a systemd service on the server (a **crew server**); on it you recruit **crewmates**: named agents with a role, a look, a task queue, memory and automations. Agents (omp, Claude Code, Codex, OpenCode) run on your own subscription or API key; Atlas Crew never calls a model API.

| Word | Meaning |
|---|---|
| **crew** | everything you run with Atlas Crew; crew-level settings |
| **crew server** | a Linux machine that runs crewmates (`atlas crew install`) |
| **crewmate** | one agent with a name, role, agent engine, approval mode, look, memory and tasks |
| **task** | one prompt a crewmate works on, one at a time per crewmate |
| **automation** | something that starts a task by itself: a schedule, an interval, a webhook, a GitHub event, a file change, another crewmate or a service (`triggers` on the wire) |

Sections: [Install a crew server](#install-a-crew-server) · [Sign agents in](#sign-agents-in) · [Crewmates](#crewmates) · [Tasks and approvals](#tasks-and-approvals) · [Automations](#automations) · [GitHub](#github) · [Telegram](#telegram) · [Push notifications](#push-notifications) · [Desktop](#atlas-desktop) · [Mobile](#atlas-mobile) · [MCP tools](#mcp-tools-for-the-agent) · [Security model](#security-model) · [Troubleshooting](#troubleshooting) · [Crewmate look](#crewmate-look) · [Code map](#code-map) · [Reference](#reference)

## Install a crew server

```sh
# 1. One time, needs sudo on the server
atlas crew install ubuntu@myserver.example.com

# 2. Sign each agent in as the crew user (see below)
atlas crew login ubuntu@myserver.example.com claude

# 3. Add the server in Atlas Desktop (Atlas Crew → Settings), or reach it from Atlas Mobile through your Mac
```

Requirements:

- A Linux x86_64 server with `systemd` and `sudo` (passwordless sudo when you run the command from a terminal).
- An SSH public key at `~/.ssh/id_ed25519.pub` or `~/.ssh/id_rsa.pub` on your machine.
- Internet access on the server, or a cached binary at `~/.atlas/remote/atlas-x86_64-linux` on your machine (see [Offline install](#offline-install)).

`atlas crew install <ssh-target>` does, in order: checks the host is Linux x86_64 with systemd and sudo; installs `atlas` to `/usr/local/lib/atlas-crew/`; creates the `atlas-crew` system user (home `/var/lib/atlas-crew`); links the binary at `<home>/.atlas/bin/atlas`; writes and enables `/etc/systemd/system/atlas-crew.service`; appends your SSH public key to the crew user's `authorized_keys`. The service starts on boot and restarts on failure.

### On a server that already runs crewmates

Servers installed before the rename used the name `atlas-bot`. Nothing breaks and nothing is lost:

- `atlas bot …` still works as an alias of `atlas crew …` (hidden from `--help`), so existing scripts keep running.
- Run `atlas crew install <ssh-target>` again on that server. It stops, disables and removes the old `atlas-bot.service`, removes `/usr/local/lib/atlas-bot/`, installs `atlas-crew.service` and **keeps your data**.
- If the system user `atlas-bot` exists it is **reused**, never renamed or deleted: its home (`/var/lib/atlas-bot`) holds your crewmates, memory and secrets. The installer says so and prints a `user: <name>` line. New servers get `atlas-crew`.
- On the first start, hostd renames the store `<hostd dir>/bots/` to `<hostd dir>/crew/` (and Atlas Desktop renames `~/.atlas/desktop/bot-hosts.json` to `crew-servers.json`). If both old and new exist, the new one wins and the old one is left untouched with a log line.

Run `atlas crew status <ssh-target>` afterwards to check.

### Commands

| Command | What it does |
|---|---|
| `atlas crew install <ssh-target> [--yes] [--port N] [--binary PATH]` | install or migrate a crew server (above). `--port` for a non-standard SSH port; `--binary` uploads that Linux x86_64 `atlas` instead of downloading this version's release (also read from `ATLAS_CREW_BINARY`) |
| `atlas crew uninstall <ssh-target> [--yes]` | stops and removes the service, the crew user (and its home, so your crewmates) and `/usr/local/lib/atlas-crew`. Asks first unless `--yes`. Refuses while the user still has a login session; it never kills processes |
| `atlas crew status <ssh-target>` | `systemctl status` of the service plus `atlas hostd --status` as the crew user |
| `atlas crew logs <ssh-target> [-f]` | last 200 lines of `journalctl -u atlas-crew`; `-f` follows |
| `atlas crew login <ssh-target> <agent>` | opens `ssh -t <crew-user>@<host> <agent-binary>` so you finish the agent's own sign-in. `<agent>` is `omp`, `claude`, `codex` or `opencode`; the command prints the install line for that agent first |

### Offline install

If the server has no internet, run `atlas update` on a connected machine so `~/.atlas/remote/atlas-x86_64-linux` is current, then `atlas crew install`. When `curl` fails on the server, the command uploads the cached binary. The downloaded release is checked against its `.sha256` before it is installed.

## Sign agents in

Agents are installed and signed in as the crew user. `atlas crew login <ssh-target> <agent>` prints the official install line and opens the sign-in session:

| Agent | Install line (run as the crew user) |
|---|---|
| omp | `curl -fsSL https://omp.sh/install \| sh` |
| Claude Code | `curl -fsSL https://claude.ai/install.sh \| bash` |
| Codex | `npm_config_prefix="$HOME/.npm-global" npm install -g @openai/codex` |
| OpenCode | `curl -fsSL https://opencode.ai/install \| bash` |

The unit's `PATH` already includes `~/.atlas/bin`, `~/.bun/bin`, `~/.local/bin`, `~/.opencode/bin` and `~/.npm-global/bin`. A crewmate can also use a custom provider (your own gateway) set in its settings; keys go to its `secrets.json` ([Security model](#security-model)).

## Crewmates

You manage crewmates from Atlas Desktop, Atlas Mobile, Telegram or the hostd protocol ([Reference](#reference)). A crewmate has:

- **name**: 1 to 40 characters.
- **role**: up to 2000 characters, prepended to every task as context.
- **agent**: `omp`, `claude`, `codex` or `opencode`, with an optional model and provider.
- **approval**: `ask` (hold each tool call for your answer) or `auto` (allow all without prompting). omp crewmates always run `auto`, because omp's hook protocol cannot take a remote decision.
- **project**: a Git URL (cloned once per server and checked out as a per-crewmate worktree) or a path on the server. Default working directory: `<crewmate dir>/work`.
- **memory**: a markdown file (up to 64 KiB) the agent reads and edits across tasks.
- **look**: see [Crewmate look](#crewmate-look).
- **automations**: see [Automations](#automations). Older scheduled routines (5-field cron) still run.

Crew-level settings (`crew_settings_get` / `crew_settings_set`): `short_replies` (default on) summarizes long replies, optionally with `short_replies_model`. Stored in `<hostd dir>/crew.json`.

A new crewmate starts with a short setup conversation (project, role, engine sign-in) in its chat.

## Tasks and approvals

A task is one prompt. Sources: `mobile`, `desktop`, `telegram`, `routine`, `trigger`. A crewmate runs one task at a time in a pty; the rest queue. A task ends `done`, `failed` or `cancelled`, with a hard cap of 2 hours. Prompts are 1 to 20000 characters. While it works, the steps the agent reports show live in the chat, grouped under the title it set for itself.

A task can also stop and wait for **you** (`waiting_action`): connect GitHub, provide a secret, answer a question, choose an option or sign an engine in. Answers go through `crewmate_action_answer`; a secret is written to the crewmate's `secrets.json` and never stored in the task. An agent proposes an action by ending its turn with a line `ATLAS_ACTION: {...}`.

### Approvals

With `approval: ask`, a tool call pauses the task and waits up to 30 minutes for `agent_approve {pane, req, allow}`; the pane key is `crew:<crewmate id>:<task id>`. The wait only starts when someone can answer: a desktop client is connected, a push token is registered, or a Telegram chat is linked. Otherwise the prompt times out at once, so configure one of them before creating an `ask` crewmate. Claude Code and Codex: re-run `atlas setup claude-code|codex --apply` once so their hook uses the matching 1805 s timeout.

## Automations

Automations (`triggers` on the wire) start a task by themselves. Build one in a crewmate's settings (Automations), or ask in chat ("every weekday at 9, review the open PRs") and the crewmate proposes it as a card you add with one click.

| Kind | JSON `type` | Fields |
|---|---|---|
| Schedule | `schedule` | `cron`: 5 fields, evaluated in the automation's `tz` (UTC when unset) |
| Interval | `interval` | `every_secs` ≥ 60 |
| Webhook | `webhook` | `token`, called as `POST /crew/<crewmate id>/hook/<token>` on the hook server (webhooks set up before the rename keep working at `/bots/<crewmate id>/hook/<token>`) |
| GitHub | `github` | `repo` (`owner/repo`), `events`: `pr_opened`, `pr_review_requested`, `issue_opened`, `ci_failed`, `release`. Polled every 2 minutes with `gh` |
| File | `file` | `path`, optional `glob` |
| After crewmate | `crewmate` | `from_crewmate` (id), `on`: `done` or `failed` |
| Service | `service` | `project_dir`, `service`, `on`: `failed` or `recovered` |

Rules the server enforces:

- Every automation has an `enabled` flag, a `prompt` (it may contain `{{event}}`) and an optional IANA `tz` (`"America/Sao_Paulo"`); an unknown zone is rejected with `unknown timezone: <tz>`.
- `crewmate_triggers_set` replaces the whole list. An existing `id` keeps its `last_fired`; a **new** schedule or interval starts from now, so it first fires at the next real time, not on the next 30 s tick.
- `next_fire` is computed by the server on every `crewmate_list`; a client value is ignored.
- `crewmate_task` takes the caller's `tz`; it is stored on the task and stamped on any automation proposed in it.
- An agent proposes an automation with `ATLAS_ACTION: {"kind":"add_trigger","trigger":{...},"prompt":"...","reason":"..."}`. An invalid one resolves the task with `The automation was invalid: <error>` so the agent can retry. Otherwise a card shows **Add** / **Not now**, answered with `crewmate_action_answer {id, task, value: "add"|"skip"}`. Webhooks are not offered to agents.

The Crew board (below) shows every queued, working, waiting and finished task across crewmates.

## GitHub

Crewmates that clone private repositories or open PRs need `gh` signed in on the crew server. In Atlas Desktop, Atlas Crew → Settings → GitHub starts a device-code login (`crew_github_login_start`, then `crew_github_login_poll`); hostd installs `gh` under the crew user's home if it is missing and configures git to use it. `crew_github_status` reports `{installed, login}`, `crew_github_logout` removes it. A task that needs GitHub waits in a **Connect GitHub** action and resumes when the login completes (`crew_github_connect_resume`).

## Telegram

One Telegram bot per crew server; every linked chat sees all of its crewmates.

1. Create a Telegram bot with [@BotFather](https://t.me/BotFather) and copy its token.
2. In Atlas Desktop, **Settings → Atlas Crew**, under the server (it must be online), paste the token in **Telegram** and click **Save** (`crew_telegram_set`). The token is write-only: the row shows only whether one is set and how many chats are linked (`crew_telegram_status`).
3. Click **Link chat** to get a 6-digit code valid 10 minutes (`crew_telegram_link`), and send your bot `/start <code>` in Telegram. Repeat for each chat.
4. **Remove** deletes the token; linked chats stop receiving.

Linked chats receive task results (truncated to 4000 characters) and approval prompts as Allow / Deny buttons. Commands:

| Command | What it does |
|---|---|
| `/crew` | list the crewmates and their agents |
| `/task <crewmate name> <text>` | queue a task |
| plain text | goes to the only crewmate when exactly one exists |

Telegram sees your bot token and the messages above. Nothing goes to CodeAtlas.

## Push notifications

When a task ends or a crewmate needs your approval, the crew server pushes to your phone.

- After pairing, push is automatic: the app registers its FCM token with each crew server (`push_register`, `push_unregister`).
- The relay at `push.codeatlas.com.br` gets the token and the event kind (`task_done` or `needs_you`), never content. Phones that are connected at that moment are skipped.
- Disable: set `Environment=ATLAS_PUSH_RELAY=off` in the unit, then `sudo systemctl daemon-reload && sudo systemctl restart atlas-crew`. Or point it at a self-hosted relay (`services/push-relay/`).

## Atlas Desktop

Open the Atlas Crew workspace with **⌘⇧B** (command *Atlas Crew*, remappable). Two views, switched with **⌘⇧C** or the bar at the top edge:

- **Chat**: the crewmate list (search, ⌘1…⌘9 to switch, server status in the footer) and the selected crewmate's thread: tasks, live steps, inline approval and action cards, and a composer that queues while another task runs. A settings window (General, Automations, Engine, Memory, Usage) saves each field on its own.
- **Board**: Queued, Working, Needs you and Done columns across every crewmate, with filters by crewmate, origin and text, a Done range, and a click that opens the chat at that task.

Settings → Atlas Crew holds the crew servers (SSH target + name, listed in `~/.atlas/desktop/crew-servers.json`). The desktop reaches each server over SSH and an `ssh -L` tunnel to its `hostd.sock`. The server must already have `atlas crew install` done; Atlas Desktop can run the install for you and asks for the sudo password on stdin (never on argv).

## Atlas Mobile

The phone talks only to its paired Macs; the Mac's hostd relays to the crew servers in the desktop list. Details and scopes: [Atlas Mobile → Crew on the phone](mobile.md#crew-on-the-phone).

The **Crew** tab lists every crewmate across paired computers (state dot, last message, time). Crewmates waiting for you appear in a "needs you" card at the top of the Computers and Crew tabs. A crewmate opens as a chat (prompt and reply bubbles, steps collapsed, inline Allow / Deny, composer that queues) and a profile screen: role, approval, automations, memory (View / Edit), delete.

When you add a Mac in Atlas Desktop, phones get a `mobile_invite` push (`computer_invite` event) and show a **Pair** sheet; dismissing it calls `mobile_invite_dismiss`.

## MCP tools for the agent

Each crewmate's agent gets a stdio MCP server, `atlas-crew-mcp` (`atlas mcp --crewmate <id>`; `--bot` is a hidden alias), registered for the task. Environment: `ATLAS_HOOK_PORT`, `ATLAS_HOOK_TOKEN`, `ATLAS_CREWMATE_ID`, `ATLAS_PANE_KEY`.

| Tool | Use |
|---|---|
| `ask_user` | post a question to Desktop, Mobile or Telegram and block until answered (up to 30 min). Backed by `POST /crew/<id>/ask` |
| `set_profile` | change this crewmate's own name or role. Backed by `POST /crew/<id>/set_profile` |
| `approve` | answer a Claude permission prompt (`--permission-prompt-tool atlas:approve`) |

The hook routes live on the same local HTTP server as the agent hooks and need its token.

## Security model

- **Dedicated user.** The crew user (`atlas-crew`, or the reused `atlas-bot`) has no sudo; it writes only its home and what agents create in `workdir`.
- **Systemd hardening.** `NoNewPrivileges=true`, `ProtectSystem=strict`, `ReadWritePaths=<crew home>`, `PrivateTmp=true`.
- **Scoped SSH key.** `atlas crew install` appends your key to the crew user's `authorized_keys` only, never to the admin user's.
- **sudo once.** Install and uninstall need sudo; the running service never does. The sudo password, when Desktop passes it, comes over stdin and is dropped after the credential cache is primed.
- **Secrets.** Per-crewmate secrets live in `<crewmate dir>/secrets.json` (mode 0600), are injected as environment into that crewmate's agent only and are never returned in any reply. Terminals and secrets are never relayed to a phone.
- **No CodeAtlas servers.** The only optional outbound call is the push relay (kind only). Details: [PRIVACY.md](https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md).

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `crewmate_*` commands answer `this computer is not a crew server (atlas crew install)` | hostd runs without `ATLAS_CREW_HOST=1`. The unit sets it; do not run `atlas hostd` by hand on a crew server |
| `info.caps` has no `"crew"` | same cause; clients check this before showing the Crew UI |
| Approval prompt times out at once | no desktop client, push token or Telegram chat to answer; set one up ([Approvals](#approvals)) |
| omp crewmate never asks | by design, omp always runs `auto` |
| `could not connect … over SSH` | SSH itself failed (refused, timeout, key rejected), not the server's OS; fix SSH first |
| `sudo … asks for a password` | grant passwordless sudo, or run the install from Atlas Desktop |
| Uninstall says the user is still logged in | close Atlas Desktop and any ssh session as the crew user, run it again |
| A first task shows `Downloading <repo>…` | the project clones before the task starts; a missing folder is refused when you set it |
| Old `atlas bot` command in a script | still works; switch to `atlas crew` when convenient |

Logs: `atlas crew logs <ssh-target> -f`. State: `atlas crew status <ssh-target>`.

## Crewmate look

Every crewmate is an animated character built from the Atlas globe: it blinks, glances around, breathes, hops when it needs you and dozes when offline. You pick its look in **Customize look** (new-crewmate dialog, or click its avatar in settings). Source of truth: `apps/desktop/src/lib/crew/look.ts`.

Editor layout: a large live preview that pops on every pick, preview-state buttons, **Randomize** (all rows at once, animated, not a hard cut), **Role kits**, then one row per part. Each row shows its label and the chosen name; tiles tint when selected; the first tile of every row is **None** except Color. A pick animates on (hat drops with a bounce, outfit wipes on, tool is tossed into the hand, eyewear snaps on).

| Row | Options | Default |
|---|---|---|
| Color | 9 swatches (Atlas blue `#3d5bff` first) plus a custom hex | stable pick from the crewmate id |
| Eyes | Round, Dots, Happy, Visor, Cyclops, Sleepy | Round |
| Hat | None, Straw hat, Cap, Beanie, Top hat, Crown, Party, Wizard, Headphones, Antenna, Map bicorne, Bandana | Straw hat |
| Tool (right hand) | None, Compass, Spyglass, Wrench, Quill, Magnifier, Megaphone, Map pin, Lantern | None |
| Outfit | None, Scarf, Captain coat, Tool apron, Satchel, Tie | None |
| Eyewear | None, Eyepatch, Monocle, Glasses, Sunglasses, Goggles | None |

- **Straw hat** (default): wide flat brim, woven texture, a band in the crewmate's accent color (never red), a thin globe-meridian arc on the crown, a slight tilt with a secondary idle brim-tip animation. Scales from 24 to 140 px.
- **Role kits** set hat, tool, outfit and eyewear in one click and keep color and eyes: Captain, Sales, Reviewer, Lookout, Scribe, Lighthouse keeper.
- Tools and outfits draw from 40 px up; list icons stay a clean face. Eyewear follows the gaze at every size. While working, each tool has its own gesture (the wrench turns, the quill scribbles, the spyglass sweeps), driven by a spring rig (`rig.ts`). Preview: `/crew-demo` in dev.
- Persisted and remote looks are untrusted: `normalizeLook` keeps every valid field and replaces each invalid one with the default for that crewmate. Clients own the catalogue, so an unknown id renders as the default.
- hostd stores the look on the crew server and returns every field in `crewmate_list` (`color` as `#rrggbb`; `eyes`, `hat`, `tool`, `outfit`, `face` as `[a-z0-9_-]`, 1 to 24 characters), so a look picked on one client shows on all of them.

## Code map

For contributors. One concept per file; the module doc comment at the top of each file says what it owns.

| Layer | Where | Owns |
|---|---|---|
| hostd (crew server and Mac) | `crates/atlas-host/src/crew/` | command routing (`mod.rs`) to `handlers/` (crewmates, tasks, settings); `hub.rs` (the crew hub); `store/` (on-disk types, the old-layout migration); `runner/` (one task: queue, `pty`, `conversation`, `prompt`, `action`, `outcome`, `summary`); `events.rs` (every push event, one builder each); `pane.rs` (the `crew:` pane key); `engine/` (omp, claude, codex, opencode); `triggers/` and `schedule.rs` (automations); `github/`; `telegram.rs`; `push.rs`; `project.rs` (checkout); `mcp.rs` (`atlas-crew-mcp`); `relay/` (phone to crew server); `tests/` (integration tests by area, shared helpers in `tests/fixture.rs`) |
| hook routes | `crates/atlas-host/src/agents/hook_server.rs` | `/crew/<id>/ask`, `/crew/<id>/set_profile`, `/crew/<id>/hook/<token>` |
| CLI | `crates/atlas-cli/src/commands/crew/` | `atlas crew install\|uninstall\|status\|logs\|login`; the unit text and migration plan are pure functions with tests |
| Desktop backend | `crates/atlas-desktop/src/crew.rs` | the server registry (`crew-servers.json`), `crew_servers_*`, `crew_server_install`, `crew_agent_*` Tauri commands |
| Desktop UI | `apps/desktop/src/lib/crew/` and `apps/desktop/src/lib/stores/crew.svelte.ts` | workspace, board, editor, settings, onboarding, automations, the look catalogue |
| Mobile | `apps/mobile/lib/features/crew/` | crew tab, chat, profile, automations, relay transport |
| Push relay | `services/push-relay/` | the optional relay |

**Wire naming rule.** An operation on one crewmate is `crewmate_<verb>` (`crewmate_create`, `crewmate_task`, `crewmate_triggers_set`); a crew-level operation is `crew_<verb>` (`crew_settings_get`, `crew_github_login_start`, and the phone relay's `crew_servers`, `crew_call`, `crew_event`). Push events follow the same rule (`crewmate_updated`). JSON fields use `crewmate_id`, and lists use `"crewmates"`. Desktop Tauri commands are `crew_servers_*`, `crew_server_install`, `crew_agent_*`. PTY pane keys are `crew:<crewmate id>:<task id>`.

**On disk** (`<hostd dir>` is `~/.atlas` on a Mac, `<crew home>/.atlas` on a crew server):

```text
<hostd dir>/crew.json                  crew-level settings
<hostd dir>/crew/<crewmate id>/
    crewmate.json                      definition (0600)
    tasks.jsonl                        task log, last state per id wins
    memory.md                          memory, 64 KiB max
    session.json                       agent session to resume
    secrets.json                       secrets (0600)
    usage.jsonl                        per-task usage records
    work/                              git worktree of the project
    <trigger id>_seen.jsonl            GitHub events already handled
<hostd dir>/projects/<slug>/           shared clone per Git project
~/.atlas/desktop/crew-servers.json     Atlas Desktop's list of crew servers
```

Tests that pin the contract: Rust tests write the JSON fixtures from the real serializers (`UPDATE_FIXTURES=1 cargo test -p atlas-host`) into `apps/mobile/test/fixtures/` (`crewmate_list.json`, `crewmate_tasks.json`, `crewmate_task_summarized.json`, `crewmate_task_event.json`, `crew_github_status.json`) and `apps/desktop/src/__fixtures__/` (`crewmate_tasks.json`, `crew_telegram_status.json`); the desktop and mobile tests read them, so a field renamed on one side fails on the other. Never edit a fixture by hand.

## Reference

### Environment

| Variable | Where | Effect |
|---|---|---|
| `ATLAS_CREW_HOST=1` | the unit | turns on the crew subsystem; without it every `crewmate_*` and `crew_*` command errors |
| `ATLAS_HOSTD_IDLE_SECS=never` | the unit | disables hostd's idle exit so the service stays up |
| `ATLAS_PUSH_RELAY` | the unit | `off` disables push; a URL selects a self-hosted relay |
| `ATLAS_CREW_BINARY` | your machine | same as `atlas crew install --binary` |
| `ATLAS_CREWMATE_ID` | inside a task | the crewmate running it (read by `atlas-crew-mcp`) |

### The systemd unit

```ini
[Unit]
Description=Atlas Crew (atlas hostd)
After=network-online.target
Wants=network-online.target
[Service]
User=atlas-crew
Environment=ATLAS_HOSTD_IDLE_SECS=never
Environment=ATLAS_CREW_HOST=1
Environment=PATH=<home>/.atlas/bin:<home>/.bun/bin:<home>/.local/bin:<home>/.opencode/bin:<home>/.npm-global/bin:/usr/local/bin:/usr/bin:/bin
ExecStart=/usr/local/lib/atlas-crew/atlas hostd
Restart=always
RestartSec=5
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=<home>
PrivateTmp=true
[Install]
WantedBy=multi-user.target
```

`User=` is `atlas-bot` and `<home>` is `/var/lib/atlas-bot` on a server that already had that user.

### Protocol commands

Same NDJSON socket as every hostd command (`~/.atlas/hostd.sock`, or the crew server's). Clients check `info.caps` for `"crew"` first.

| Command | Args | Reply |
|---|---|---|
| `crewmate_list` | `{}` | `{crewmates:[…]}` |
| `crewmate_create` | `{name, role, agent, approval, workdir?, look?, engine?, project?}` | the crewmate |
| `crewmate_update` | `{id, name?, role?, agent?, approval?, look?, …}` | the crewmate |
| `crewmate_delete` | `{id}` | `{}`; cancels running and queued tasks first |
| `crewmate_task` | `{id, prompt, source, tz?}` | `{task}` |
| `crewmate_tasks` | `{id, limit?}` | `{tasks:[…]}` newest first, default 50 |
| `crewmate_cancel` | `{id, task}` | `{}` |
| `crewmate_answer` | `{id, ask_id, key?, text?}` | answer an `ask_user` question |
| `crewmate_action_answer` | `{id, task, value}` | answer a waiting action: a secret, text, a choice, `add`/`skip` |
| `crewmate_memory`, `crewmate_memory_set` | `{id}`, `{id, text}` | `{text}`, `{}` (64 KiB max) |
| `crewmate_routine_set` | `{id, routines}` | `{}`; replaces the list, invalid cron rejected |
| `crewmate_triggers_set` | `{id, triggers[]}` | `{}`; see [Automations](#automations) |
| `crewmate_secret_set` | `{id, key, value}` | `{}` |
| `crewmate_usage` | `{id?, since?}` | usage records |
| `crew_telegram_status`, `crew_telegram_set`, `crew_telegram_link` | `{}`, `{token?}`, `{}` | `{configured, chats}`, `{}`, `{code}` (desktop only; the token is never returned) |
| `crew_settings_get`, `crew_settings_set` | `{short_replies, short_replies_model?}` | crew settings |
| `crew_github_status`, `_login_start`, `_login_poll`, `_logout`, `_connect_resume` | `{}` | see [GitHub](#github) |
| `push_register`, `push_unregister` | `{token, platform:"fcm"}`, `{token}` | `{}` |
| `mobile_invite`, `mobile_invites`, `mobile_invite_dismiss` | `{url, name}`, `{}`, `{url}` | desktop-only invites to phones (TTL 120 s) |
| `crew_servers`, `crew_call`, `crew_event` | `{}`, `{server, cmd, args}`, push | phone to crew server relay |

### Crewmate object

```json
{
  "id": "a1b2c3d4",
  "name": "Reviewer",
  "role": "Review PRs and leave inline comments.",
  "agent": "claude",
  "approval": "ask",
  "state": "idle",
  "queued": 0,
  "running_task": { "id": "e5f6a7b8", "pane": "crew:a1b2c3d4:e5f6a7b8" },
  "last": { "text": "Done. Left 3 comments.", "state": "done", "ts": 1729612800 },
  "pending_approval": { "req": "r1", "tool_name": "Bash", "tool_input": {}, "pane_key": "crew:a1b2c3d4:e5f6a7b8" },
  "triggers": [
    {
      "id": "t1", "kind": { "type": "schedule", "cron": "0 9 * * 1-5" },
      "enabled": true, "prompt": "Review the open PRs and summarize.",
      "tz": "America/Sao_Paulo", "last_fired": 1729612800, "next_fire": 1729699200
    }
  ]
}
```

- `state`: `idle`, `working` or `needs_you`.
- `running_task` exists only while a task runs; `pane` is the key to attach a terminal.
- `last`: the most recent task's result, error or prompt (first 140 characters), final state and time.
- `pending_approval` exists only in `needs_you`.
- `triggers` is always present; `next_fire` is computed by the server.

### Task object

```json
{
  "id": "e5f6a7b8",
  "ts": 1729612800,
  "source": "mobile",
  "prompt": "Review the open PRs.",
  "state": "done",
  "pane": "crew:a1b2c3d4:e5f6a7b8",
  "result": "Left 3 comments on #42.",
  "error": null,
  "started_at": 1729612801,
  "finished_at": 1729612900,
  "steps": [{ "ts": 1729612820, "text": "Reading PR diff…" }]
}
```

- `source`: `mobile`, `desktop`, `telegram`, `routine` or `trigger`.
- `state`: `queued`, `running`, `waiting_action`, `done`, `failed` or `cancelled`.
- `steps` may be empty; `started_at` and `finished_at` are absent until reached. Other optional fields: `reply`, `reply_full`, `summarized`, `title`, `preparing`, `ask`, `action`, `answer`, `usage`, `trigger_id`, `tz`.
