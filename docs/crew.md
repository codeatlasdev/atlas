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

`atlas crew install <ssh-target>` does, in order: one preflight probe that changes nothing and names **every** missing piece at once (systemd running as PID 1, sudo, curl, tar, git, sha256sum, awk, useradd, ... each with the package that has it); stages `atlas` as `/usr/local/lib/atlas-crew/atlas.new`; creates the `atlas-crew` system user (home `/var/lib/atlas-crew`); links the binary at `<home>/.atlas/bin/atlas`; writes and enables `/etc/systemd/system/atlas-crew.service`; swaps the binary in (a binary that was already there stays as `atlas.prev`), starts the service and **waits up to 60 s for hostd to answer**, rolling back to `atlas.prev` when it does not (a dead binary fails the install and points at `atlas crew logs`, it is never reported as ready); appends your SSH public key to the crew user's `authorized_keys`. The service starts on boot and restarts on failure.

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
| `atlas crew install <ssh-target> [--yes] [--port N] [--binary PATH] [--browsers]` | install or migrate a crew server (above), x86_64 or aarch64. `--port` for a non-standard SSH port; `--binary` uploads that Linux `atlas` (the server's architecture) instead of downloading this version's release (also read from `ATLAS_CREW_BINARY`); `--browsers` also installs the capture browsers ([Captures](#captures)) |
| `atlas crew update <ssh-target> [--version X] [--rollback] [--port N] [--binary PATH]` | put a release (default: this CLI's version) on a crew server, keep the old binary as `atlas.prev`, restart, wait for hostd, roll back by itself when it does not come back; `--rollback` restores `atlas.prev` ([Update a crew server](#update-a-crew-server)) |
| `atlas crew uninstall <ssh-target> [--yes] [--port N]` | lists what it will delete (the crew home, the number of crewmates and its size), stops and removes the service, the crew user (and its home, so your crewmates) and `/usr/local/lib/atlas-crew`. In a terminal it offers an export first and asks you to type the server name; `--yes` skips the questions but still prints what was deleted and that it is gone; with neither a terminal nor `--yes` it refuses. Refuses while the user still has a login session; it never kills processes |
| `atlas crew export <ssh-target> [-o FILE] [--crewmate ID]… [--with-secrets] [--port N]` | writes one archive of the crewmates ([Back up and move crewmates](#back-up-and-move-crewmates)) |
| `atlas crew import <ssh-target> <FILE> [--rename] [--port N]` | restores an archive into a server |
| `atlas crew status <ssh-target> [--port N]` | `systemctl status` of the service plus `atlas hostd --status` as the crew user, then one line with the server's atlas version: up to date, **behind** this CLI (run `atlas crew update`), ahead, or unknown |
| `atlas crew pause <ssh-target> [--port N]` | the kill switch: cancels every running task, holds the queues, skips automations. Runs `atlas hostd --crew-pause` on the server as the crew user ([Pause and resume](#pause-and-resume)) |
| `atlas crew resume <ssh-target> [--port N]` | starts the queues again (`atlas hostd --crew-resume`) |
| `atlas crew logs <ssh-target> [-f] [--port N]` | last 200 lines of `journalctl -u atlas-crew`; `-f` follows |
| `atlas crew login <ssh-target> <agent> [--port N]` | opens `ssh -t <crew-user>@<host> <agent-binary>` (with the same `PATH` the Desktop's engine check uses: `~/.local/bin`, `~/.bun/bin`, `~/.npm-global/bin`, so an agent installed there is found) so you finish the agent's own sign-in. `<agent>` is `omp`, `claude`, `codex` or `opencode`; the command prints the install line for that agent first |
| `atlas crew browsers install <ssh-target> [--port N]` | installs Playwright's chromium and webkit for the `capture` tool ([Captures](#captures)); idempotent. Nothing from npm runs as root: sudo is used only for one `apt-get install` that atlas builds itself |
| `atlas crew browsers status <ssh-target> [--port N]` | one line per missing piece (node older than 20 or missing, npm, runner, Playwright package or version, each browser, a browser that cannot start for lack of libraries); says it is ready and exits 0 when nothing is missing, exits 1 otherwise |

### One SSH path

Every ssh call to a crew server (every `atlas crew` command, Atlas Desktop's engine check, agent install and sign-in terminal, the hostd tunnel and the relay Atlas Mobile uses) is built by one helper, `atlas_core::ssh::SshTarget`, from the server's address and port. A non-22 port is given once: `--port N` on any `atlas crew` command, or at install in Atlas Desktop, which keeps it with the server in `~/.atlas/desktop/crew-servers.json` (`"port": 2222`; no `port` means 22, and a port set in `~/.ssh/config` still applies to a server added before the field existed). The agents' dirs (`~/.local/bin`, `~/.bun/bin`, `~/.opencode/bin`, `~/.npm-global/bin`) are put on the remote `PATH` for the check, the install, `atlas crew login` and the sign-in terminal, so an agent installed by its own installer is found in a non-interactive ssh command.

Windows clients: OpenSSH for Windows has no connection sharing, so `ControlMaster`/`ControlPath`/`ControlPersist` are only passed on macOS and Linux (every call there is its own handshake); a remote script is sent with no double quote in the command line, which Windows' argument parsing would mangle. The SSH key onboarding looks for is the one `ssh -G` lists for the host (`~/.ssh/config` `IdentityFile`s and the default names), not only `id_ed25519` and `id_rsa`.

### Offline install

If the server cannot download the release (no internet, or behind a proxy it has no settings for), `atlas crew install` and `atlas crew update` download it **on your machine** (with your `https_proxy` and curl settings), check its `.sha256`, and send the binary over ssh. Only if that fails too does it use a Linux `atlas` you put at `~/.atlas/remote/atlas-x86_64-linux` (x86_64) or `~/.atlas/remote/atlas-aarch64-linux` (arm64); `--binary` always wins. The downloaded release is checked against its `.sha256` before it is installed.

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

5. **Refreshes the capture runner**, after the new hostd is up, on a server that already has the [capture browsers](#captures) (`<hostd dir>/capture/runner.mjs` exists): `runner.mjs` and `detectors.mjs` are rewritten as the crew user, the same way `atlas crew browsers install` writes them. No root, no npm, no browser download; the package and its lock are not touched (they move with `atlas crew browsers install`, which `atlas crew browsers status` asks for when the pinned Playwright differs). A server without the capture dir is left alone, and `--rollback` keeps the runner it has. Without this step a new hostd would ask an old runner for the [UI detectors](#ui-detectors-temporary), get no findings and fail every UI gate scenario; if that ever happens the error says `this server's capture runner predates the UI detectors` and names the fix (`atlas crew update` or `atlas crew browsers install`).

Only `/usr/local/lib/atlas-crew/` changes, plus the two capture runner files above when they exist. The crew home (crewmates, memory, secrets, `crew.json`) is never touched, and the update never calls pause or resume: the paused flag lives in `crew.json` and hostd reads it again on start, so **a paused crew stays paused**. The restart is a daemon restart: a task that was running is put back at the head of its queue and run again, marked `resumed` (after 2 restarts it fails instead), and queued tasks are picked up again when hostd is back ([What survives a restart](#what-survives-a-restart)). Pause the crew first (`atlas crew pause`) if you do not want a task cut in the middle and run again.

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

Crew-level settings (`crew_settings_get` / `crew_settings_set`): `short_replies` (default on) summarizes long replies, optionally with `short_replies_model`; `max_agents` (default 4, at least 1) is how many agents the server runs at once. Stored in `<hostd dir>/crew.json`.

A new crewmate starts with a short setup conversation (project, role, engine sign-in) in its chat.

## Tasks and approvals

A task is one prompt (and, optionally, [images](#images-on-tasks)). Sources: `mobile`, `desktop`, `telegram`, `routine`, `trigger`. A crewmate runs one task at a time (in a pty, or as headless engine turns); the rest queue. A task waits queued while its crewmate runs another one, and also while the server already runs `max_agents` agents (the task says `waiting_for: "server busy"` and the chat shows it); a task that only waits for you (a GitHub login, an action card) holds no agent slot. Checking the project out is part of the task: every `git` call has a 120 s deadline and is killed with the processes it started, the checkout lock is per repo (two repos check out at once), and cancelling the task during the checkout stops it and never starts the agent. A task ends `done`, `failed` or `cancelled`, with a hard cap of 2 hours (the agent and the processes it started are killed when it passes). Prompts are 1 to 20000 characters (counted as characters, not bytes; longer ones are refused, also from Telegram), and names and roles are limited the same way. While it works, the steps the agent reports show live in the chat, grouped under the title it set for itself.

A task can also stop and wait for **you** (`waiting_action`): connect GitHub, provide a secret, answer a question, choose an option, sign an engine in, decide on work the [guards](#guards-against-the-agent-itself) refused, or show a [UI task](#ui-tasks-and-proof) that came back without proof. Answers go through `crewmate_action_answer`; a secret is written to the crewmate's `secrets.json` and never stored in the task. An agent proposes an action by ending its turn with a line `ATLAS_ACTION: {...}`.

### Guards against the agent itself

A task that ends well is checked before it is `done` (`runner/guard.rs`), because "finished" is also what an agent says when it made the checks pass instead of the code. The guards read the crewmate's work tree and its own words:

| Guard | Refused when |
|---|---|
| Weakened tests | The diff against the task's base (the branch's upstream, else `origin/HEAD`; committed and uncommitted work both count) removes assertions from a test file (`assert!`, `assert_eq!`, `expect(`, `.should`…, counted removed against added), deletes a test file or a test function, adds a skip (`#[ignore]`, `.skip`, `it.only`, `xit`, `skip:`, `@Skip`), or loosens a limit in any file: `timeout`, `threshold`, `budget`, `max_bytes`, `max_warnings`, `waiver(s)`, `tolerance`, `retries`… raised, and `coverage_threshold` / `fail_under` lowered. Rust, TypeScript/JavaScript and Dart |
| Stray files | A new untracked file is a scratch name (`probe*`, `scratch*`, `tmp*`, `temp*`, `debug*`, `repro*`, `playground*`, `*.orig`, `*.rej`, `*.bak`, `*.log`) or sits in a `tmp`/`scratch`/`probes` folder; sits at the repository root where the repo tracks no file of that extension; or sits in a new folder whose parent folder the repo does not track either (a feature may add one folder next to tracked ones, not a tree of them). A new source file next to its siblings is expected, and so is anything under `.atlas/` (Atlas's own files, such as task attachments). A command that redirects or `tee`s into `/tmp` or `/var/tmp` is refused too: git cannot see those files, the tool calls can |
| Invented references | A `#N` in the task's commits, in the reply, or in a command that publishes text (`gh pr`, `gh issue`, `gh api`, `git commit`) does not exist in the project's GitHub repo (checked through the same GitHub API the GitHub triggers use), or could not be checked |
| Unproven claims | Not a refusal: a sentence of the result that says something was verified, tested, passes or compiles is removed, and a note says what was removed, unless a command of this task ran a test or check (`cargo test`, `bun test`, `bun run check`, `flutter test`, `pytest`, `go test`…) and printed output with no failure in it |

Atlas learns what a command printed from the agent's own reports. Claude Code's `post_tool_use` carries it (`tool_response`). omp reports it when a call ends (`tool_execution_end`: the result's text and whether it failed): a terminal omp crewmate sends it through the status extension (`atlas setup omp --apply` rewrites it; before that omp still reports the commands but not their output, so its claims count as unproven), and an omp crewmate with an engine reports it on its RPC stream, which hostd reads itself. A result the tool marked as an error never proves a claim, whatever it printed. Output is kept to its last 16 KiB. A claim whose command has no recorded output counts as unproven. The commands are kept in memory per task and dropped when it ends; after a server restart the evidence is gone and the claim is judged without it. Engines that report nothing through a hook or a stream hostd reads (a headless `codex` or `opencode`, or a `claude` whose hooks `atlas setup claude-code` has not installed) leave no evidence, so their claims are removed.

A refusal parks the task as `waiting_action` with a `refused` action, `{"kind": "refused", "reasons": ["…"]}`, shown by the action card on the desktop and the phone and as a Telegram message with **Override** and **Retry with reasons** buttons. Answer `crewmate_action_answer` with `"override"` (accept the work as it is: the task ends `done`) or `"retry"` (the task runs again from the start with the reasons added to its prompt; you may attach images to a retry, see [UI tasks and proof](#ui-tasks-and-proof)). Any other value is an error and the task keeps waiting. A refusal survives a restart like any other card. An agent cannot raise a `refused` action itself: an `ATLAS_ACTION` line of that kind is ignored. The audit records `task_refused` (with the reasons) and `refusal_overridden`.

### UI tasks and proof

A task marked **`ui`** changes what people see, so it only ends `done` with at least **2 images the agent attached itself** (its `capture` tool, before and after, top and end of the page). An image *you* sent with the prompt never counts: the proof is what the agent looked at. A `ui` task that finishes with fewer parks as `waiting_action` with a `prove_ui` action, `{"kind": "prove_ui", "reason": "Could not prove it: this is a UI task and the agent attached 1 of 2 captures. …"}`, shown on the same card as a refusal. The gate runs after the [guards](#guards-against-the-agent-itself): work the guards refused and you overrode still has to show itself.

Mark a task `ui` with `crewmate_task {ui: true}`, with a leading `/ui ` in the desktop composer or in Telegram (`/ui /task Ana restyle the header`, or `/ui restyle the header` for a single crewmate), or let the agent do it with the `set_ui` MCP tool when the task turns out to change the interface. The mark only turns the rule on: an agent cannot take it off, and cannot raise a `prove_ui` card itself (an `ATLAS_ACTION` line of that kind is ignored). A `/ui` with no message after it is not a task: Telegram answers `Usage: /ui <message>, or /ui /task <crewmate-name> <message>` and the desktop composer says to add a message and keeps what you typed; nothing is queued either way (a photo captioned `/ui` still works, the image is the content).

The card shows the agent's captures (thumbnails you tap to enlarge) and the reason, on Atlas Desktop, Atlas Mobile (the task's card in the chat and the Crew tab's Captain's call) and Telegram (the images as one media group of at most 10, the newest, then the message with the buttons). Answer `crewmate_action_answer` with:

- `"override"`: accept the work as it is; the task ends `done`. Overriding the guards' `refused` card does not waive the proof: a `ui` task that is still unproven then gets its `prove_ui` card, and that card's override is the final word. This holds the same way for a card that was waiting when hostd restarted: after the restart `override` is the owner's explicit choice (audited as `refusal_overridden`) and `retry` runs the task again and checks the proof when it finishes.
- `"retry"`: the task runs again from the start with the reason in its prompt. You can attach new images to a retry (`attachments`, uploaded like a task's, for a screenshot with the broken part circled): they are stored on the task as yours, staged in the agent's work dir and named in its prompt. Images with an `override`, or with an answer to any other card, are refused and the card keeps waiting.

The agent's captures are the task's `attachments` with `"source": "agent"` (yours have no `source`); they are stored through the same code as your images, so the same checks apply (png, jpeg, webp or gif by the bytes). An agent may attach up to 32 per task (one `capture` call makes up to 16); your own cap stays 4. The audit records `task_unproven_ui` (`task_id`, `reason`, `captures`).

### Images on tasks

A task can carry up to **4 images** of at most **5 MiB** each (png, jpeg, webp or gif), for the cases where words fail: a screenshot with the broken part circled, a photo of a screen. Send them from Atlas Desktop (the **+** in the composer, or paste a screenshot), Atlas Mobile (the image button: camera or photo library) or Telegram (send a photo).

- **The bytes decide, not the sender.** The type is read from the file's first bytes: a script named `shot.png`, an SVG or a PDF is refused with its name, whatever mime the client declared. An empty file, more than 4 images or one over 5 MiB refuses the **whole** task (nothing is queued, nothing is left on disk). The file name is reduced to one safe path component (`../../x`, an absolute path or a backslash path cannot leave the task's folder) and made unique (`shot.png`, `shot-2.png`); its extension follows the bytes.
- **Where they live.** On the crew server, under the crewmate: `attachments/<task id>/<name>` (see [layout](#reference)). At each start of the task they are also copied into the work dir as `.atlas/attachments/<task id>/<name>`, and that directory is added to the checkout's local `info/exclude` so an agent's `git add -A` never commits your screenshots (the project's own `.gitignore` is not touched). The prompt tells the agent the paths and to look at them first; that line is written by hostd and comes before the task text, so a hostile task body cannot imitate it.
- **On the wire.** A protocol line is at most 1 MiB in both directions, so an image of at most 256 KiB goes inline in `crewmate_task` (`data_b64`); a bigger one is uploaded first with `crewmate_attachment_put` (pieces of at most 512 KiB, in order, the whole capped at 5 MiB) and named by its `upload` id in `crewmate_task`. The apps read an image back with `crewmate_attachment` in pieces. Both go over the channels that already exist (the desktop's SSH tunnel; the phone through your Mac's `crew_call`). **The push relay never sees an image**: a push carries only its kind.
- **Unfinished uploads are bounded.** An app killed mid-send or a dropped network leaves an upload behind, so the crew server never keeps one for good: an upload with no new piece for **1 hour** is deleted (each piece renews it) at the next upload of that crewmate and when the crew server starts, and a crewmate holds at most **8 unfinished uploads** (2 tasks' worth): a *new* upload past that is refused with `too many unfinished uploads (8); send the task or wait`, while continuing one that exists is always allowed. With the 5 MiB cap per upload that bounds what any client can leave on the disk to 40 MiB per crewmate.
- **Phones.** Fetching an image is `read`, sending one is `act` (the same as sending a task), so a phone granted `act` only can send an image but is told `forbidden: read` when the thread asks for it back, and shows that image as "Tap to retry". Photos are scaled to at most 2048 px and re-encoded at 85 % quality on the phone before they are sent.

### Approvals

With `approval: ask`, a tool call pauses the task and waits up to 30 minutes for `agent_approve {pane, req, allow}`; the pane key is `crew:<crewmate id>:<task id>`. The wait only starts when someone can answer now: a connected desktop client, a connected phone, or a linked Telegram chat. A registered push token alone does not count (that phone may be offline). Otherwise the prompt is denied at once, so set one of them up before relying on an `ask` crewmate. Claude Code and Codex: re-run `atlas setup claude-code|codex --apply` once so their hook uses the matching 1805 s timeout.

### Pause and resume

A crew server has one kill switch. **Pause** (`crew_pause`) cancels every running task (state `cancelled`, error `paused`) and ends its process: a task in a terminal has its terminal killed, and a task with a headless `engine` (`claude`, `codex`, `opencode`, `omp`) has its turn killed together with everything the agent started (its process group, so the tool processes of omp's `bash` calls or a build are gone too). The same holds for cancelling one task and for the 2-hour cap, which fails the task and kills the turn. Pause keeps queued tasks queued, starts nothing new, and skips every automation that would fire (a skipped automation is not queued for later). New tasks still queue while paused. **Resume** (`crew_resume`) starts the queues again.

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
| File | `file` | `path` (watched recursively), optional `glob` (`*.md` matches Markdown files at any depth, `docs/*.md` is relative to `path`; a glob that does not parse matches nothing). A change inside the crewmate's own work tree never fires it |
| After crewmate | `crewmate` | `from_crewmate` (id), `on`: `done` or `failed`. A task started this way remembers the crewmates that led to it (`chain`); a crewmate already in that chain is not started again, so two crewmates triggering each other run one round and stop (audit `chain_refused`) |
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
- A new GitHub automation starts at the head of the feed: the events GitHub still lists when it is created are marked seen and start nothing (the seen file is created then).
- An event is recorded in the seen file before its task is made. If that file cannot be written, nothing fires and the automation is not polled again for 15 minutes, so an unwritable disk never repeats a task every 2 minutes.
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
| `/ui <message>` | the same message for a [UI task](#ui-tasks-and-proof): `/ui /task <name> <text>`, or `/ui <text>` for a single crewmate; also at the start of a photo's caption |
| plain text | goes to the only crewmate when exactly one exists |
| a photo | becomes a task with the image: the caption is the prompt (`/task <name> <text>` picks the crewmate, otherwise the only crewmate takes it; no caption: "Look at the attached image."). Telegram's largest size is downloaded through the Bot API (`getFile`), a file over 5 MiB is refused without being fetched, and the chat is told why when it is refused. See [Images on tasks](#images-on-tasks) |

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

**Screenshots for UI work.** The install screen has an opt-in, off by default: *Screenshots for UI work (Chromium + WebKit)*, with why (the `capture`, UI gate and `locate` tools need it) and the cost (Node.js 20+ on the server, about 500 MB of downloads). Ticking it runs `atlas crew install … --browsers` and adds a step to the install checklist. On an existing server, Settings → Atlas Crew has the same row: the **admin** SSH address you installed with (`ubuntu@host` or `ubuntu@host:2222`) is already there: the install saved it as `admin` in `~/.atlas/desktop/crew-servers.json` (`{"target": "ubuntu@host", "port": 2222}`, no `port` for 22). The browsers install runs `sudo -u <crew user>`, which the crew service user the desktop connects as cannot do, so it needs that account. The field is editable and an edit you commit (leave the field, Enter, or a Check that worked) is saved back; a half-typed address never replaces a good one. A server added before this was kept, or added by hand, starts empty and keeps what you type after a Check that worked. The file holds an address like an `~/.ssh/config` `Host`, never a password or key. Press **Check** (`atlas crew browsers status`: installed, or one line per missing piece) and **Install** (`atlas crew browsers install`, its output streams into a log). A server without Node.js 20 or newer says so plainly and Install stays off until you have installed Node there. When sudo asks for a password the row asks for it and sends it over stdin, like the install.

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
| `set_ui` | mark this task as a [UI task](#ui-tasks-and-proof): it needs two captures the agent took to be done. Backed by `POST /crew/<id>/set_ui` |
| `capture` | screenshots of a URL in real browsers, attached to the task ([Captures](#captures)). Backed by `POST /crew/<id>/capture` |
| `locate` | which route file, component and selector draw a region of a screenshot ([Locate](#locate)). Backed by `POST /crew/<id>/locate` |

| `ui_gate` | the fast UI gate: capture only what a change needs, rerun only failures, replay past bugs ([UI gate](#ui-gate)). Backed by `POST /crew/<id>/ui_gate` |
| `approve` | answer a Claude permission prompt (`--permission-prompt-tool atlas:approve`) |

The hook routes live on the same local HTTP server as the agent hooks and need its token.

## Captures

The `capture` tool lets a crewmate look at a UI instead of guessing: it opens a URL in **Playwright's chromium and webkit**, at several widths and in light and dark, and hands back one PNG per combination. A capture that returns is complete: if any requested combination is missing or failed, the tool returns an error that names each one (`webkit/390px/dark`), never a partial set.

```json
{"url": "http://localhost:5173/settings", "widths": [390, 1440], "themes": ["light", "dark"], "engines": ["chromium", "webkit"], "slow_network": false, "full_page": true, "steps": [{"do": "click", "selector": "#menu"}]}
```

Only `url` is required. Each field and what it means:

| Field | Default | Meaning |
|---|---|---|
| `url` | | `http://` or `https://` only. `localhost` and LAN addresses are allowed on purpose (the page is usually a dev server on the crew server); `file:`, `data:`, `javascript:` and the like are refused |
| `widths` | `[390, 1440]` | viewport widths in px, 200 to 4000: a phone and a desktop |
| `themes` | `["light", "dark"]` | the page's `prefers-color-scheme` |
| `engines` | `["chromium", "webkit"]` | chromium is Chrome/Edge/Android; webkit is Safari, iOS and Tauri's WKWebView, where most "works on my machine" layout bugs live |
| `slow_network` | `false` | every request is delayed 400 ms, on every engine |
| `full_page` | `true` | scroll to the end (so lazy content loads), then shoot the whole page; `false` shoots the first screen |
| `steps` | none | at most 20 things to do before the shot: `click`, `fill`, `press`, `wait_for`, `wait_ms`. Selectors and text only, no script |
| `dom` | `false` | also write the page's element boxes next to each PNG (`<engine>-<width>px-<theme>.dom.json`): per visible element its tag, role, accessible name, visible text (clipped to 120 characters), `data-testid` and other `data-*` attributes, a stable CSS selector (`[data-testid=…]`, then `#id`, then a short `tag:nth-of-type` chain) and `x`, `y`, `w`, `h` in CSS px of the whole page at that width. A cell without its file is a failed cell. The files are kept next to the PNGs and each shot in the answer lists its `dom` (file name) and `dom_path`. This is what [`locate`](#locate) reads |
| `checks` | `false` | also run the [UI detectors](#ui-detectors-temporary) in each page after it loads and write their findings next to each PNG (`<engine>-<width>px-<theme>.findings.json`, a JSON list of `{detector, severity, message, rect?, selector?}`; `[]` is clean). Each shot in the answer then lists `findings` inline plus `findings_file` and `findings_path`. A cell without its file is a failed cell. The [UI gate](#ui-gate) always asks for it |

A call may ask for at most **16 combinations** (engines × widths × themes; repeats count once): split bigger matrices into several calls. The runner is stopped after 5 minutes. The default is 8 shots (2 engines × 2 widths × 2 themes).

**Where the shots go.** They are attached to the task as the agent's images (`source: "agent"`, through `crew::attachments`, so they are stored under `attachments/<task id>/`, fetched with `crewmate_attachment` and shown in the chat like any other image), and the tool's answer lists every combination visited with its file name. They are the proof a [UI task](#ui-tasks-and-proof) needs.

**Requirements** on the crew server, for this feature only:

- **Node.js 20 or newer** with npm (`node`, `npm`, `npx` on `PATH`), the floor of the pinned Playwright. `apt install nodejs` on Ubuntu 24.04 is Node 18 and is refused: use Node from [nodesource](https://github.com/nodesource/distributions) or another source. `atlas crew browsers status` says it plainly (`Node 18.19.1 found, need >= 20`). Nothing else in Atlas Crew needs Node.
- `atlas crew browsers install <ssh-target>` (or `atlas crew install <ssh-target> --browsers`), once, on Debian or Ubuntu. **No npm code runs as root.** In order:
  1. as the crew user, `package.json`, a `package-lock.json` embedded in atlas (`crates/atlas-host/src/crew/capture/package-lock.json`), `runner.mjs` and `detectors.mjs` (the runner imports it) are written to `<hostd dir>/capture/` (`/var/lib/<crew user>/.atlas/capture/`);
  2. as the crew user, `npm ci`: it installs exactly the locked `playwright` and `playwright-core` (one version, `PLAYWRIGHT_VERSION` in `crates/atlas-host/src/crew/capture/mod.rs`) and checks every tarball against the integrity hash in the lock, so a registry that serves something else fails the install instead of running;
  3. as the crew user, `npx --no-install playwright install chromium webkit` downloads the browsers into `<capture dir>/browsers/`, readable by every user on the server;
  4. as the crew user, `npx --no-install playwright install-deps --dry-run chromium webkit` only *reports* which system libraries are missing (`Missing system dependencies (N):` and one package name per line);
  5. atlas reads that report and accepts only package names matching `^[a-z0-9][a-z0-9.+-]*$` (so no space, quote, `;`, `&`, `|`, `$`, backtick or leading `-`), a count that equals the list, and nothing else: any other line is an error shown to you and nothing runs as root. **Atlas builds the one root command itself**, `sudo apt-get update && sudo apt-get install -y --no-install-recommends <those names>`: nothing npm prints is ever executed as a command.
  Re-run it to repair, or after an `atlas crew update` that moves the pin.
- About 1 GB of disk for the two browsers and a few hundred MB of RAM per capture.

`atlas crew browsers status <ssh-target>` prints one line per missing piece, and a crewmate that calls `capture` on a server without the browsers gets the same instruction as the tool's error.

## Locate

"Make this button bigger" arrives with a screenshot, not a file name. The `locate` tool turns a region of a screenshot into the files to edit: it reads the **live page** (the element boxes come from the same Playwright runner as [Captures](#captures), with `dom: true`) and the **work tree**, never the pixels, so there is no OCR and no guessing from the picture.

```json
{"image": ".atlas/attachments/task9/shot.png", "rect": [235, 615, 170, 58], "url": "http://localhost:5173/pricing", "width": 1440}
```

| Field | Meaning |
|---|---|
| `image` | required. Path of the screenshot **inside the crewmate's work tree**, where image attachments are staged (`.atlas/attachments/<task>/<name>`). A path that resolves outside the work tree (`..`, an absolute path elsewhere, a symlink out) is refused |
| `url` | required. The `http(s)` page the image shows (a dev server on the crew server works). The boxes cannot come from a picture, so without it the tool says so |
| `rect` | `[x, y, w, h]` in the **image's** pixels. Omit it to ask "what is this page": the page's main landmark comes first |
| `width` | viewport width to open the page at. Default: the image's own width when it is 390 or 1440 (the two widths `capture` defaults to), else 1440. A `rect` of an image of another width (a retina shot) is scaled onto the page |

**What it does**, in order: opens `url` in chromium at `width` (light theme, whole page) with `dom: true`; keeps the elements whose box **intersects** `rect` (the whole page without one); ranks them; maps the URL path and each candidate to source files; answers.

**Ranking.** For a `rect`, an element scores the geometric mean of the share of the element the region covers and the share of the region the element fills, so a button the region is drawn around beats the page-wide container that merely contains it. That is multiplied by a *specificity* (`data-testid` 1.0, role plus accessible name 0.7, visible text 0.4, nothing 0.1) and nudged up to 20% by depth (a deeper element is a narrower answer). Without a `rect` the `<main>` / `role=main` landmark leads, then the biggest named elements.

**Route files** are found by matching the URL's segments against the path of each file in the work tree (monorepo prefixes like `apps/web/` work; `node_modules`, `target`, `build`, dot folders and symlinks are not read):

| Framework | Files | Notes |
|---|---|---|
| SvelteKit | `src/routes/**/+page.svelte` | `[id]` is one segment, `[...rest]` one or more, `[[lang]]` optional, `(group)` folders are not in the URL |
| Next app router | `app/**/page.tsx` (also `.jsx`, `.ts`, `.js`, `.mdx`, and under `src/`) | same segment rules, `@slot` folders are not in the URL |
| Next pages router | `pages/**` (`index` is its folder; `_app` and `pages/api/**` are not pages) | |
| Flutter | `GoRoute(path: '/computers/:id')` in `.dart` files | `:id` is a parameter; a relative nested path (`settings`) matches the tail of the URL. Reported as `file:line` |

A static segment beats a parameter that also matches (`/settings` over `/[tab]`). The **component file** is the first file other than the route that contains the candidate's `data-testid`, else its visible text (3 characters or more); when only the route file has it, that is the answer.

**Answer.**

```json
{"url": "…", "width": 1440, "image_width": 1440, "ambiguous": false,
 "candidates": [{"route_file": "src/routes/pricing/+page.svelte", "component_file": "src/lib/PlanCard.svelte",
                 "selector": "[data-testid=\"buy-button\"]", "testid": "buy-button", "text": "Buy now", "score": 0.912}]}
```

At most 5 candidates, best first. **`ambiguous: true` when the top two scores are within 10% of each other**, and the answer then carries a `next` line; the tool's description tells the agent to `ask_user` with the candidates (selector and text) as the options and only then edit, never to pick one. A `rect` that touches no element, a `url` that fails to load and an image outside the work tree are errors the agent reads.

It needs the capture browsers ([Requirements](#captures)) and only works for a crewmate's own task pane, since it reads that crewmate's files.

## UI gate

`capture` looks at one page. The `ui_gate` tool decides **which** pages a change needs, remembers what failed, and never lets a fixed UI bug come back unnoticed. It uses `capture` underneath, so it needs the same browsers ([Captures](#captures)).

A **scenario** is one URL in a matrix of engine x width x theme (default chromium + webkit, 390 + 1440 px, light + dark). It has a stable id (12 hex digits of the URL and its sorted cells).

| Call | What it does |
|---|---|
| `ui_gate {action: "run", base_url}` | plans, captures, answers pass/fail per scenario with the capture names. `base_url` is where the app is served (`http://localhost:5173`); routes are joined to it. Optional: `routes` (pages to check instead of the ones the diff implies), `files` (changed files instead of the git diff), `force` (ignore the cache) |
| `ui_gate {action: "rerun_failed"}` | captures only the scenarios that failed last time on this task. Passed ones keep their verdict and are not touched |
| `ui_gate {action: "add_regression", url, note?, widths?, themes?, engines?}` | registers a UI bug the crewmate just fixed |

**The plan.** The changed files are the worktree's diff (`atlas_core::affected::changed_files`: committed since the merge base with `origin/main`, plus staged, unstaged and untracked). The routes they touch come from the same graph as the MCP tool `atlas_affected`: the `affected:` block of `atlas.yaml` (a plugin `command`, or `rules` with `routes:`), else nothing. Each route becomes a default-matrix scenario. **Every regression is always added**, whatever the diff touches. A route that is not a page path (a wildcard, `GET /api`) is an error naming it, not a skipped page. A change that touches no route and a project with no regressions plans nothing and passes.

```yaml
affected:
  rules:
    - match: ['src/routes/docs/**']
      routes: ['/docs']
```

**Results per task.** Each scenario's verdict (pass/fail, capture names, the problems found) is saved in `<crewmate dir>/ui-gate/<task id>/ui-gate.json` after every scenario, so a restart, or a killed run, still knows what failed. The shots are kept next to it as `<scenario id>-<engine>-<width>-<theme>.png`.

**Cache.** A scenario's verdict is stored in the [task cache](cli.md) (`~/.atlas/cache`, shared with every worktree and `atlas run --cache`), keyed by the scenario id, the registered checks and the bytes of every project file git does not ignore, uncommitted edits included. A scenario that already passed over the same bytes is a **hit**: the runner is not started and the answer says `"source": "cached"`. Only passes are stored, so a failing scenario is always captured again; a hit whose screenshots are no longer in the task's folder is a miss. The cache cannot see the running app's data: pass `force: true` when the data behind a page changed.

**Regression registry.** `.atlas/ui-regressions.json` in the project:

```json
{ "regressions": [ { "url": "http://localhost:5173/pricing", "widths": [390], "themes": ["dark"], "note": "toggle clipped at 390" } ] }
```

`add_regression` writes it (the same scenario again only updates its note). **The crewmate commits the file with its fix**, which is how every other worktree and crewmate replays the bug. A file that is not valid JSON, has an unknown field or an entry `capture` would refuse is an error naming the file and what is wrong; the gate never treats an unreadable registry as an empty one, and `add_regression` never overwrites a file it could not read. Regressions are stored with their URL as written, so keep `base_url` stable across runs (a dev server on another port is another scenario).

**Passing.** A scenario passes when every requested combination captured without error and no registered check found a problem. Checks are the extension point: implement `crew::uigate::Check` (`fn name`, `fn run(combo, png) -> Result<Vec<Finding>, String>`) and register it in `uigate::check::registered()`. One is registered, `ui-detectors`: the [UI detectors](#ui-detectors-temporary) run in each page during the capture and a finding of severity `error` fails the scenario, reported per cell as `webkit/390px/dark: empty-band: 554 px of the page … hold no content`, with the element's selector when there is one; a `warn` is kept in the findings file and does not fail it. A check that cannot run fails the scenario (a gate that cannot look must not pass: a cell with no findings file, or one that is not a list of findings, is exactly that), and the registered check names are part of the cache key, so adding a detector re-captures what passed without it.

## UI detectors (temporary)

> **Temporary.** The detectors belong to `@codeatlasdev/ui` ([codeatlasdev/ui#1290](https://github.com/codeatlasdev/ui/issues/1290), requested by [#215](https://github.com/codeatlasdev/atlas/issues/215)): Atlas and every app of the kit must share the same rules. That package has not shipped, so `crates/atlas-host/src/crew/capture/detectors.mjs` is a local implementation of its API, written to be deleted. It has the same names and the same `Finding`, and it depends on no component kit (Atlas runs on customers' apps).

```ts
type Finding = { detector: string; severity: "error" | "warn"; message: string; rect?: DOMRectInit; selector?: string };
detect(page: Page, opts?: { only?: string[] /* + any threshold below */ }): Promise<Finding[]>
```

`capture {checks: true}` and the UI gate run it once per cell, after the screenshot, and write `<cell>.findings.json`. The detectors, with the heuristic each one uses (they favour a missed finding over a false one: a gate that cries wolf is switched off):

| Detector | Fails (`error`) when | Knobs (default) |
|---|---|---|
| `empty-band` | a stretch of the page's height holds no content: text, media, form controls and painted leaf boxes count, a bare wrapper or a painted box that spans a third of the page does not, fixed bars are ignored. A page that simply ends short of the screen is not a band | `emptyBandPx` (490 at ≤ 500 px wide), `emptyBandScreens` (0.6 of the viewport height on wider screens) |
| `gutter` | on a viewport ≤ `mobileMaxWidth` wide, text (measured on its own line boxes), controls or media sit closer than `gutterPx` to the left or right edge. A full-bleed image or video may touch it; off-screen things (a carousel, a drawer) are ignored | `gutterPx` (12), `mobileMaxWidth` (500) |
| `sibling-overlap` | two in-flow element children of one parent share at least `overlapMinPx2` px² and `overlapMinShare` of the smaller box. `absolute`, `fixed`, `sticky`, floats, dialogs, tooltips, menus, listboxes and popovers are overlays and are skipped | `overlapMinPx2` (16), `overlapMinShare` (0.15) |
| `raw-text` | a heading, button, `role=button\|tab\|menuitem`, label, table header, legend or summary is only an identifier: `snake_case`, `kebab-case` of two or more words, a UUID or a 16+ digit hex id. A sentence, a single word or anything with a space is not | none |
| `action-hierarchy` | one group has more than one visible, enabled **primary** action. Primary means `data-variant`, `data-kind`, `data-intent`, `data-emphasis` or `data-priority` equal to `primary`; or a class token `primary`, `cta` or ending in `-primary`/`_primary`/`-cta`; or a `type=submit` button with none of those attributes. A group is the nearest `form`, `[role=group\|toolbar\|dialog]`, `dialog`, `.actions`, `.button-group`, `.btn-group`, `footer`, `nav`, `section`, `article` or `header`, else the parent | none |
| `cls` | the layout shifts without recent input add up to `clsError`; from `clsWarn` it is a `warn`. The finding names the element that moved (`selector`) and where it is (`rect`). Chromium reports shifts itself; WebKit does not, so the runner measures the same quantity by hand (impact fraction × distance fraction of the outermost box that moved, per frame) from before the page loads. Both engines agree on the fixture | `clsError` (0.1), `clsWarn` (0.02) |
| `focus-after-tap` | after focusing a control that does something (button, `a[href="#…"]`, summary, `[tabindex]`, `role=button\|tab\|menuitem`) and activating it with Enter, the focus is on the page body. Focus moving into what it opened, or the control being removed, is fine. Links that would navigate away and submit buttons are not activated. The key press, not a pointer press, is deliberate: WebKit and Safari blur the active element on a pointer press and never focus a button, which is the engine's doing and not the page's | `tapMax` (6 controls) |

**Fixtures** (`crates/atlas-host/src/crew/capture/fixtures/`): `<detector>.fail.html` and `<detector>.fixed.html` for each, plus `clean.fixed.html`, an ordinary page (navigation, hero, cards, a form with its own primary) that must stay clean at 390 and 1440 px. `cargo test -p atlas-host crew::capture` runs every one of them in real chromium and webkit (it skips, printing why, on a machine without node or the browsers): the failing page must give an `error` from its own detector and only that one, the fixed page nothing. `empty-band.fail.html` is the 490 px band of #215.

**Removal plan**, when ui#1290 ships:

1. add `@codeatlasdev/ui` to `package.json`/`package-lock.json` of the capture dir (`PLAYWRIGHT_VERSION`'s neighbour in `capture/mod.rs`), pinned like Playwright;
2. in `runner.mjs`, replace `import { detect, observeShifts } from "./detectors.mjs"` with the package's import (the API is the same: `detect(page, opts)` and the init script that records shifts); the findings file, the Rust side (`capture/detectors.rs`, `uigate/check.rs`) and the tests do not change;
3. delete `detectors.mjs`, `DETECTORS_MJS` (`capture/mod.rs`), its line in `atlas crew browsers install` and `status` (`atlas-cli/src/commands/crew/browsers.rs`), and this section's "temporary" framing;
4. keep `fixtures/` and the real test as the acceptance of the package in Atlas (move them upstream if ui#1290 wants them).

Until then, a threshold that needs to change is changed in `detectors.mjs` and reported on ui#1290 so the package gets the same one.

## Security model

- **Dedicated user.** The crew user (`atlas-crew`, or the reused `atlas-bot`) has no sudo; it writes only its home and what agents create in `workdir`.
- **Systemd hardening.** `NoNewPrivileges=true`, `ProtectSystem=strict`, `ProtectHome=read-only` with `ReadWritePaths=<crew home>`, `PrivateTmp=true`, `ProtectKernelTunables=true`, `ProtectControlGroups=true`, `RestrictSUIDSGID=true`, `MemoryMax=80%`, `TasksMax=4096` ([the unit](#the-systemd-unit)). The crewmates still share one user: they are not isolated from each other.
- **Scoped SSH key.** `atlas crew install` appends your key to the crew user's `authorized_keys` only, never to the admin user's.
- **sudo once.** Install and uninstall need sudo; the running service never does. The sudo password, when Desktop passes it, comes over stdin and is dropped after the credential cache is primed.
- **No npm code as root.** `atlas crew browsers install` runs `npm ci` (from a lock embedded in atlas, integrity-checked), the browser download and Playwright's dependency report as the crew user. Only one command runs as root: `apt-get install -y --no-install-recommends <names>` that atlas builds itself from package names that matched `^[a-z0-9][a-z0-9.+-]*$` in the report; anything else in the report is an error and nothing runs as root. See [Captures](#captures).
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
| `task_refused` | the guards refused a finished task | `task_id`, `reasons` |
| `chain_refused` | an after-crewmate automation was not fired because its crewmate was already in the chain | `trigger_id`, `from_crewmate`, `from_task`, `chain` |
| `refusal_overridden` | the owner accepted refused work | `task_id` |
| `task_unproven_ui` | a `ui` task finished without two agent captures | `task_id`, `reason`, `captures` |
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
| `sudo … asks for a password` | grant passwordless sudo, or run the install from Atlas Desktop (it sends the password with every privileged call, so Ubuntu's per-terminal sudo cache cannot lose it) |
| Uninstall says a crew user is still in use | uninstall checks first and removed nothing: close Atlas Desktop and any ssh session or process running as the crew user, run it again |
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


| hostd (crew server and Mac) | `crates/atlas-host/src/crew/` | command routing (`mod.rs`) to `handlers/` (crewmates, tasks, settings); `hub.rs` (the crew hub); `store/` (on-disk types, the old-layout migration, `archive.rs`: the export/import archive); `runner/` (one task: queue, `pty`, `conversation`, `prompt`, `action`, `outcome`, `summary`, `recover` for tasks a restart interrupted, `guard`/`gate`/`tape` for the guards against the agent); `events.rs` (every push event, one builder each); `pane.rs` (the `crew:` pane key); `engine/` (omp, claude, codex, opencode); `triggers/` and `schedule.rs` (automations); `github/`; `telegram.rs`; `push.rs`; `project.rs` (checkout); `mcp.rs` (`atlas-crew-mcp`); `relay/` (phone to crew server); `tests/` (integration tests by area, shared helpers in `tests/fixture.rs`) |
| hook routes | `crates/atlas-host/src/agents/hook_server.rs` | `/crew/<id>/ask`, `/crew/<id>/set_profile`, `/crew/<id>/hook/<token>` |
| CLI | `crates/atlas-cli/src/commands/crew/` | `atlas crew install\|update\|uninstall\|export\|import\|status\|logs\|login`; the unit text, migration plan, release asset per architecture, update/rollback scripts and the health decision (`binary.rs`), and the export/import/uninstall scripts and decisions (`transfer.rs`) are pure functions with tests |

| hostd (crew server and Mac) | `crates/atlas-host/src/crew/` | command routing (`mod.rs`) to `handlers/` (crewmates, tasks, settings); `hub.rs` (the crew hub); `store/` (on-disk types, the old-layout migration, `archive.rs`: the export/import archive); `runner/` (one task: queue, `pty`, `conversation`, `prompt`, `action`, `outcome`, `summary`, `recover` for tasks a restart interrupted); `events.rs` (every push event, one builder each); `pane.rs` (the `crew:` pane key); `engine/` (omp, claude, codex, opencode); `triggers/` and `schedule.rs` (automations); `github/`; `telegram.rs`; `push.rs`; `project.rs` (checkout); `mcp.rs` (`atlas-crew-mcp`); `capture/` (the `capture` tool: `spec.rs` request and report rules, `detectors.mjs` + `detectors.rs` + `fixtures/` the temporary UI detectors, `runner.mjs` the embedded Playwright script, `package-lock.json` the lock `npm ci` installs from); `relay/` (phone to crew server); `tests/` (integration tests by area, shared helpers in `tests/fixture.rs`) |
| hook routes | `crates/atlas-host/src/agents/hook_server.rs` | `/crew/<id>/ask`, `/crew/<id>/set_profile`, `/crew/<id>/capture`, `/crew/<id>/set_ui`, `/crew/<id>/hook/<token>` |
| CLI | `crates/atlas-cli/src/commands/crew/` | `atlas crew install\|update\|uninstall\|export\|import\|status\|logs\|login\|browsers`; the unit text, migration plan, release asset per architecture, update/rollback scripts and the health decision (`binary.rs`), the export/import/uninstall scripts and decisions (`transfer.rs`), the capture browsers install/status scripts (`browsers.rs`) and the reader of Playwright's dependency report that builds the one root `apt-get` (`apt.rs`) are pure functions with tests |

| hostd (crew server and Mac) | `crates/atlas-host/src/crew/` | command routing (`mod.rs`) to `handlers/` (crewmates, tasks, settings); `hub.rs` (the crew hub); `store/` (on-disk types, the old-layout migration, `archive.rs`: the export/import archive); `runner/` (one task: queue, `pty`, `conversation`, `prompt`, `action`, `outcome`, `summary`, `recover` for tasks a restart interrupted); `events.rs` (every push event, one builder each); `pane.rs` (the `crew:` pane key); `engine/` (omp, claude, codex, opencode); `triggers/` and `schedule.rs` (automations); `github/`; `telegram.rs`; `push.rs`; `project.rs` (checkout); `mcp.rs` (`atlas-crew-mcp`); `locate/` (the `locate` tool: `route.rs` URL path to route files per framework, `rank.rs` the page's element boxes and their ranking, `mod.rs` the request, the `dom: true` capture and the mapping to source); `capture/` (the `capture` tool: `spec.rs` request and report rules, `detectors.mjs` + `detectors.rs` + `fixtures/` the temporary UI detectors, `runner.mjs` the embedded Playwright script, `package-lock.json` the lock `npm ci` installs from); `relay/` (phone to crew server); `tests/` (integration tests by area, shared helpers in `tests/fixture.rs`) |
| hook routes | `crates/atlas-host/src/agents/hook_server.rs` | `/crew/<id>/ask`, `/crew/<id>/set_profile`, `/crew/<id>/capture`, `/crew/<id>/locate`, `/crew/<id>/hook/<token>` |

| hostd (crew server and Mac) | `crates/atlas-host/src/crew/` | command routing (`mod.rs`) to `handlers/` (crewmates, tasks, settings); `hub.rs` (the crew hub); `store/` (on-disk types, the old-layout migration, `archive.rs`: the export/import archive); `attachments.rs` (images on a task: caps, the by-the-bytes type check, safe names, storage, the agent's copy, chunked upload and fetch); `runner/` (one task: queue, `pty`, `conversation`, `prompt`, `action`, `outcome`, `summary`, `recover` for tasks a restart interrupted); `events.rs` (every push event, one builder each); `pane.rs` (the `crew:` pane key); `engine/` (omp, claude, codex, opencode); `triggers/` and `schedule.rs` (automations); `github/`; `telegram.rs`; `push.rs`; `project.rs` (checkout); `mcp.rs` (`atlas-crew-mcp`); `relay/` (phone to crew server); `tests/` (integration tests by area, shared helpers in `tests/fixture.rs`) |

| hostd (crew server and Mac) | `crates/atlas-host/src/crew/` | command routing (`mod.rs`) to `handlers/` (crewmates, tasks, settings); `hub.rs` (the crew hub); `store/` (on-disk types, the old-layout migration, `archive.rs`: the export/import archive); `runner/` (one task: queue, `pty`, `conversation`, `prompt`, `action`, `outcome`, `summary`, `recover` for tasks a restart interrupted, `guard`/`gate`/`tape` for the guards against the agent); `events.rs` (every push event, one builder each); `pane.rs` (the `crew:` pane key); `engine/` (omp, claude, codex, opencode); `triggers/` and `schedule.rs` (automations); `github/`; `telegram.rs`; `push.rs`; `project.rs` (checkout); `mcp.rs` (`atlas-crew-mcp`); `relay/` (phone to crew server); `tests/` (integration tests by area, shared helpers in `tests/fixture.rs`) |
| hook routes | `crates/atlas-host/src/agents/hook_server.rs` | `/crew/<id>/ask`, `/crew/<id>/set_profile`, `/crew/<id>/hook/<token>` |
| CLI | `crates/atlas-cli/src/commands/crew/` | `atlas crew install\|update\|uninstall\|export\|import\|status\|logs\|login`; the unit text, migration plan, release asset per architecture, update/rollback scripts and the health decision (`binary.rs`), and the export/import/uninstall scripts and decisions (`transfer.rs`) are pure functions with tests |

| hostd (crew server and Mac) | `crates/atlas-host/src/crew/` | command routing (`mod.rs`) to `handlers/` (crewmates, tasks, settings); `hub.rs` (the crew hub); `store/` (on-disk types, the old-layout migration, `archive.rs`: the export/import archive); `runner/` (one task: queue, `pty`, `conversation`, `prompt`, `action`, `outcome`, `summary`, `recover` for tasks a restart interrupted); `events.rs` (every push event, one builder each); `pane.rs` (the `crew:` pane key); `engine/` (omp, claude, codex, opencode); `triggers/` and `schedule.rs` (automations); `github/`; `telegram.rs`; `push.rs`; `project.rs` (checkout); `mcp.rs` (`atlas-crew-mcp`); `capture/` (the `capture` tool: `spec.rs` request and report rules, `detectors.mjs` + `detectors.rs` + `fixtures/` the temporary UI detectors, `runner.mjs` the embedded Playwright script, `package-lock.json` the lock `npm ci` installs from); `relay/` (phone to crew server); `tests/` (integration tests by area, shared helpers in `tests/fixture.rs`) |
| hook routes | `crates/atlas-host/src/agents/hook_server.rs` | `/crew/<id>/ask`, `/crew/<id>/set_profile`, `/crew/<id>/capture`, `/crew/<id>/ui_gate`, `/crew/<id>/hook/<token>` |
| CLI | `crates/atlas-cli/src/commands/crew/` | `atlas crew install\|update\|uninstall\|export\|import\|status\|logs\|login\|browsers`; the unit text, migration plan, release asset per architecture, update/rollback scripts and the health decision (`binary.rs`), the export/import/uninstall scripts and decisions (`transfer.rs`), the capture browsers install/status scripts (`browsers.rs`) and the reader of Playwright's dependency report that builds the one root `apt-get` (`apt.rs`) are pure functions with tests |
| Desktop backend | `crates/atlas-desktop/src/crew.rs` | the server registry (`crew-servers.json`), `crew_servers_*` (`crew_servers_set_admin` keeps the admin address), `crew_server_install`, `crew_browsers_status`, `crew_browsers_install`, `crew_agent_*` Tauri commands |
| Desktop UI | `apps/desktop/src/lib/crew/` and `apps/desktop/src/lib/stores/crew.svelte.ts` | workspace, board, editor, settings, onboarding, automations, the look catalogue |
| Mobile | `apps/mobile/lib/features/crew/` | crew tab, chat, profile, automations, relay transport |
| Push relay | `services/push-relay/` | the optional relay |

**Wire naming rule.** An operation on one crewmate is `crewmate_<verb>` (`crewmate_create`, `crewmate_task`, `crewmate_triggers_set`); a crew-level operation is `crew_<verb>` (`crew_settings_get`, `crew_github_login_start`, and the phone relay's `crew_servers`, `crew_call`, `crew_event`). Push events follow the same rule (`crewmate_updated`). JSON fields use `crewmate_id`, and lists use `"crewmates"`. Desktop Tauri commands are `crew_servers_*`, `crew_server_install`, `crew_browsers_*`, `crew_agent_*`. PTY pane keys are `crew:<crewmate id>:<task id>`.

**On disk** (`<hostd dir>` is `~/.atlas` on a Mac, `<crew home>/.atlas` on a crew server):

```text
<hostd dir>/crew.json                  crew-level settings
<hostd dir>/crew-audit.jsonl            crew-wide audit (crewmates created/deleted, pause/resume)
<hostd dir>/capture/                      package.json, package-lock.json, runner.mjs, detectors.mjs, node_modules/playwright, browsers/ (atlas crew browsers install)
<hostd dir>/crew/<crewmate id>/
    crewmate.json                      definition (0600)
    tasks.jsonl                        task log: a full record per state change (last one per id wins) plus one `{"step_of", "step"}` line per live step; compacted to one record per task once it has doubled past 4 MiB
    attachments/<task id>/<name>       a task's images (0600): sent with it, or screenshots the agent took (`capture`)
    uploads/<upload id>/               pieces of an image on its way to a task (removed when the task takes it, or after 1 hour without a piece)
    memory.md                          memory, 64 KiB max
    session.json                       agent session to resume
    secrets.json                       secrets (0600)
    usage.jsonl                        per-task usage records, cut back to the newest half past 1 MiB
    audit.jsonl (+ .1 … .4)            append-only audit, rotated at 1 MiB
    ui-gate/<task id>/                 the UI gate's shots and ui-gate.json
    dom/<task id>/                     element boxes of `capture {dom: true}` (JSON, one per combination)

    work/                              git worktree of the project
    <trigger id>_seen.jsonl            GitHub events already handled (newest ids kept, 256 KiB max)
<hostd dir>/projects/<slug>/           shared clone per Git project
~/.atlas/desktop/crew-servers.json     Atlas Desktop's list of crew servers ([{host, name, admin?: {target, port?}}])
```

The `*.jsonl` logs are crash safe: readers go line by line and decode lossily, so a torn line or a byte that is not UTF-8 costs that one record and never the ones after it; a writer first ends a torn final line, so the next record is not glued onto it; `tasks.jsonl` is read through an in-memory index (the file is parsed once per process, then only what was appended), so listing crewmates and the 30 s cancel check read nothing from disk; files written atomically (`crewmate.json`, `crew.json`, `secrets.json`, a compacted log) are flushed to disk, and their directory, before the rename. A line a version cannot read is kept by compaction, not dropped.

Tests that pin the contract: Rust tests write the JSON fixtures from the real serializers (`UPDATE_FIXTURES=1 cargo test -p atlas-host`) into `apps/mobile/test/fixtures/` (`crewmate_list.json`, `crewmate_tasks.json`, `crewmate_task_summarized.json`, `crewmate_task_resumed.json`, `crewmate_task_event.json`, `crewmate_task_attachments.json`, `crewmate_task_prove_ui.json`, `task_actions.json`, `crew_github_status.json`) and `apps/desktop/src/__fixtures__/` (`crewmate_tasks.json`, `crewmate_task_attachments.json`, `crewmate_task_prove_ui.json`, `crew_telegram_status.json`); the desktop and mobile tests read them, so a field renamed on one side fails on the other. Never edit a fixture by hand.

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
| `crewmate_delete` | `{id}` | `{}`; cancels running and queued tasks and waits for the running turn to end, removes the Git worktree and the `atlas/<id>/…` branches from the shared clone, then the crewmate's directory. A turn that still writes afterwards is refused, so the directory is not recreated |
| `crewmate_task` | `{id, prompt, source, tz?, attachments?, ui?}` | `{task}`; `ui: true` makes it a [UI task](#ui-tasks-and-proof); `attachments` is up to 4 images, `[{name, data_b64}]` or `[{name, upload}]`, see [Images on tasks](#images-on-tasks) |
| `crewmate_attachment_put` | `{id, upload, seq, data_b64, last?}` | `{staged, last}`; one piece (at most 512 KiB raw) of an image too big for a protocol line, pieces in order from `seq` 0. Mobile scope `act` |
| `crewmate_attachment` | `{id, task, name, offset?, limit?}` | `{name, mime, bytes_len, offset, data_b64, last}`; one piece (at most 512 KiB raw) of a stored image, `name` being one of the task's `attachments`. Mobile scope `read` |
| `crewmate_tasks` | `{id, limit?}` | `{tasks:[…]}` newest first, default 50 |
| `crewmate_cancel` | `{id, task}` | `{}` |
| `crewmate_answer` | `{id, ask_id, key?, text?}` | answer an `ask_user` question |
| `crewmate_action_answer` | `{id, task, value, attachments?}` | answer a waiting action: a secret, text, a choice, `add`/`skip`, `override`/`retry` for a refusal; `attachments` (up to 4 images, like a task's) only with `retry` on a `refused` or `prove_ui` card |
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
- `waiting_task` (absent unless a task is parked on a card): `{id, action, result, attachments}`, the card the oldest waiting task is on and the images the agent took for it (only the agent's, as records). Atlas Mobile's Captain's call lists a crewmate with a `refused` or `prove_ui` card from it, with Retry and Override in place. It does not change `state`.
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
- `attachments` (absent without images): the images sent with the prompt, `[{name, mime, bytes_len, sha256, path, source?}]`. `name` is one safe file name unique within the task, `mime` comes from the file's own bytes, `path` is where the agent finds it in its work dir (`.atlas/attachments/<task id>/<name>`). The bytes are never part of a task: fetch them with `crewmate_attachment`. See [Images on tasks](#images-on-tasks). `source` is `"agent"` for a capture the crewmate took and absent for what you sent.
- `ui` (absent unless true): the task is a [UI task](#ui-tasks-and-proof).
