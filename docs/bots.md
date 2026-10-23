# Atlas Crew

Atlas Crew lets you run AI agents 24/7 on your own server without your laptop. You install `atlas hostd` as a systemd service on a Linux box; agents run under a dedicated `atlas-bot` user, fully isolated from other workloads.

## Quick start

```sh
# 1. Install the bot computer (one-time, needs sudo on the server)
atlas bot install ubuntu@myserver.example.com

# 2. Sign each agent in as atlas-bot
atlas bot login ubuntu@myserver.example.com claude
atlas bot login ubuntu@myserver.example.com omp

# 3. Add the computer in Atlas Desktop → Settings → Bots, or use Atlas Mobile
```

## Requirements

- A Linux x86_64 server with `systemd` and `sudo`
- An SSH key in `~/.ssh/id_ed25519` or `~/.ssh/id_rsa` on your local machine
- Internet access on the server (to download the atlas binary), or a cached copy at `~/.atlas/remote/atlas-x86_64-linux`

## Commands

### `atlas bot install <ssh-target> [--yes]`

Turns a Linux server into a bot computer:

1. Verifies the host is Linux x86_64 with `systemd` and `sudo`
2. Downloads and installs `atlas` to `/usr/local/lib/atlas-bot/`
3. Creates the `atlas-bot` system user with home at `/var/lib/atlas-bot`
4. Writes and enables `/etc/systemd/system/atlas-bot.service`
5. Appends your SSH public key to `/var/lib/atlas-bot/.ssh/authorized_keys`

The service starts automatically on boot. The `atlas-bot` user has no sudo.

### `atlas bot uninstall <ssh-target> [--yes]`

Removes the service, the `atlas-bot` user and `/usr/local/lib/atlas-bot` from the server. Asks for confirmation unless `--yes` is given.

### `atlas bot status <ssh-target>`

Shows `systemctl status atlas-bot` and `atlas hostd --status` (as `atlas-bot`).

### `atlas bot logs <ssh-target>`

Streams the last 200 lines of `journalctl -u atlas-bot`.

### `atlas bot login <ssh-target> <agent>`

Opens an interactive `ssh -t atlas-bot@<host> <agent-bin>` session so you can complete the agent's own sign-in flow. The agents themselves are installed by you as `atlas-bot`; see install lines below.

## Installing agents on the bot computer

After `atlas bot install`, SSH in as `atlas-bot` and install whichever agents you plan to use:

```sh
ssh atlas-bot@myserver.example.com
```

Then inside that shell:

| Agent | Install command |
|---|---|
| **omp** | `curl -fsSL https://omp.sh/install \| sh` |
| **Claude Code** | `npm install -g @anthropic-ai/claude-code` |
| **Codex** | `npm install -g @openai/codex` |
| **OpenCode** | `curl -fsSL https://opencode.ai/install \| sh` |

After installing, run `atlas bot login <ssh-target> <agent>` to complete the sign-in (API keys, OAuth, etc.).

## The systemd unit

The service runs as the unprivileged `atlas-bot` user with several systemd hardening options:

