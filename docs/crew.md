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

Sections: [Install a crew server](#install-a-crew-server) · [Back up and move crewmates](#back-up-and-move-crewmates) · [Sign agents in](#sign-agents-in) · [Crewmates](#crewmates) · [Tasks and approvals](#tasks-and-approvals) · [Automations](#automations) · [GitHub](#github) · [Telegram](#telegram) · [Push notifications](#push-notifications) · [Desktop](#atlas-desktop) · [Mobile](#atlas-mobile) · [MCP tools](#mcp-tools-for-the-agent) · [Security model](#security-model) · [Troubleshooting](#troubleshooting) · [Crewmate look](#crewmate-look) · [Code map](#code-map) · [Reference](#reference)

## Install a crew server

```sh
# 1. One time, needs sudo on the server
atlas crew install ubuntu@myserver.example.com

# 2. Sign each agent in as the crew user (see below)
atlas crew login ubuntu@myserver.example.com claude

# 3. Add the server in Atlas Desktop (Atlas Crew → Settings), or reach it from Atlas Mobile through your Mac
```

Requirements:

- A Linux server, x86_64 or aarch64 (arm64: Graviton, Oracle A1, Hetzner CAX), with `systemd` and `sudo` (passwordless sudo when you run the command from a terminal). Any other architecture is refused with a message saying so.
- An SSH public key at `~/.ssh/id_ed25519.pub` or `~/.ssh/id_rsa.pub` on your machine.
- Internet access on the server, or a Linux `atlas` of the server's architecture on your machine (see [Offline install](#offline-install)).

`atlas crew install <ssh-target>` does, in order: checks the host is Linux (x86_64 or aarch64) with systemd and sudo; installs `atlas` to `/usr/local/lib/atlas-crew/`; creates the `atlas-crew` system user (home `/var/lib/atlas-crew`); links the binary at `<home>/.atlas/bin/atlas`; writes and enables `/etc/systemd/system/atlas-crew.service`; appends your SSH public key to the crew user's `authorized_keys`. The service starts on boot and restarts on failure.

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
| `atlas crew install <ssh-target> [--yes] [--port N] [--binary PATH]` | install or migrate a crew server (above), x86_64 or aarch64. `--port` for a non-standard SSH port; `--binary` uploads that Linux `atlas` (the server's architecture) instead of downloading this version's release (also read from `ATLAS_CREW_BINARY`) |
| `atlas crew update <ssh-target> [--version X] [--rollback] [--port N] [--binary PATH]` | put a release (default: this CLI's version) on a crew server, keep the old binary as `atlas.prev`, restart, wait for hostd, roll back by itself when it does not come back; `--rollback` restores `atlas.prev` ([Update a crew server](#update-a-crew-server)) |
| `atlas crew uninstall <ssh-target> [--yes]` | lists what it will delete (the crew home, the number of crewmates and its size), stops and removes the service, the crew user (and its home, so your crewmates) and `/usr/local/lib/atlas-crew`. In a terminal it offers an export first and asks you to type the server name; `--yes` skips the questions but still prints what was deleted and that it is gone; with neither a terminal nor `--yes` it refuses. Refuses while the user still has a login session; it never kills processes |
| `atlas crew export <ssh-target> [-o FILE] [--crewmate ID]… [--with-secrets]` | writes one archive of the crewmates ([Back up and move crewmates](#back-up-and-move-crewmates)) |
| `atlas crew import <ssh-target> <FILE> [--rename]` | restores an archive into a server |
| `atlas crew status <ssh-target>` | `systemctl status` of the service plus `atlas hostd --status` as the crew user, then one line with the server's atlas version: up to date, **behind** this CLI (run `atlas crew update`), ahead, or unknown |
| `atlas crew pause <ssh-target>` | the kill switch: cancels every running task, holds the queues, skips automations. Runs `atlas hostd --crew-pause` on the server as the crew user ([Pause and resume](#pause-and-resume)) |
| `atlas crew resume <ssh-target>` | starts the queues again (`atlas hostd --crew-resume`) |
| `atlas crew logs <ssh-target> [-f]` | last 200 lines of `journalctl -u atlas-crew`; `-f` follows |
| `atlas crew login <ssh-target> <agent>` | opens `ssh -t <crew-user>@<host> <agent-binary>` so you finish the agent's own sign-in. `<agent>` is `omp`, `claude`, `codex` or `opencode`; the command prints the install line for that agent first |

### Offline install

If the server has no internet, put a Linux `atlas` of the server's architecture on your machine at `~/.atlas/remote/atlas-x86_64-linux` (x86_64) or `~/.atlas/remote/atlas-aarch64-linux` (arm64), or pass it with `--binary`, then run `atlas crew install`. When `curl` fails on the server, the command uploads that cached binary. The downloaded release is checked against its `.sha256` before it is installed.

## Update a crew server

```sh
atlas crew update ubuntu@myserver.example.com                 # to this CLI's version
atlas crew update ubuntu@myserver.example.com --version 1.6.0 # to a specific release
atlas crew update ubuntu@myserver.example.com --rollback      # back to the binary before the last update
```

`atlas crew status <ssh-target>` ends with the server's atlas version and says when it is **behind** this CLI (or ahead of it, or unknown because hostd is not running). That is the cue to update.

`atlas crew update` needs the server already installed (`atlas crew install`), passwordless sudo and the same Linux x86_64 or aarch64 host as install. In order, on the server:

1. Downloads the release asset for the server's architecture (`atlas-<version>-x86_64-unknown-linux-gnu.tar.gz` or `…-aarch64-unknown-linux-gnu.tar.gz`), checks its `.sha256` and stages it as `/usr/local/lib/atlas-crew/atlas.new`. With no internet on the server it uploads the [offline](#offline-install) binary instead; `--binary PATH` uploads that file (no version check then: any hostd that answers is accepted).
2. Copies the running `atlas` to `atlas.prev`, moves `atlas.new` over `atlas` and restarts `atlas-crew.service`.
3. Waits up to 60 s for `atlas hostd --status` to answer as the crew user.
4. If hostd does not answer, **restores `atlas.prev` and restarts again by itself**, then exits with an error saying it rolled back (exit code of the command is non-zero). If even the old binary does not come up it says so and points at `atlas crew logs`.

`--rollback` does steps 2 and 3 with `atlas.prev` explicitly; the binary it replaces becomes the new `atlas.prev`, so running it twice goes back to where you were. It fails with a clear message when there is no `atlas.prev`.

Only `/usr/local/lib/atlas-crew/` changes. The crew home (crewmates, memory, secrets, `crew.json`) is never touched, and the update never calls pause or resume: the paused flag lives in `crew.json` and hostd reads it again on start, so **a paused crew stays paused**. The restart is a daemon restart: a task that was running is recorded as failed (`interrupted by daemon restart`) and queued tasks are picked up again when hostd is back. Pause the crew first (`atlas crew pause`) if you do not want a task cut in the middle.

### arm64 servers

Graviton, Oracle Ampere A1 and Hetzner CAX servers work the same way: `atlas crew install` and `atlas crew update` read `uname -m` on the server (`x86_64` or `aarch64`/`arm64`) and download the matching asset. Any other architecture is refused before anything changes. The aarch64 build is the CLI only (it is what a crew server runs); it is built on Ubuntu 22.04, so it runs on glibc 2.35 or newer (Ubuntu 22.04, Debian 12).

## Back up and move crewmates

```sh
atlas crew export ubuntu@myserver.example.com                      # atlas-crew-myserver.example.com-<date>.tar.gz here
atlas crew export ubuntu@myserver.example.com -o aria.tar.gz --crewmate <id> --with-secrets
atlas crew import ubuntu@newserver.example.com aria.tar.gz [--rename]
```

`export` runs `atlas hostd --crew-export` on the server as the crew user and streams one `.tar.gz` back over SSH. It reads the files directly, so it works with the service stopped, and it never overwrites a local file (`-o` names another). The command reads the archive back and lists the crewmates in it. A server whose `atlas` predates this feature says so; run `atlas crew install <ssh-target>` to update it.

| In the archive | Notes |
|---|---|
| `manifest.json` (always first) | `format` (currently `1`), `atlas_version`, `created_at`, whether it has secrets, and the crewmates' ids and names |
| `crew.json` | the crew-wide settings; `paused` is never carried |
| `crew/<id>/crewmate.json` | definition and automations, written in the current shape (a server installed as Atlas Bot exports in the new names) |
| `crew/<id>/memory.md`, `tasks.jsonl`, `usage.jsonl` | memory, task history, usage |
| `crew/<id>/secrets.json` | **only with `--with-secrets`** |

- **Secrets are opt-in.** Without `--with-secrets` the archive has no `secrets.json`, and a webhook automation (whose token is a credential) arrives with its token cleared and **disabled**; create a new webhook after the import. With `--with-secrets` the file holds credentials in clear text: it is written with mode 0600 and the command says so. Guard it and delete it when done.
- **Never exported:** the `work/` checkout, the saved agent session (`session.json`), the GitHub "already seen" files, and the agents' sign-in files (they live outside the crew data). After an import, run `atlas crew login <ssh-target> <agent>` on the new server.
- **Across servers.** The archive is plain `tar.gz` of text, so it moves between x86_64 and arm64 servers and between Atlas versions that read the same `format`.

`import` checks the manifest on your machine first, then sends the file; the server writes nothing until every check has passed.

- **Unknown format.** An archive with a `format` this version does not read is refused whole ("update atlas").
- **Id collisions.** A crewmate id that already exists on the server is refused, and nothing is imported. `--rename` gives colliding crewmates new ids; an after-crewmate automation that followed one of them follows its new id. Imported automations start fresh (`last_fired` cleared).
- **Settings.** `crew.json` is restored only on a server that has none: a running crew keeps its own settings and its pause state.
- **Safe paths.** An entry that is not `manifest.json`, `crew.json` or `crew/<id>/<one of the files above>` (a `..`, an absolute path, nesting) refuses the whole archive.

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
- **approval**: `ask` (hold each tool call for your answer) or `auto` (allow all without prompting). Where `ask` works depends on how the crewmate runs. In a terminal (no `engine` set), Claude Code and Codex ask through their hook and omp through an approval extension Atlas writes into the crewmate's directory (fail-closed: an error or timeout denies the call). With a headless `engine`, only `claude` and `omp` can ask; `codex` and `opencode` engines run unattended, so the server refuses `ask` for them (`approval_supported: false` in `crewmate_list`).
- **project**: a Git URL (cloned once per server and checked out as a per-crewmate worktree) or a path on the server. Default working directory: `<crewmate dir>/work`.
- **memory**: a markdown file (up to 64 KiB) the agent reads and edits across tasks.
- **look**: see [Crewmate look](#crewmate-look).
- **automations**: see [Automations](#automations). Older scheduled routines (5-field cron) still run.

Crew-level settings (`crew_settings_get` / `crew_settings_set`): `short_replies` (default on) summarizes long replies, optionally with `short_replies_model`. Stored in `<hostd dir>/crew.json`.

A new crewmate starts with a short setup conversation (project, role, engine sign-in) in its chat.

## Tasks and approvals

A task is one prompt. Sources: `mobile`, `desktop`, `telegram`, `routine`, `trigger`. A crewmate runs one task at a time in a pty; the rest queue. A task ends `done`, `failed` or `cancelled`, with a hard cap of 2 hours. Prompts are 1 to 20000 characters (counted as characters, not bytes; longer ones are refused, also from Telegram), and names and roles are limited the same way. While it works, the steps the agent reports show live in the chat, grouped under the title it set for itself.

A task can also stop and wait for **you** (`waiting_action`): connect GitHub, provide a secret, answer a question, choose an option or sign an engine in. Answers go through `crewmate_action_answer`; a secret is written to the crewmate's `secrets.json` and never stored in the task. An agent proposes an action by ending its turn with a line `ATLAS_ACTION: {...}`.

### Approvals

With `approval: ask`, a tool call pauses the task and waits up to 30 minutes for `agent_approve {pane, req, allow}`; the pane key is `crew:<crewmate id>:<task id>`. The wait only starts when someone can answer now: a connected desktop client, a connected phone, or a linked Telegram chat. A registered push token alone does not count (that phone may be offline). Otherwise the prompt is denied at once, so set one of them up before relying on an `ask` crewmate. Claude Code and Codex: re-run `atlas setup claude-code|codex --apply` once so their hook uses the matching 1805 s timeout.

### Pause and resume

A crew server has one kill switch. **Pause** (`crew_pause`) cancels every running task (state `cancelled`, error `paused`), keeps queued tasks queued, starts nothing new, and skips every automation that would fire (a skipped automation is not queued for later). New tasks still queue while paused. **Resume** (`crew_resume`) starts the queues again.

Trigger it from the CLI (`atlas crew pause|resume <ssh-target>`), Atlas Desktop (footer menu **Pause all** / **Resume all**, and Settings → Atlas Crew per server) or Atlas Mobile (Crew tab, needs the `act` scope). The state is saved in `<hostd dir>/crew.json` (`"paused": true`), so a restart of the service keeps the crew paused. Clients read it from `crewmate_list.paused` and the `crew_paused` push event; a server older than this feature sends neither and reads as running.

### What survives a restart

Restarting the service (an upgrade, a reboot, a crash) loses nothing a user started:

| Task was | After the restart |
|---|---|
| `queued` | still queued, in order |
| `running`, crewmate with an engine | back at the head of its queue, same id, marked `resumed`; the agent continues its saved session with a prompt saying the previous run was interrupted |
| `running`, agent in a terminal | back at the head of its queue, same id, marked `resumed`; the agent runs the task again |
| `waiting_action` | still waiting with its card; the answer (`crewmate_action_answer`) still continues it |
| holding an `ask_user` question | the question is dropped (its agent is gone) and a step says so |

A resumed task carries `resumed: true`, a `restarts` count and a step "The server restarted; running this task again". After 2 restarts a task is not resumed again: the third one fails it with an error that names the restarts, so a task that takes the server down cannot loop. A paused crew resumes nothing until **Resume**. Pending tool approvals do not survive: the prompt belonged to the dead agent process.

## Automations

Automations (`triggers` on the wire) start a task by themselves. Build one in a crewmate's settings (Automations), or ask in chat ("every weekday at 9, review the open PRs") and the crewmate proposes it as a card you add with one click.

| Kind | JSON `type` | Fields |
|---|---|---|
| Schedule | `schedule` | `cron`: 5 fields, evaluated in the automation's `tz` (UTC when unset) |
| Interval | `interval` | `every_secs` ≥ 60 |
| Webhook | `webhook` | `token`, called as `POST /crew/<crewmate id>/hook/<token>` on the hook server (webhooks set up before the rename keep working at `/bots/<crewmate id>/hook/<token>`) |
| GitHub | `github` | `repo` (`owner/repo`), `events`: `mention`, `issue_assigned`, `pr_review_comment`, `ci_failed`, `pr_opened`, `pr_review_requested`, `issue_opened`, `release`, `authors`: who may start a task, `handle` (for `mention`) and `assignee` (for `issue_assigned`), see [GitHub in and out](#github-in-and-out). Polled every 2 minutes with `gh` |
| File | `file` | `path`, optional `glob` |
| After crewmate | `crewmate` | `from_crewmate` (id), `on`: `done` or `failed` |
| Service | `service` | `project_dir`, `service`, `on`: `failed` or `recovered` |

Rules the server enforces:

- Every automation has an `enabled` flag, a `prompt` (it may contain `{{event}}`) and an optional IANA `tz` (`"America/Sao_Paulo"`); an unknown zone is rejected with `unknown timezone: <tz>`.
- `crewmate_triggers_set` replaces the whole list. An existing `id` keeps its `last_fired`; a **new** schedule or interval starts from now, so it first fires at the next real time, not on the next 30 s tick.
- `next_fire` is computed by the server on every `crewmate_list`; a client value is ignored.
- **GitHub author filter.** Anyone who can open an issue or PR on a watched repo controls the text of the event, so a GitHub automation only fires for authors whose `author_association` is in `authors`. The default is `["owner", "member", "collaborator"]`; add `"contributor"` or others only for repos where you trust them. Events from anyone else are skipped.
- **Tasks from outside ask.** A task started by a **GitHub** or **webhook** automation runs with `approval: ask` even on an `auto` crewmate, because its text comes from outside: a tool call waits for your answer. Set `allow_auto: true` on that automation (default `false`) to keep `auto`, only where strangers cannot write the event. Schedule, interval, file, after-crewmate and service automations keep the crewmate's own approval, so unattended routines keep running. A crewmate that cannot ask (a `codex` or `opencode` engine) refuses to run a GitHub or webhook task without `allow_auto`.
- **Untrusted content.** `{{event}}` is text from outside (a GitHub title, a webhook body). Before it reaches the agent, hostd wraps it in an untrusted-content block and tells the agent not to follow instructions inside it. This lowers the risk but does not remove it, which is why the author filter and `ask` exist.
- A paused crew skips automations ([Pause and resume](#pause-and-resume)).
- `crewmate_task` takes the caller's `tz`; it is stored on the task and stamped on any automation proposed in it.
- An agent proposes an automation with `ATLAS_ACTION: {"kind":"add_trigger","trigger":{...},"prompt":"...","reason":"..."}`. An invalid one resolves the task with `The automation was invalid: <error>` so the agent can retry. Otherwise a card shows **Add** / **Not now**, answered with `crewmate_action_answer {id, task, value: "add"|"skip"}`. Webhooks are not offered to agents.

The Crew board (below) shows every queued, working, waiting and finished task across crewmates.

## GitHub

Crewmates that clone private repositories or open PRs need `gh` signed in on the crew server. In Atlas Desktop, Atlas Crew → Settings → GitHub starts a device-code login (`crew_github_login_start`, then `crew_github_login_poll`); hostd installs `gh` under the crew user's home if it is missing and configures git to use it. `crew_github_status` reports `{installed, login}`, `crew_github_logout` removes it. A task that needs GitHub waits in a **Connect GitHub** action and resumes when the login completes (`crew_github_connect_resume`).

### GitHub in and out

A crewmate can take work from GitHub and answer there, the way the Claude GitHub app does, from your own server. Everything goes through `gh` as the crew user, with the login above; nothing is sent to CodeAtlas.

**Set it up.** Sign `gh` in (above), then add a GitHub automation to the crewmate (Settings → Automations → GitHub, or `crewmate_triggers_set`) with the repo and the events you want:

```json
{"id": "gh", "enabled": true, "prompt": "Handle this: {{event}}",
 "kind": {"type": "github", "repo": "owner/repo",
          "events": ["mention", "issue_assigned", "pr_review_comment", "ci_failed"],
          "handle": "nami", "assignee": "nami-bot", "authors": ["owner", "member", "collaborator"]}}
```

The crew polls the repo's events every 2 minutes, so a comment starts a task within about 2 minutes.

| Event | Starts a task when | The prompt carries |
|---|---|---|
| `mention` | `@<handle>` appears in an issue or PR comment, or in the body of an issue that is opened or edited. `handle` defaults to the crewmate's name as a slug (`Code Reviewer` → `code-reviewer`); matching is whole-word and ignores case (`@namibia` and `a@nami.com` do not match) | the number, title and body of the issue or PR, and the comment |
| `issue_assigned` | an issue is assigned to `assignee` (a GitHub login, required for this event) | the issue number, title and body |
| `pr_review_comment` | someone comments on a PR's diff | the PR, the file, the line and the comment; the task works on that PR's branch |
| `ci_failed` | a check run fails | the check, its branch and PR, and the end of the failed job's log (`gh run view --log-failed`, at most 200 lines and 32 KiB; without the log the failure is still reported) |

`pr_opened`, `pr_review_requested`, `issue_opened` and `release` still work as before.

**What it posts.** When the task starts, the crewmate comments "Working on it." on the issue or PR where it was asked, then **edits that same comment**: a checklist of the steps the agent reports (at most one edit every 30 seconds, the latest 15 steps, with no links, images or `@` mentions in it), and at the end the result, the link of the PR it opened (found with `gh pr list --head <branch>`, for the branch the agent moved to or the PR's own branch), or the error if the task failed or was cancelled. A task whose comment cannot be posted (GitHub down, no permission) still runs; reporting is best effort. Every comment starts with a hidden marker (`<!-- atlas-crew -->`) so it never starts a task by itself. A CI failure on a branch without a PR has nowhere to reply, so it posts nothing.

**Where a task came from.** The task records it as structured fields, `origin: {repo, kind, number, comment_id, branch, reply_comment_id}` (`kind`: `mention`, `issue_assigned`, `ci_failed`, `pr_review_comment`), in `crewmate_tasks`; nothing is parsed back out of the prompt.

**Security.**

- The text of an issue, a comment, a review comment or a log is written by other people. It reaches the agent only inside the untrusted-content block ([Automations](#automations)), after the instructions.
- **`authors` applies to everyone whose text reaches the prompt:** the author of the issue or PR *and* the commenter. A collaborator's mention on a stranger's issue does nothing. The default is `["owner", "member", "collaborator"]`.
- GitHub's events feed carries no `author_association`, so hostd asks GitHub for the issue, PR or comment itself. If that lookup fails, the event is not decided and not marked seen: it is asked again on the next poll, never allowed.
- The branch of a CI-fix or review-comment task is put in the instructions ("check out this branch, push there") only when it is a plain git ref name (letters, digits, `.`, `_`, `/`, `-`); any other name is left to the untrusted block.
- A task from GitHub runs with `approval: ask` even on an `auto` crewmate unless the automation sets `allow_auto: true`. Set it only on repos where strangers cannot comment.
- The crew's own comments never start a task, by its `gh` login and by the marker. The same event, or the same comment seen again, never starts a second task (`<crewmate dir>/<automation id>_seen.jsonl`).
- A paused crew ([Pause and resume](#pause-and-resume)) does not poll GitHub at all.

## Telegram

One Telegram bot per crew server; every linked chat sees all of its crewmates.

1. Create a Telegram bot with [@BotFather](https://t.me/BotFather) and copy its token.
2. In Atlas Desktop, **Settings → Atlas Crew**, under the server (it must be online), paste the token in **Telegram** and click **Save** (`crew_telegram_set`). The token is write-only: the row shows only whether one is set and how many chats are linked (`crew_telegram_status`).
3. Click **Link chat** to get a 6-digit code valid 10 minutes (`crew_telegram_link`), and send your bot `/start <code>` in Telegram. Repeat for each chat. **Lockout:** the fifth wrong code (from any chat) kills the live code, even the right one is then refused; click **Link chat** again for a new one. Four typos do not.
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

- After pairing, push is automatic: the app gets a **grant** for its FCM token from the relay (`POST /v1/register`) and registers `{token, grant}` with each crew server (`push_register`, `push_unregister`). It does so on every launch and connect and whenever FCM refreshes the token.
- The relay at `push.codeatlas.com.br` gets the token, the grant and the event kind (`task_done` or `needs_you`), never content. Phones that are connected at that moment are skipped.
- The relay only notifies a caller that holds the token's grant, so knowing a token is not enough to ping a phone. A crew server drops a token the relay refuses (`403`) or that FCM no longer has (`410`).
- **Updating from an earlier alpha:** a token registered before grants existed has no grant. The crew server keeps it but sends nothing for it; open the updated app once and it registers again with a grant. Until then that phone gets no push.
- Disable: set `Environment=ATLAS_PUSH_RELAY=off` in the unit, then `sudo systemctl daemon-reload && sudo systemctl restart atlas-crew`. Or point it at a self-hosted relay (`services/push-relay/`, see its README for `RELAY_GRANT_KEY`); build the app with `--dart-define=ATLAS_PUSH_RELAY=<url>` to match.

## Atlas Desktop

Open the Atlas Crew workspace with **⌘⇧B** (command *Atlas Crew*, remappable). Two views, switched with **⌘⇧C** or the bar at the top edge:

- **Chat**: the crewmate list (search, ⌘1…⌘9 to switch, server status in the footer) and the selected crewmate's thread: tasks, live steps, inline approval and action cards, and a composer that queues while another task runs. A settings window (General, Automations, Engine, Memory, Usage, Audit) saves each field on its own. A paused server shows a **Paused** banner over the workspace with a **Resume** button; the footer menu has **Pause all** / **Resume all**.
- **Board**: Queued, Working, Needs you and Done columns across every crewmate, with filters by crewmate, origin and text, a Done range, and a click that opens the chat at that task.

Settings → Atlas Crew holds the crew servers (SSH target + name, listed in `~/.atlas/desktop/crew-servers.json`). The desktop reaches each server over SSH and an `ssh -L` tunnel to its `hostd.sock`. The server must already have `atlas crew install` done; Atlas Desktop can run the install for you and asks for the sudo password on stdin (never on argv).

## Atlas Mobile

The phone talks only to its paired Macs; the Mac's hostd relays to the crew servers in the desktop list. Details and scopes: [Atlas Mobile → Crew on the phone](mobile.md#crew-on-the-phone).

The **Crew** tab lists every crewmate across paired computers (state dot, last message, time). Each crew server has a **Pause all** button, and a **Crew paused** banner with **Resume** while it is paused. Crewmates waiting for you appear in a "needs you" card at the top of the Computers and Crew tabs. A crewmate opens as a chat (prompt and reply bubbles, steps collapsed, inline Allow / Deny, composer that queues) and a profile screen: role, approval, automations, memory (View / Edit), delete.

When you add a Mac in Atlas Desktop, phones get a `mobile_invite` push (`computer_invite` event) and show a **Pair** sheet; dismissing it calls `mobile_invite_dismiss`.

## MCP tools for the agent

Each crewmate's agent gets a stdio MCP server, `atlas-crew-mcp` (`atlas mcp --crewmate <id>`; `--bot` is a hidden alias), registered for the task. Environment: `ATLAS_HOOK_PORT`, `ATLAS_HOOK_TOKEN`, `ATLAS_CREWMATE_ID`, `ATLAS_PANE_KEY`.

| Tool | Use |
|---|---|
| `ask_user` | post a question to Desktop, Mobile or Telegram and block until answered (up to 30 min). Backed by `POST /crew/<id>/ask` |
| `set_profile` | change this crewmate's own name or role. Backed by `POST /crew/<id>/set_profile` |
| `approve` | answer a Claude permission prompt (`--permission-prompt-tool atlas:approve`) |

The hook routes live on the same local HTTP server as the agent hooks and need its token.

## One Linux user per crewmate

Issue [#200](https://github.com/codeatlasdev/atlas/issues/200). `atlas crew install` (re-run it on an existing server) sets up two socket-activated root services, both `atlas crew-spawn`, both reachable only by the crew user (socket mode 0660, group = crew user, and a `SO_PEERCRED` check):

| service | does | limits |
|---|---|---|
| `atlas-crew-spawn` | starts an engine binary (`claude`, `codex`, `omp`, `opencode`, by bare name, from a pinned directory) as `atlas-cm-<id>`, hands hostd the pty or a socketpair, signals it, reports its exit and working directory, ends its leftovers | `CAP_SETUID/SETGID/CHOWN/KILL/DAC_OVERRIDE`, read-only filesystem except `/var/lib/atlas-cm`; every request is validated (user shape, binary, cwd inside the crewmate's directory, no `LD_*`/`PATH`/`HOME` in env, signal list) |
| `atlas-crew-provision` | creates and removes a crewmate's user and directories, rewrites the egress rules | two operations, no free text (a user of the exact `atlas-cm-<12 hex>` shape); writes `/etc` and `/var/lib/atlas-cm` only |

hostd stays the unprivileged crew user under `NoNewPrivileges`. It asks the helper for each process and gets the pty master back over `SCM_RIGHTS`.

Layout under `/var/lib/atlas-cm/<id>/`:

| path | owner | holds |
|---|---|---|
| `home/` | crewmate, 0700 | `$HOME`: the agents' sign-ins |
| `work/` | crewmate, 2770 (setgid) | the checkout (a full clone, since a worktree would point into the crew home); hostd writes it through the group |
| `crew-runtime/` | crew user, group crewmate, 0750 | what the agent reads and cannot change: MCP config, omp approval extension, hook endpoint file, git config |
| `memory.md` | crewmate, 0660 | the agent appends, hostd edits it for the apps |

Other behavior with isolation on:

- **Hook endpoint.** The hook server listens on one loopback port the firewall names (47631; random when taken, and isolated crewmates then cannot report). Each crewmate has its own token (`<crewmate dir>/hook-token`, in its environment and `crew-runtime/hook-endpoint.env`) that opens only its own routes: its own `/crew/<id>/ask` and `set_profile`, and hook events for its own panes. Another crewmate's route is `403`, the webhook route is never open to an agent, a wrong token is `401`. The global token is unchanged and is not given to isolated agents.
- **Egress.** One nftables chain per crewmate user (`meta skuid`): replies, DNS, the hook port on loopback, then a drop for the rest of loopback, `10/8`, `172.16/12`, `192.168/16`, `169.254/16` (cloud metadata), `100.64/10`, `fc00::/7`, `fe80::/10`, then TCP 22/80/443, then drop. The file is `/etc/atlas-crew/egress.nft`, replayed at boot by `atlas-crew-egress.service`. The policy is the same for every crewmate today (`Policy` supports per-user ports and extra destinations; there is no setting for it yet).
- **Engines** are installed system-wide with each vendor's own install line under `/usr/local/lib/atlas-crew/engine-home` and linked into `/usr/local/lib/atlas-crew/engines`, the first directory the helper searches. Updating an engine is the vendor's own update, run as root there.
- **Create and delete.** `crewmate_create` creates the user first and saves nothing if that fails (for example out of system uids: `useradd --system` has about 900). `crewmate_delete` ends its processes and removes the user and directory. A crewmate that exists without a user (imported, or made before the install) gets one when its first task starts.
- **Sign in** as the crewmate: `atlas crew login <server> <agent> --crewmate <id>` (over ssh as your admin user, then `sudo -u atlas-cm-<id>`). Hook registration (`atlas setup <agent> --apply`) is also per user.
- **Migration.** Existing crewmates get users during the install. Old checkouts are renamed `work.pre-isolation` (they were worktrees of a clone in the crew home that the crewmate's user cannot read; the next task clones again), `memory.md` moves on the first task (the old file is kept as `memory.md.pre-isolation`), and agents must be signed in again per crewmate (their shared sign-in stays with the crew user). Nothing is deleted.
- **Export** follows the memory into the crewmate's directory. Agent sign-ins and checkouts were never exported.

Not covered: the GitHub login (`gh`) is still the crew user's and shared; per-crewmate egress allowlists have no setting; the engines in `engine-home` are shared and root-installed.

## Security model

- **Dedicated user.** The crew user (`atlas-crew`, or the reused `atlas-bot`) has no sudo; it writes only its home and what agents create in `workdir`.
- **Systemd hardening.** `NoNewPrivileges=true`, `ProtectSystem=strict`, `ProtectHome=read-only` with `ReadWritePaths=<crew home>`, `PrivateTmp=true`, `ProtectKernelTunables=true`, `ProtectControlGroups=true`, `RestrictSUIDSGID=true`, `MemoryMax=80%`, `TasksMax=4096` ([the unit](#the-systemd-unit)). With the helper installed each crewmate has its own user ([below](#one-linux-user-per-crewmate)); without it they still share the crew user.
- **One Linux user per crewmate** ([details](#one-linux-user-per-crewmate)). Each crewmate runs as `atlas-cm-<id>`: the others cannot read its `secrets.json`, sign-ins or checkout, and an egress policy limits where it connects. Active only on a server where `atlas crew install` set up the helper; otherwise every crewmate shares the crew user as before.
- **Scoped SSH key.** `atlas crew install` appends your key to the crew user's `authorized_keys` only, never to the admin user's.
- **sudo once.** Install and uninstall need sudo; the running service never does. The sudo password, when Desktop passes it, comes over stdin and is dropped after the credential cache is primed.
- **Secrets.** Per-crewmate secrets live in `<crewmate dir>/secrets.json` (mode 0600), are injected as environment into that crewmate's agent only and are never returned in any reply. Terminals and secrets are never relayed to a phone.
- **Audit.** Every crewmate has an append-only log, written by hostd and never by the agent: each tool call the agent reports (full tool name and input, not the 80-character step), each approval asked and how it ended (`allow`, `deny` or `timeout`, and who answered: `desktop`, `phone:<node id>`, `telegram:<chat id>` or `none`), the task lifecycle, and changes to its approval mode, triggers, project, engine and secrets (a secret's name, never its value). See [Audit](#audit).
- **No CodeAtlas servers.** The only optional outbound call is the push relay (kind only). Details: [PRIVACY.md](https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md).

## Audit

What a crewmate did and who decided what, kept on the crew server in `<crewmate dir>/audit.jsonl` (mode 0600, one JSON object per line: `seq`, `ts`, `kind`, `crewmate_id`, plus the fields of the kind). Crew-wide events (a crewmate created or deleted, the crew paused or resumed) go to `<hostd dir>/crew-audit.jsonl`, which outlives a deleted crewmate.

| `kind` | Written when | Fields |
|---|---|---|
| `tool_call` | the agent reports a tool call (terminal agents and headless engines alike) | `task_id`, `tool_name`, `tool_input` |
| `approval_asked` | an `ask` crewmate needs a decision | `task_id`, `req`, `tool_name`, `tool_input` |
| `approval_decision` | the prompt ended | `task_id`, `req`, `tool_name`, `decision` (`allow`/`deny`/`timeout`), `by` |
| `task_queued`, `task_started`, `task_finished` | task lifecycle | `task_id`, `source`, `prompt`; `state` and `error` at the end |
| `config_changed` | approval, engine, project, agent, name, role length, short replies, triggers, routines | `field`, `from`, `to` (webhook tokens are left out) |
| `secret_set` | `crewmate_secret_set` | `name` |
| `audit_gap` (crew-level) | the agent hub dropped events under load | `dropped` |

Crew-level kinds: `crewmate_created`, `crewmate_deleted`, `paused`, `resumed`, `audit_gap`.

**Who answered.** `desktop` is any client on the server's socket, which includes the CLI and, for a crew server, the Mac's relay: an approval a phone sends through the relay to a crew server reads `desktop` there. A phone connected straight to a hostd is `phone:<node id>`; Telegram is `telegram:<chat id>`; a prompt nobody answered in 30 minutes is `timeout` / `none`.

**Redaction.** The values of the crewmate's secrets and the hook token are replaced by `[secret:NAME]` / `[secret:hook_token]` before anything is written (the secrets are read on every write, so one set a minute ago is covered; values under 4 characters are not treated as secrets). A string over 8 KiB is cut with a marker that says how many bytes there were.

**Bounded.** A file rotates at 1 MiB (`audit.jsonl.1` … `.4`); the oldest of five is dropped, so a log never exceeds about 5 MiB. A line that cannot be written is logged (`atlas crew logs`), never silently skipped. If the agent hub drops events under load, an `audit_gap` entry says so.

Read it with `crewmate_audit` and `crew_audit` (Mobile scope `read`); Atlas Desktop shows it under a crewmate's settings → **Audit** (filter by tool calls, approvals, tasks or settings; copy one entry or everything shown). Atlas Mobile has no audit screen.

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `crewmate_*` commands answer `this computer is not a crew server (atlas crew install)` | hostd runs without `ATLAS_CREW_HOST=1`. The unit sets it; do not run `atlas hostd` by hand on a crew server |
| `info.caps` has no `"crew"` | same cause; clients check this before showing the Crew UI |
| Approval prompt is denied at once | nobody can answer right now: no connected desktop client or phone, and no linked Telegram chat ([Approvals](#approvals)) |
| An `ask` crewmate never asks | check its approval really is `ask` (a GitHub or webhook task asks too, see [Automations](#automations)), and that someone can answer ([Approvals](#approvals)). `codex` and `opencode` engines cannot ask, so `ask` is refused for them |
| `could not connect … over SSH` | SSH itself failed (refused, timeout, key rejected), not the server's OS; fix SSH first |
| `sudo … asks for a password` | grant passwordless sudo, or run the install from Atlas Desktop |
| Uninstall says the user is still logged in | close Atlas Desktop and any ssh session as the crew user, run it again |
| Uninstall says it needs a decision | no terminal and no `--yes`: run it in a terminal, or pass `--yes` after `atlas crew export` |
| Export or import says the server's atlas is too old | `atlas crew install <ssh-target>` updates the server's binary, then retry |
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
| hostd (crew server and Mac) | `crates/atlas-host/src/crew/` | command routing (`mod.rs`) to `handlers/` (crewmates, tasks, settings); `hub.rs` (the crew hub); `store/` (on-disk types, the old-layout migration, `archive.rs`: the export/import archive); `runner/` (one task: queue, `pty`, `conversation`, `prompt`, `action`, `outcome`, `summary`, `recover` for tasks a restart interrupted); `events.rs` (every push event, one builder each); `pane.rs` (the `crew:` pane key); `engine/` (omp, claude, codex, opencode); `triggers/` and `schedule.rs` (automations); `github/`; `telegram.rs`; `push.rs`; `project.rs` (checkout); `mcp.rs` (`atlas-crew-mcp`); `relay/` (phone to crew server); `tests/` (integration tests by area, shared helpers in `tests/fixture.rs`) |
| hook routes | `crates/atlas-host/src/agents/hook_server.rs` | `/crew/<id>/ask`, `/crew/<id>/set_profile`, `/crew/<id>/hook/<token>` |
| CLI | `crates/atlas-cli/src/commands/crew/` | `atlas crew install\|update\|uninstall\|export\|import\|status\|logs\|login`; the unit text, migration plan, release asset per architecture, update/rollback scripts and the health decision (`binary.rs`), and the export/import/uninstall scripts and decisions (`transfer.rs`) are pure functions with tests |
| Desktop backend | `crates/atlas-desktop/src/crew.rs` | the server registry (`crew-servers.json`), `crew_servers_*`, `crew_server_install`, `crew_agent_*` Tauri commands |
| Desktop UI | `apps/desktop/src/lib/crew/` and `apps/desktop/src/lib/stores/crew.svelte.ts` | workspace, board, editor, settings, onboarding, automations, the look catalogue |
| Mobile | `apps/mobile/lib/features/crew/` | crew tab, chat, profile, automations, relay transport |
| Push relay | `services/push-relay/` | the optional relay |

**Wire naming rule.** An operation on one crewmate is `crewmate_<verb>` (`crewmate_create`, `crewmate_task`, `crewmate_triggers_set`); a crew-level operation is `crew_<verb>` (`crew_settings_get`, `crew_github_login_start`, and the phone relay's `crew_servers`, `crew_call`, `crew_event`). Push events follow the same rule (`crewmate_updated`). JSON fields use `crewmate_id`, and lists use `"crewmates"`. Desktop Tauri commands are `crew_servers_*`, `crew_server_install`, `crew_agent_*`. PTY pane keys are `crew:<crewmate id>:<task id>`.

**On disk** (`<hostd dir>` is `~/.atlas` on a Mac, `<crew home>/.atlas` on a crew server):

```text
<hostd dir>/crew.json                  crew-level settings
<hostd dir>/crew-audit.jsonl            crew-wide audit (crewmates created/deleted, pause/resume)
<hostd dir>/crew/<crewmate id>/
    crewmate.json                      definition (0600)
    tasks.jsonl                        task log, last state per id wins
    memory.md                          memory, 64 KiB max
    session.json                       agent session to resume
    secrets.json                       secrets (0600)
    usage.jsonl                        per-task usage records
    audit.jsonl (+ .1 … .4)            append-only audit, rotated at 1 MiB
    work/                              git worktree of the project
    <trigger id>_seen.jsonl            GitHub events already handled
<hostd dir>/projects/<slug>/           shared clone per Git project
~/.atlas/desktop/crew-servers.json     Atlas Desktop's list of crew servers
```

Tests that pin the contract: Rust tests write the JSON fixtures from the real serializers (`UPDATE_FIXTURES=1 cargo test -p atlas-host`) into `apps/mobile/test/fixtures/` (`crewmate_list.json`, `crewmate_tasks.json`, `crewmate_task_summarized.json`, `crewmate_task_resumed.json`, `crewmate_task_event.json`, `crew_github_status.json`) and `apps/desktop/src/__fixtures__/` (`crewmate_tasks.json`, `crew_telegram_status.json`); the desktop and mobile tests read them, so a field renamed on one side fails on the other. Never edit a fixture by hand.

## Reference

### Environment

| Variable | Where | Effect |
|---|---|---|
| `ATLAS_CREW_HOST=1` | the unit | turns on the crew subsystem; without it every `crewmate_*` and `crew_*` command errors |
| `ATLAS_HOSTD_IDLE_SECS=never` | the unit | disables hostd's idle exit so the service stays up |
| `ATLAS_PUSH_RELAY` | the unit | `off` disables push; a URL selects a self-hosted relay (the app's build must use the same one) |
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
ProtectHome=read-only
ReadWritePaths=<home>
PrivateTmp=true
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictSUIDSGID=true
MemoryMax=80%
TasksMax=4096
[Install]
WantedBy=multi-user.target
```

`User=` is `atlas-bot` and `<home>` is `/var/lib/atlas-bot` on a server that already had that user.

What the hardening does, and what it costs (every directive exists in systemd 245, Ubuntu 20.04, the oldest server the installer accepts):

| Directive | Effect |
|---|---|
| `ProtectSystem=strict` | the whole filesystem is read-only except `ReadWritePaths` and `/dev`, `/proc`, `/sys` |
| `ProtectHome=read-only` | `/home`, `/root` and `/run/user` are read-only: a crewmate cannot change other users' files there |
| `ReadWritePaths=<home>` | the crew home stays writable, wherever it is. Everything agents install lives inside it and is on `PATH` (`~/.atlas`, `~/.bun`, `~/.local`, `~/.opencode`, `~/.npm-global`), so agents keep working |
| `ProtectKernelTunables=true`, `ProtectControlGroups=true` | `/proc/sys`, `/sys` and the cgroup tree are read-only |
| `RestrictSUIDSGID=true` | the service cannot create setuid or setgid files |
| `MemoryMax=80%` | the service (hostd and every agent under it) is killed by the kernel past 80 % of the server's memory, instead of taking the host down. Needs cgroup v2 (the default since Ubuntu 21.10, Debian 11); on cgroup v1 the limit is ignored |
| `TasksMax=4096` | at most 4096 processes and threads, which stops a fork bomb. A very parallel build can hit it: raise it with a drop-in (`systemctl edit atlas-crew`) |

To change a limit without reinstalling, use a drop-in; `atlas crew install` rewrites the unit file but not `/etc/systemd/system/atlas-crew.service.d/`.

### Protocol commands

Same NDJSON socket as every hostd command (`~/.atlas/hostd.sock`, or the crew server's). Clients check `info.caps` for `"crew"` first.

| Command | Args | Reply |
|---|---|---|
| `crewmate_list` | `{}` | `{crewmates:[…], paused}` |
| `crew_pause`, `crew_resume` | `{}` | `{}`; see [Pause and resume](#pause-and-resume). Pushes `{"ev":"crew_paused","paused":bool}` (Mobile scope `act`) |
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
| `crewmate_audit` | `{id, since?, limit?, kind?}` | `{entries:[…]}` newest first, default 200; `since` unix seconds. Mobile scope `read` |
| `crew_audit` | `{since?, limit?, kind?}` | the crew-wide log, same shape |
| `crew_telegram_status`, `crew_telegram_set`, `crew_telegram_link` | `{}`, `{token?}`, `{}` | `{configured, chats}`, `{}`, `{code}` (desktop only; the token is never returned) |
| `crew_settings_get`, `crew_settings_set` | `{short_replies, short_replies_model?}` | crew settings |
| `crew_github_status`, `_login_start`, `_login_poll`, `_logout`, `_connect_resume` | `{}` | see [GitHub](#github) |
| `push_register`, `push_unregister` | `{token, platform:"fcm", grant}`, `{token}` | `{}`; `grant` is what the relay issued for the token (optional on the wire for apps older than relay auth, which are then not notified) |
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
- `steps` may be empty; `started_at` and `finished_at` are absent until reached. Other optional fields: `reply`, `reply_full`, `summarized`, `title`, `preparing`, `ask`, `action`, `answer`, `usage`, `trigger_id`, `tz`, `resumed` (a restart interrupted it and it started again) and `restarts` (how many).
