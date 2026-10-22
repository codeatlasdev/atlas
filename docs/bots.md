# Atlas Bot

Atlas Bot lets you run AI agents 24/7 on your own server without your laptop. You install `atlas hostd` as a systemd service on a Linux box; agents run under a dedicated `atlas-bot` user, fully isolated from other workloads.

## Quick start

```sh
# 1. Install the bot computer (one-time, needs sudo on the server)
atlas bot install ubuntu@myserver.example.com

# 2. Sign each agent in as atlas-bot
atlas bot login ubuntu@myserver.example.com claude
atlas bot login ubuntu@myserver.example.com omp

# 3. Add the computer in Atlas Desktop → Bots, or use Atlas Mobile
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
5. Adds your SSH public key to `/var/lib/atlas-bot/.ssh/authorized_keys`

The service starts automatically on boot. The `atlas-bot` user has no sudo.

### `atlas bot uninstall <ssh-target> [--yes]`

Removes the service, the `atlas-bot` user and `/usr/local/lib/atlas-bot` from the server. Asks for confirmation unless `--yes` is given.

### `atlas bot status <ssh-target>`

Shows `systemctl status atlas-bot` and `atlas hostd --status` (as `atlas-bot`).

### `atlas bot logs <ssh-target> [--follow]`

Streams `journalctl -u atlas-bot`. Add `--follow` / `-f` to tail live output.

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
Description=Atlas Bot (atlas hostd)
After=network-online.target
Wants=network-online.target

[Service]
User=atlas-bot
Environment=ATLAS_HOSTD_IDLE_SECS=never
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

`ATLAS_HOSTD_IDLE_SECS=never` disables the 10-minute idle-exit that the daemon applies by default, so the service stays up even when no tasks are queued.

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

All commands go over the same NDJSON socket as other hostd commands (`~/.atlas/hostd.sock` or the bot computer's socket). Snake-case command names, JSON arguments and replies.

| Command | Args | Reply |
|---|---|---|
| `bot_list` | `{}` | `{bots:[{id,name,role,agent,approval,state,running_task?,pending_approval?}]}` |
| `bot_create` | `{name,role,agent,approval,workdir?}` | the bot object |
| `bot_update` | `{id,name?,role?,agent?,approval?}` | the bot object |
| `bot_delete` | `{id}` | `{}` |
| `bot_task` | `{id,prompt,source}` | `{task}` — prompt 1..=20 000 chars |
| `bot_tasks` | `{id,limit?}` | `{tasks:[…newest first]}` — default limit 50 |
| `bot_cancel` | `{id,task}` | `{}` |
| `bot_memory` | `{id}` | `{text}` |
| `bot_memory_set` | `{id,text}` | `{}` — ≤ 64 KiB |
| `bot_routine_set` | `{id,routines}` | `{}` — replaces the whole list; invalid cron is rejected |
| `push_register` | `{token,platform:"fcm"}` | `{}` |
| `push_unregister` | `{token}` | `{}` |
| `bot_telegram_set` | `{token?}` | `{}` — set or clear; desktop only |
| `bot_telegram_link` | `{}` | `{code}` — 6-digit code valid 10 min |

Approval prompts use the existing `agent_approve` command; `bot_list` exposes `pending_approval` for each bot that is waiting.

## Telegram

Link a Telegram chat to your bot computer:

1. Create a Telegram bot at [t.me/BotFather](https://t.me/BotFather) and copy the token.
2. In Atlas Desktop → Bots → your bot computer → Telegram, paste the token and click **Link chat**.
3. A 6-digit code is shown. Start a chat with your bot and send `/start <code>`.
4. The chat is now linked. Any linked chat receives task results and can create tasks.

**Commands available in linked chats:**

| Command | What it does |
|---|---|
| `/bots` | List all bots and their current state |
| `/task <bot name> <text>` | Create a task for the named bot |
| plain text | Sent to the only bot when exactly one exists; otherwise prompts for `/task` |

Approval prompts appear as inline keyboard messages (Allow / Deny).

## Push notifications

Atlas Bot can push a notification to your phone when a task finishes or a bot needs your approval.

- After pairing your phone in Atlas Desktop → Settings → Mobile, push is automatic: Atlas registers your FCM token with each bot computer.
- The relay at `push.codeatlas.com.br` forwards only the event kind (`task_done` or `needs_you`), never any content.
- To disable: set `ATLAS_PUSH_RELAY=off` in `/etc/systemd/system/atlas-bot.service` and restart.
- Self-hosters can point `ATLAS_PUSH_RELAY` at their own relay instance.