```ini
[Unit]
Description=Atlas Crew (atlas hostd)
After=network-online.target
Wants=network-online.target

[Service]
User=atlas-bot
Environment=ATLAS_HOSTD_IDLE_SECS=never
Environment=ATLAS_BOT_HOST=1
ExecStart=/usr/local/lib/atlas-bot/atlas hostd
Restart=always
RestartSec=5
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/lib/atlas-bot
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

`ATLAS_HOSTD_IDLE_SECS=never` disables the idle-exit that the daemon applies by default, so the service stays up even when no tasks are queued.

`ATLAS_BOT_HOST=1` enables the bots subsystem. Without it every `bot_*` command returns an error and the `"bots"` capability does not appear in `info`. This variable is set automatically by the unit; you do not need to set it yourself.

## Bot management

Bots are managed through the hostd protocol, Atlas Desktop, or Atlas Mobile. Each bot has:

- **name** (1–40 chars)
- **role** (≤ 2000 chars) — prepended to every task as context
- **agent** — one of `omp`, `claude`, `codex`, `opencode`
- **approval** — `ask` (hold for your answer) or `auto` (allow all tool calls without prompting). omp bots always use `auto` because omp's hook protocol does not support remote decisions.
- **workdir** — where the agent runs; defaults to `<bot-home>/work`
- **memory** — a markdown file (≤ 64 KiB) the agent reads and edits across tasks
- **routines** — scheduled tasks (5-field cron, server's local time)

### Approvals

When `approval: ask`, an approval prompt pauses the task and waits up to 30 minutes for an answer. The wait proceeds only when at least one of these is true:

- A desktop client (unix socket) is connected
- A push token is registered (phone exists)
- A Telegram chat is linked

If none of those are true the prompt times out immediately, so configure at least one before creating an `ask` bot.

Approval answers use `agent_approve {pane, req, allow}`. The `pane_key` for a bot task is `bot:<bot_id>:<task_id>`.

## Security model

- **Dedicated user.** `atlas-bot` is a system user with no sudo and no login shell. It can only write to its own home (`/var/lib/atlas-bot`) and the paths agents create inside `workdir`.
- **Systemd hardening.** `NoNewPrivileges`, `ProtectSystem=strict`, and `PrivateTmp` prevent the service from escalating privileges or leaking state across restarts.
- **SSH key scoped to atlas-bot.** `atlas bot install` appends your public key to `atlas-bot`'s `authorized_keys` only, not to the admin user's.
- **sudo required once.** Installing the unit and the binary needs sudo, but the running service never does.
- **No CodeAtlas servers.** All agents run on your own subscription or API key. The only optional outbound call is the push relay (`push.codeatlas.com.br`), which forwards only an event kind (never content). Set `ATLAS_PUSH_RELAY=off` to disable it entirely.

## Offline install

If the server has no internet access, populate the local cache first on a connected machine:

```sh
atlas update  # ensures ~/.atlas/remote/atlas-x86_64-linux is up to date
```

Then run `atlas bot install` normally. When `curl` fails on the server, the command falls back to uploading the cached binary automatically.

## Protocol commands

All commands go over the same NDJSON socket as other hostd commands (`~/.atlas/hostd.sock` or the bot computer's socket). Snake-case command names, JSON arguments and replies. Every `bot_*` command requires `ATLAS_BOT_HOST=1`; otherwise the reply is `{"ok":false,"error":"this computer is not a bot server (atlas bot install)"}`.

Clients detect bot-server support by checking `info.caps` for `"bots"` before calling these commands.

| Command | Args | Reply |
|---|---|---|
| `bot_list` | `{}` | `{bots:[…]}` — see shape below |
| `bot_create` | `{name, role, agent, approval, workdir?}` | the bot object |
| `bot_update` | `{id, name?, role?, agent?, approval?}` | the bot object |
| `bot_delete` | `{id}` | `{}` — cancels running and queued tasks first |
| `bot_task` | `{id, prompt, source}` | `{task}` — prompt 1..=20 000 chars |
| `bot_tasks` | `{id, limit?}` | `{tasks:[…newest first]}` — default limit 50 |
| `bot_cancel` | `{id, task}` | `{}` |
| `bot_memory` | `{id}` | `{text}` |
| `bot_memory_set` | `{id, text}` | `{}` — ≤ 64 KiB |
| `bot_routine_set` | `{id, routines}` | `{}` — replaces the whole list; invalid cron is rejected |
| `push_register` | `{token, platform:"fcm"}` | `{}` |
| `push_unregister` | `{token}` | `{}` |
| `bot_telegram_set` | `{token?}` | `{}` — set or clear; desktop only |
| `bot_telegram_link` | `{}` | `{code}` — 6-digit code valid 10 min |
| `mobile_invite` | `{url, name}` | `{}` — desktop only; pushes `computer_invite` event to phones |
| `mobile_invites` | `{}` | `{invites:[{url,name,created_at}]}` — non-expired pending invites (TTL 120 s) |
| `mobile_invite_dismiss` | `{url}` | `{}` — phone tells the Mac it dismissed the invite |

### `bot_list` bot object shape

```json
{
  "id": "a1b2c3d4",
  "name": "Reviewer",
  "role": "Review PRs and leave inline comments.",
  "agent": "claude",
  "approval": "ask",
  "state": "idle",
  "queued": 0,
  "running_task": { "id": "e5f6a7b8", "pane": "bot:a1b2c3d4:e5f6a7b8" },
  "last": { "text": "Done. Left 3 comments.", "state": "done", "ts": 1729612800 },
  "pending_approval": { "req": "r1", "tool_name": "Bash", "tool_input": {}, "pane_key": "bot:a1b2c3d4:e5f6a7b8" }
}
```

- `state`: `idle` | `working` | `needs_you`
- `running_task`: present only when a task is running; `pane` is the pane key for terminal attachment
- `queued`: number of tasks waiting in the queue behind the running one
- `last`: the most recent task's result/error/prompt (first 140 chars), its final state and timestamp
- `pending_approval`: the pending `agent_approval` event; present only in `needs_you` state

### Task object shape

```json
{
  "id": "e5f6a7b8",
  "ts": 1729612800,
  "source": "mobile",
  "prompt": "Review the open PRs.",
  "state": "done",
  "pane": "bot:a1b2c3d4:e5f6a7b8",
  "result": "Left 3 comments on #42.",
  "error": null,
  "started_at": 1729612801,
  "finished_at": 1729612900,
  "steps": [
    { "ts": 1729612820, "text": "Reading PR diff…" }
  ]
}
```

- `source`: `mobile` | `desktop` | `telegram` | `routine`
- `state`: `queued` | `running` | `done` | `failed` | `cancelled`
- `steps`: intermediate messages the agent emitted during the task (may be empty)
- `started_at` / `finished_at`: Unix seconds; absent until the task reaches that milestone

## Atlas Desktop

Open the Crew workspace with **⌘⇧B** (command `atlas::ToggleBotsWorkspace`, remappable). This switches the app between the IDE view and the Crew view.

The Crew workspace has two columns:

- **Left column**: search box, list of all crewmates across bot servers, ship status in the footer. Click a ship dot or "No ship yet · Launch" to open **Settings → Crew**.
- **Right panel**: chat view for the selected crewmate, or the Logbook when none is selected.

**Settings → Crew** is where you add bot servers (SSH target + name). Each server must already have `atlas bot install` completed.

The chat view shows the task thread (oldest at top, newest at bottom), an inline approval card when `needs_you`, and a text composer that queues tasks even while another is running. A profile panel (⋯ menu) lets you edit the role, change the approval setting, manage routines and read or replace the memory file.

## Atlas Mobile

The **Crew** tab (bottom bar) lists every crewmate across all paired computers in a messenger-style list. Each row shows the crewmate's name, agent, server, last message preview, time, and a state dot.

Crewmates whose `state` is `needs_you` appear as a card at the top of both the Computers and Crew tabs until the approval is answered.

Tapping a bot opens the chat view:

- Task thread with user prompt bubbles and bot reply bubbles (steps collapsed by default, tap to expand)
- Inline approval card when the bot is waiting — **Allow** / **Deny** calls `agent_approve`
- Composer at the bottom; queues tasks while one is running
- Tap the bot name in the app bar to open the **profile screen**

The profile screen covers:

- **What it does** (role) — tap Edit to change
- **Approval** — switch between `ask` and `auto`
- **Routines** — list, toggle enabled/disabled, add (cron + prompt), delete
- **Memory** — line count with a View / Edit button (full-screen text editor, saves with `bot_memory_set`)
- **Delete bot** — destructive, asks for confirmation

### Computer invite flow

When you add a new Mac in Atlas Desktop → **Computers**, the Mac sends a `mobile_invite` push (`computer_invite` event) to all connected phones. A sheet appears on the phone with the computer's name and a **Pair** button. Tapping it completes pairing using the invite URL. Dismissing the sheet calls `mobile_invite_dismiss`, which removes the pending invite from the Mac.

## Telegram

Link a Telegram chat to your bot computer:

1. Create a Telegram bot at [t.me/BotFather](https://t.me/BotFather) and copy the token.
2. In Atlas Desktop → Bots workspace → your bot computer → Settings → Telegram, paste the token.
3. Click **Link chat**. A 6-digit code is shown (`bot_telegram_link`).
4. Start a chat with your Telegram bot and send `/start <code>`.
5. The chat is now linked. Any linked chat receives task results and can create tasks.

**Commands available in linked chats:**

| Command | What it does |
|---|---|
| `/bots` | List all bots and their current state |
| `/task <bot name> <text>` | Create a task for the named bot |
| plain text | Sent to the only bot when exactly one exists; otherwise prompts for `/task` |

Approval prompts appear as inline keyboard messages (Allow / Deny). The callback calls `agent_approve` on the bot's pane.

## Push notifications

Atlas Crew can push a notification to your phone when a task finishes or a bot needs your approval.

- After pairing your phone, push is automatic: the mobile app registers its FCM token with each bot server (`push_register`).
- The relay at `push.codeatlas.com.br` forwards only the event kind (`task_done` or `needs_you`), never any content.
- Notifications are sent only to phones that are not actively connected at the time.
- To disable: set `ATLAS_PUSH_RELAY=off` in the `[Service]` section of `/etc/systemd/system/atlas-bot.service` and run `sudo systemctl daemon-reload && sudo systemctl restart atlas-bot`.
- Self-hosters can point `ATLAS_PUSH_RELAY` at their own relay instance (see `services/push-relay/`).
