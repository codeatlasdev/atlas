# Atlas Crew: map of main today (2026-11-20)

Read-only study of the repository on `main`. Nothing was edited and `atlas dev` was not run. Issue state came from the GitHub issue pages; the full open list was checked afterwards with `gh issue list --search crew --state open --limit 100`: #527, #528, #348 (agent coordination) and #289 (Linux roadmap).

Primary sources: `docs/crew.md` (read in full, 1093 lines), `docs/mobile.md` crew sections (109-173, plus the permission and audit tables), code under `crates/atlas-host/src/crew/`, `crates/atlas-cli/src/commands/crew/`, `apps/desktop/src/lib/crew/`, `apps/mobile/lib/features/crew/`, and the issues linked below.

## Summary

- **Concept:** Crew runs named agents (crewmates) 24/7 on a Linux server the user owns. `atlas hostd` runs as a systemd service (`atlas-crew.service`). Clients are Atlas Desktop, Atlas Mobile, Telegram, webhooks, GitHub, and the agent's own MCP tools. Model calls go through the user's own agent CLI (omp, Claude Code, Codex, OpenCode). Atlas never calls a model API (`docs/crew.md:5`).
- **Shipped on main:** install/update/export/import/pause/resume over SSH; a permission mode per crewmate decided by hostd on every tool call; one Linux user per crewmate with an nftables egress policy and a per-agent cgroup memory cap; GitHub in and out, through `gh` or a per-crew GitHub App; webhooks behind an operator-supplied HTTPS URL; Telegram; whisper.cpp dictation on the server; phone dictation and spoken replies; capture/locate/UI gate with real browsers.
- **Largest gaps:** the capture browser runs as the crew user with no OS sandbox (N4 in the doc). The push relay is not deployed, so no phone gets a push. Speech and voice engines are not verified on real hardware. The UI detectors are a temporary copy. `gh` is still the crew user's login for everything that has no GitHub App.
- **Open crew issues:** #527 and #528 are the two parent issues. Both are still OPEN, though all their children (#642-#648) are CLOSED. The full open list adds #348 (agent coordination) and #289 (Linux roadmap, "crew-safe").

## 1. Concept

| Term | Meaning | Source |
|---|---|---|
| crew | everything run with Crew; crew-level settings (`crew.json`) | `docs/crew.md:5-15` |
| crew server | Linux host running hostd as `atlas-crew`, installed by `atlas crew install` | `docs/crew.md:17-40` |
| crewmate | named agent: role, engine, mode, look, memory, tasks, automations | `docs/crew.md:255-266` (Crewmates) |
| task | one prompt, run one at a time per crewmate | `docs/crew.md` "Tasks and approvals" |
| automation | something that starts a task: schedule, interval, webhook, GitHub, file, after-crewmate, service | `docs/crew.md` "Automations"; `crates/atlas-host/src/crew/store/mod.rs:89-100` (trigger enum) |

The mental model: an agent is an engine process the server launches per turn. A crewmate is a persistent identity (Linux user, memory file, audit log, look) that owns a queue of tasks. Everything a crewmate does passes through hostd, which decides it, logs it, and can stop it.

## 2. Feature inventory

Paths are relative to the repo root. Line numbers are as read.

| Feature | What the user can do | How it works | Where |
|---|---|---|---|
| Install crew server | `atlas crew install <ssh>` (preflight, binary staged, user, unit, helpers) | Preflight names every missing piece; unit written via temp file then moved (no write-through of a masked unit) | `crates/atlas-cli/src/commands/crew/install.rs`, `layout.rs:255-281` (unit template), `docs/crew.md:42-68` |
| Update / rollback | `atlas crew update [--version] [--rollback] [--binary]` | Stages `atlas.new`, keeps `atlas.prev`, restarts, waits 60 s for hostd, rolls back on failure; refreshes capture runner | `crates/atlas-cli/src/commands/crew/update.rs`, `binary.rs` (health decision), `docs/crew.md:"Update a crew server"` |
| Uninstall | lists what goes, offers export, types server name | Refuses without TTY or `--yes`; never kills processes | `crates/atlas-cli/src/commands/crew/install.rs`, `docs/crew.md:"Commands"` |
| Export / import | one `tar.gz` with manifest, crew.json, per-crewmate definition, memory, tasks, usage; secrets opt-in | Manifest checked before send; unknown `format` refused; collisions refused unless `--rename` | `crates/atlas-cli/src/commands/crew/transfer.rs`; `crates/atlas-host/src/crew/mod.rs:70` (`export_archive`), `:81` (`import_archive`); `docs/crew.md:"Back up and move crewmates"` |
| Pause / resume (kill switch) | `atlas crew pause/resume`, Desktop footer, Mobile button | Cancels running tasks (error `paused`), kills process groups, holds queues, skips automations; state persisted in `crew.json` | `docs/crew.md:"Pause and resume"`; `crates/atlas-host/src/crew/mod.rs:101-178` (dispatch) |
| Permission modes | Ask / Accept edits / Bypass per crewmate; switch mid-task | hostd decides each call: rails → mode → task grants → ask a person | `crates/atlas-host/src/crew/approval.rs:59-100` (`Auto`, `classify`, `rail`), `:158` (`destructive_command`), `:347-360` (`protected`, `protected_name`) |
| Allow once / for this task / deny | prompt answers with optional comment | `agent_approve {pane, req, allow, scope, comment}`; 30-minute wait; timeout = deny | `docs/crew.md:"Approvals"`; `crates/atlas-host/src/crew/hub.rs:283` (`forward_approvals`) |
| Guards on finished work | refusal parks task with Override / Retry | weakened tests, stray files, invented `#N`, unproven claims | `docs/crew.md:"Guards against the agent itself"`; `crates/atlas-host/src/crew/runner/guard.rs` |
| UI proof | `ui` tasks need ≥2 agent-taken captures | `prove_ui` card if fewer | `docs/crew.md:"UI tasks and proof"`; `crates/atlas-host/src/crew/uigate/` |
| Images on tasks | up to 4 images, 5 MiB each, byte-sniffed | `attachments.rs`; staged to `.atlas/attachments/<task>/` and excluded via `info/exclude`; chunked upload | `crates/atlas-host/src/crew/attachments.rs`; `docs/crew.md:"Images on tasks"` |
| Ask user (`ask_user`) | agent asks a question; answer as option or text | `POST /crew/<id>/ask`, 30 min | `crates/atlas-host/src/crew/mcp.rs:159` (dispatch), `:195`; `crates/atlas-host/src/agents/hook_server.rs` |
| Automations: schedule/interval | cron (5 fields, tz) or every ≥60 s | server computes `next_fire` | `docs/crew.md:"Automations"`; `crates/atlas-host/src/crew/schedule.rs`; `croner 4` (CHANGELOG `:76`) |
| Automation: file | watch a path, glob | polled with `std::fs::metadata` | `crates/atlas-host/src/crew/triggers/file_watch.rs` |
| Automation: after-crewmate | chains; `chain` stops cycles | `chain_refused` audit | `crates/atlas-host/src/crew/triggers/mod.rs` |
| Automation: service | `project_dir/.logs/<svc>.log` failed/recovered | blocking open, only after owner confirmed | `crates/atlas-host/src/crew/triggers/file_watch.rs`; `docs/crew.md` Not-covered L3 |
| Automation: webhook | `POST /crew/<id>/hook/<token>` | token in path; auto tasks ask unless `allow_auto` | `crates/atlas-host/src/crew/triggers/webhook.rs`; `crates/atlas-host/src/agents/hook_server.rs` |
| GitHub polling | 2-minute poll of mention, issue_assigned, pr_review_comment, ci_failed, pr_opened, pr_review_requested, issue_opened, release | `gh` as crew user; author filter; seen file per trigger | `crates/atlas-host/src/crew/triggers/github.rs:23` (`POLL_EVERY`), `:28` (`BACKOFF` 15 min), `:67` (`poll_all`), `:82` (`poll_trigger`); `triggers/mod.rs:133` (`AUTHORS`) |
| GitHub: report back | "Working on it." comment, edited checklist, final PR link | marker `<!-- atlas-crew -->` | `crates/atlas-host/src/crew/github/reply.rs`; `docs/crew.md:"GitHub in and out"` |
| GitHub App | create one-click (manifest flow); bot identity `<slug>[bot]`; short-lived tokens per repo | key in `<crew dir>/github-app.json` (0600); JWT RS256 via `ring`; token cached until 5 min before expiry | `crates/atlas-host/src/crew/github/app/` (`mod.rs:57` USERNAME, `api.rs`, `jwt.rs`, `tokens.rs`, `service.rs`, `manifest.rs`, `helper.rs`, `gh_shim.rs`, `hooks.rs`, `record.rs`, `http.rs`); `docs/crew.md:"GitHub App"` |
| GitHub credential helper | git gets a token per operation | `atlas crew git-credential` → hostd hook `POST /crew/<id>/git_credential` | `crates/atlas-host/src/crew/github/app/helper.rs`; `docs/crew.md:"Git for a crewmate"` |
| `gh` shim | `gh` in a crewmate asks hostd for a per-repo token | `<crew dir>/shims/gh` → atlas binary | `crates/atlas-host/src/crew/github/app/gh_shim.rs` |
| Co-author attribution | commits get `Co-authored-by: <requester>` | `core.hooksPath` commit-msg hook written by hostd | `docs/crew.md:"Attribution"`; `crates/atlas-host/src/crew/github/app/hooks.rs` |
| GitHub webhooks | GitHub App webhook delivers instantly when a public URL exists | route `POST /github/webhook` on loopback hook server; HMAC `sha256=` on raw body; delivery-id dedupe; 202 at once; reconcile every 5 min | `crates/atlas-host/src/crew/triggers/github_hook.rs:27` (`RECONCILE_EVERY`), `:111` (`process`); `docs/crew.md:"Webhooks"` |
| Telegram | one bot per crew server; link chats with a 6-digit code; commands `/crew /mode /task /ui`, photos, voice notes | 5 wrong codes kill the live code | `crates/atlas-host/src/crew/telegram.rs:54` (`/ui`), `:64` (`UI_USAGE`), `:148` (`MAX_WRONG_CODES`=5), `:643` (`/mode`); `crates/atlas-host/src/crew/telegram/voice_notes.rs`; `docs/crew.md:"Telegram"` |
| Dictation, server side | Desktop mic and Telegram notes transcribed by whisper.cpp on the crew server | pinned source 1.9.5 and models (sha256 checked); no fallback to a cloud engine; 300 s cap; audio deleted after use | `crates/atlas-host/src/crew/voice/engine.rs:29` (`WHISPER_VERSION`), `:47` (`MODELS`), `:63` (`DEFAULT_MODEL` small), `:78` (`RUN_DEADLINE` 15 min); `voice/audio.rs`, `voice/vocab.rs`; `atlas-cli/src/commands/crew/voice.rs`; `docs/crew.md:"Dictation"` |
| Dictation, optional hosted | OpenAI-compatible endpoint with secret key | `crew_voice_set {engine: openai_compatible}` | `crates/atlas-host/src/crew/voice/engine.rs` |
| Dictation, phone | on-device recognizer (iOS `SpeechAnalyzer` / `SFSpeechRecognizer`, Android `createOnDeviceSpeechRecognizer`); system recognizer only as labelled opt-in | audio never reaches hostd | `apps/mobile/lib/features/crew/chat/dictation_bar.dart`, `dictation_flow.dart`; `apps/mobile/lib/core/voice/`; `docs/mobile.md:"Dictation"` |
| Voice answers | "allow"/"deny" exact phrases; choice by number or label; Bypass, rails and destructive commands need a tap | hostd returns verdict; untrusted Telegram transcripts answer nothing | `crates/atlas-host/src/crew/voice_answer.rs`, `voice_card.rs`; `docs/crew.md:"Voice answers to cards"` |
| Spoken replies | `spoken:` line from agent, else first sentences; read aloud on the device | hostd computes `spoken` per task | `crates/atlas-host/src/crew/runner/spoken.rs`; `apps/desktop/src/lib/crew/voice/readAloud.svelte.ts`; `apps/mobile/lib/core/voice/speech.dart`; `docs/crew.md:"Spoken replies"` |
| Engine sign-in from phone | `crew_engine_login {engine}` opens a PTY running only that engine's login command | allowlist `ENGINES`; status command per engine | `crates/atlas-host/src/crew/engine_login.rs` (`REAP_AFTER_EXIT`=10 min at `:317`); `apps/mobile/lib/features/crew/ops/engine_signin.dart`; `docs/crew.md:"Signing in from the phone"` |
| Capture (browsers) | `capture` opens URL in Playwright chromium and webkit at widths/themes; PNG per combo | runner `capture/runner.mjs`, pinned Playwright in `capture/mod.rs`; proxy blocks private/metadata addresses for isolated crewmates | `crates/atlas-host/src/crew/capture/` (`mod.rs`, `runner.mjs`, `tests.rs`, `fixtures/`); `docs/crew.md:"Captures"` |
| Locate | screenshot region → route file and component | reads live DOM via `dom: true`; SvelteKit, Next, Flutter route files | `crates/atlas-host/src/crew/locate/` (`route.rs`); `docs/crew.md:"Locate"` |
| UI gate | plans scenarios from git diff, caches passes, regression registry | `.atlas/ui-regressions.json`; verdicts in `<crewmate>/ui-gate/` | `crates/atlas-host/src/crew/uigate/mod.rs`, `tests.rs`, `check.rs`; `docs/crew.md:"UI gate"` |
| UI detectors (temporary) | seven detectors in each page: empty-band, gutter, sibling-overlap, raw-text, action-hierarchy, cls, focus-after-tap | local copy of an API that ui#1290 will ship; marked temporary | `crates/atlas-host/src/crew/capture/detectors.mjs`, `detectors.rs`; `docs/crew.md:"UI detectors (temporary)"`; issue #215 (closed) |
| Crewmate look | 9 colors, hats, tools, outfits, eyewear, role kits | `normalizeLook` replaces invalid fields | `apps/desktop/src/lib/crew/look.ts`, `lookLabels.ts`, `crewmateFace.ts`; `apps/mobile/lib/features/crew/avatar/`; `docs/crew.md:"Crewmate look"` |
| Crew board | queued / working / needs you / done across crewmates | desktop `board.ts`; mobile `board.dart` | `apps/desktop/src/lib/crew/board.ts`, `CrewBoard.svelte`; `apps/mobile/lib/features/crew/board.dart` |
| Audit | per-crewmate `audit.jsonl`, crew-wide `crew-audit.jsonl`; secret redaction | 1 MiB rotation, five files | `crates/atlas-host/src/crew/store/` (audit writer), `crates/atlas-host/src/crew/audit/` (`mod.rs`, `observe.rs`); `docs/crew.md:"Audit"` |
| Memory | `memory.md`, 64 KiB, edited by agent and apps | `crewmate_memory`, `crewmate_memory_set` | `crates/atlas-host/src/crew/store/`; `apps/desktop/src/lib/crew/settings/MemorySection.svelte`; `apps/mobile/lib/features/crew/profile/` |
| Usage | per-task usage, rotation at 1 MiB, 400 files | `usage.jsonl` | `crates/atlas-host/src/crew/store/usage.rs`; `apps/desktop/src/lib/crew/settings/UsageSection.svelte`; mobile `ops/crewmate_usage_screen.dart` |
| Push | task-done and needs-you pushes via a relay | kind only; relay grants; `ATLAS_PUSH_RELAY=off` disables | `crates/atlas-host/src/crew/push.rs:44` (`relay_base`), `:55` (`notify`), `:64` (`notify_agent`), `:116-151` (tokens); `services/push-relay/`; `docs/crew.md:"Push notifications"` |
| Mac relay | phone reaches crew servers through a paired Mac | `crew_servers`, `crew_call`, `crew_event` | `crates/atlas-host/src/crew/relay/mod.rs`; `docs/mobile.md:"Crew on the phone"` |
| Direct phone pairing | `atlas crew pair` puts a phone on the server without a Mac | `atlas hostd --mobile-pair` | `crates/atlas-cli/src/commands/crew/pair.rs`; `docs/mobile.md:"Pair a crew server with no computer in the loop"` |
| Per-crewmate Linux user | each crewmate is `atlas-cm-<12 hex>`; helper services start agents and run git | `atlas-crew-spawn` (socket, SO_PEERCRED), `atlas-crew-provision`; nftables egress per uid | `crates/atlas-host/src/crew/spawn/` (`server.rs`, `layout.rs`, `validate.rs`, `git.rs`, `protocol.rs`, `provision.rs`, `cgroup.rs`); `docs/crew.md:"One Linux user per crewmate"` |
| Memory cap per agent | `memory.max` per crewmate leaf, OOM group kill | `agent_memory_max` setting | `crates/atlas-host/src/crew/spawn/cgroup.rs`; `docs/crew.md` (#296) |
| Hook endpoint | loopback port 47631; per-crewmate token | `AGENT_ROUTES` in `agents/hook_auth.rs` | `crates/atlas-host/src/agents/hook_server.rs`, `hook_auth.rs` |
| MCP for the agent | `atlas-crew-mcp` stdio server, seven tools | `mcp.rs:159-165` dispatch, `:345` `tools_list` | `crates/atlas-host/src/crew/mcp.rs`; `crates/atlas-host/src/crew/engine/launch.rs:464` (`McpServer`) |

## 3. Architecture

### 3.1 Runtime

- **hostd** is `atlas hostd` as the unprivileged `atlas-crew` user, under `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome=read-only`, `MemoryMax=80%`, `TasksMax=4096` (`crates/atlas-cli/src/commands/crew/layout.rs:255-281`; `docs/crew.md` "The systemd unit"). It exits only with `ATLAS_HOSTD_IDLE_SECS=never`.
- Crew is gated by `ATLAS_CREW_HOST=1`. Without it every `crewmate_*` and `crew_*` command errors (`docs/crew.md` "Environment").
- **Command routing:** `crates/atlas-host/src/crew/mod.rs:101` (`match cmd`) dispatches `HostCmd::Crewmate*` and `HostCmd::Crew*` to `handlers/` (`crewmates.rs`, `tasks.rs`, `settings.rs`). Protocol is NDJSON on `~/.atlas/hostd.sock` (or the crew server's), reached over `ssh -L` by Desktop.
- **Hub** (`crates/atlas-host/src/crew/hub.rs`) holds per-crewmate state, approval forwarding (`:283`), and the agent event stream.
- **Runner** (`crates/atlas-host/src/crew/runner/`): one task at a time per crewmate, through `RunnerMap` (`runner/mod.rs:111`). Files: `pty.rs` (terminal agents), `conversation.rs` (headless turns), `prompt.rs`, `action.rs`, `outcome.rs`, `summary.rs`, `spoken.rs`, `recover.rs` (restart recovery), `gate.rs`, `guard.rs`, `tape.rs` (`MAX_COMMANDS` 300 at `:22`, `MAX_OUTPUT` 16 KiB at `:24`), `github.rs`, `isolation.rs`.
- **Engines** (`crates/atlas-host/src/crew/engine/`): `launch.rs` is the one launcher. `Cmd` (`:29`), `Exec` (`:73`), `Output` (`:107`), `TurnCtx` (`:131`), `output` (`:210`), `rpc` (`:258`). `claude.rs`, `omp.rs`. Cancel, pause and the 2 h cap kill the process group.

### 3.2 PTY vs headless

| Mode | When | Approval | Source |
|---|---|---|---|
| Terminal (PTY) | crewmate has no `engine` | Claude Code and Codex by hook; omp by approval extension | `runner/pty.rs`; `docs/crew.md` "Crewmates" |
| Headless engine | `engine` set (`claude`, `codex`, `opencode`, `omp`) | only `claude` and `omp` can ask; `codex` and `opencode` accept only `bypass` | `engine/launch.rs`; `docs/crew.md` "Crewmates" |

Claude is launched with `--permission-mode default --permission-prompt-tool atlas:approve`. omp is launched with the approval extension and never `--auto-approve` (`docs/crew.md` "Permission modes").

### 3.3 Task queue

- A task is `queued`, `running`, `waiting_action`, `done`, `failed` or `cancelled` (`docs/crew.md` "Task object").
- Queueing: one per crewmate; `max_agents` (default 4) caps agents per server. `waiting_for: "server busy"` is shown when full. A card-waiting task holds no slot.
- Storage: `tasks.jsonl` (full record per change, compacted past 4 MiB) plus `tasks.idx` (`docs/crew.md` "On disk").
- Restart: running tasks go back to the head of the queue as `resumed` (`recover.rs`). After 2 restarts the task fails (`docs/crew.md` "What survives a restart"). Pending approvals do not survive. Issue #199 (resume after restart) is CLOSED.
- Hard cap: 2 h per turn (`docs/crew.md` "Pause and resume").

### 3.4 Approvals, modes and rails

Decision order per tool call, `crates/atlas-host/src/crew/approval.rs`:

1. `rail(call)` (`:85`): `no_auto` tasks (GitHub or webhook without `allow_auto`), and protected paths. Protected names are `.git`, `.claude`, `.omp`, `.codex`, `.mcp.json`, shell rc files, `.env*`, key files, `.ssh/` (`:347`, `:357-360`). A rail holds the call in every mode.
2. `classify(mode, call, granted)` (`:68-80`): `Bypass` allows; `Edits` allows tree edits; otherwise a task grant allows; else hold for a person.
3. A held call is answered `allow once`, `allow for this task` (`scope: task`) or `deny`, with comment. A timeout denies and notifies (`approval_decision` with `source: timeout`).
4. `destructive(call)` (`:107` area, `destructive_command` `:158`) forces a tap even for voice answers.

Switching the mode (`crewmate_set_approval`) is owner-only: Desktop, Mobile with the `approve` scope, and the owner's Telegram chat (`/mode`). A crewmate's own process is refused (`docs/crew.md` "Permission modes").

Protection that is a single rule: `allow_bypass` on the crew (default `true`) turns Bypass off everywhere when false (`docs/crew.md`).

### 3.5 Automations and triggers

- Types and fields: `docs/crew.md` "Automations". Stored in `crewmate.json` under `triggers` (`crates/atlas-host/src/crew/store/mod.rs:89-100`).
- Authors allowed for GitHub: `AUTHORS` at `crates/atlas-host/src/crew/triggers/mod.rs:133` lists `owner, member, collaborator, contributor, anyone`. Default is `default_authors()` at `store/mod.rs:70` = owner, member, collaborator (`docs/crew.md` "Automations").
- Untrusted text: `untrusted_event` (`triggers/mod.rs`, from the `UNTRUSTED CONTENT` preamble constant) wraps every stranger's text in quoted lines before the prompt.
- Tick: `triggers/github.rs` polls every 120 s (`POLL_EVERY` `:23`); a failed poll backs off 15 min (`BACKOFF` `:28`).
- Scheduling: `crates/atlas-host/src/crew/schedule.rs` (croner 4, jiff). Times and zones via `jiff` (CHANGELOG `:76`).

### 3.6 GitHub

Three layers:

1. **`gh` login, crew user.** `device-code` login (`crew_github_login_start`, `_poll`, `_logout`, `_status`). hostd installs `gh` under the crew home if missing. Used for polling and as fallback for repos no installation covers (`docs/crew.md` "GitHub").
2. **GitHub App** (`crates/atlas-host/src/crew/github/app/`). The app key is in `github-app.json` (0600), never in `secrets.json`. Tokens are per repo (`repositories: [repo]`), minimum permissions, cached, never on disk. Audited (`github_token_minted`, `github_app_fallback`). `GithubApi` in `app/api.rs` picks the App token if an installation covers the repo, else `gh`. If a token cannot be minted it fails rather than silently using `gh`.
3. **Webhooks** (`triggers/github_hook.rs`). Route `POST /github/webhook` on the loopback hook server. The operator adds their own HTTPS front. Signature first (`X-Hub-Signature-256`, constant time), then delivery-id dedupe in `github-deliveries.jsonl` (last 1000), `202` at once, work in its own task. Polling hands over per repo only when a signed delivery has arrived. Reconciliation every 5 minutes (`RECONCILE_EVERY` `:27`) redelivers failed ones from the last 3 days.

Reporting: the crew posts a marker comment `<!-- atlas-crew -->` and edits it (`github/reply.rs`, `docs/crew.md` "GitHub in and out").

Guard for invented `#N` numbers: `runner/guard.rs` checks against the GitHub API (see Known limits L4).

### 3.7 Telegram

- `crates/atlas-host/src/crew/telegram.rs` (1240 lines). Bot token write-only (`crew_telegram_set`). Link code 6 digits, 10 min, `MAX_WRONG_CODES = 5` (`:148`).
- Owner chat: the only chat allowed to answer approvals and cards. Untrusted chats get nothing answered (`docs/crew.md` "Voice answers to cards").
- Voice notes: `telegram/voice_notes.rs`. Echo with Send as task / Edit / Cancel, 1 h memory.

### 3.8 Voice

- Server: `voice/` (`audio.rs`, `engine.rs`, `vocab.rs`, `mod.rs`). Model `small` by default, `base` optional. Single transcription at a time, 15 min deadline. `ffmpeg` only for Telegram Opus.
- Phone: on-device only (`apps/mobile/lib/core/voice/`).
- Desktop: native mic capture (16 kHz mono WAV), uploaded in 512 KiB pieces (`crewmate_voice_put`), transcribed by `crewmate_transcribe` (`docs/crew.md` "Dictation", `apps/desktop/src/lib/crew/voice/dictation.svelte.ts`).

### 3.9 Memory

`memory.md` in the crewmate dir (`docs/crew.md` "On disk"). Under isolation it is `0660` in `crewmate/` owned by the crewmate (`docs/crew.md` "One Linux user per crewmate", table row). A first task moves it and keeps `memory.md.pre-isolation`.

### 3.10 Crewmate look

`apps/desktop/src/lib/crew/look.ts` defines the catalogue; hostd stores the look and validates it (`crewmate_list` returns `color` as `#rrggbb` and fields `[a-z0-9_-]{1,24}`). Phone renders in `apps/mobile/lib/features/crew/avatar/crewmate_painter.dart`.

### 3.11 Multi-crewmate

- Each crewmate has its own user under isolation, its own worktree and audit, its own secrets.
- `max_agents` caps concurrency across the server.
- Chaining (`after crewmate`) cycle guard via `chain` (`chain_refused`).
- No direct crewmate-to-crewmate messaging. Handoff is by after-crewmate automation with the summary (Issue #106 review said so).

### 3.12 Push

Relay at `push.codeatlas.com.br` gets token, grant, and event kind only (`docs/crew.md` "Push notifications"). The app is built without Firebase, so **no push reaches a phone today**. Per-device relay grants were added (`push_register {token, grant}`). Issue #205 (relay caller auth) is CLOSED, though the doc says the relay is not deployed.

### 3.13 Security model

From `docs/crew.md` "Security model" and the sections it links:

- Dedicated Linux user, no sudo; systemd hardening as above.
- Per-crewmate Linux users (`atlas-cm-<id>`), nftables egress per uid (`/etc/atlas-crew/egress.nft`, `atlas-crew-egress.service`), `ProtectProc=invisible` where supported.
- Scoped SSH key appended to the crew user's `authorized_keys` only.
- sudo only at install and uninstall; the service never uses it.
- Secrets in `<crewmate>/secrets.json` (0600), injected as env to that crewmate's agent only, redacted in audit, never returned.
- Audit: every tool call, approval decision with device, task lifecycle, config changes.
- Pinned, hash-checked whisper.cpp; `capture` proxy for isolated crewmates refuses private, link-local and metadata addresses and non-own ports.
- No CodeAtlas servers in the path except the optional push relay (kind only).

### 3.14 Install, update, backup and move

- Install: `atlas crew install <ssh>` (x86_64 and aarch64, `--port`, `--identity`, `--binary`, `--browsers`, `--voice`). Desktop does the same with SSH key or password (password is passed via `SSH_ASKPASS` through a one-shot local socket, `docs/crew.md` "Add a crew server in Atlas Desktop").
- Update: `atlas crew update` with `atlas.prev` rollback (`update.rs`, `binary.rs`).
- Backup: `atlas crew export` / `import` (`transfer.rs`, `crew/mod.rs:70,81`). Secrets only with `--with-secrets`. Never exported: `work/`, `session.json`, GitHub seen files, agent sign-ins.
- Uninstall: `atlas crew uninstall`, lists what goes, refuses without TTY or `--yes`.

### 3.15 Observability

- `atlas crew status` and `atlas crew logs -f` (journalctl).
- Audit logs as above; `crew_audit` and `crewmate_audit` for clients.
- Usage `usage.jsonl`.

## 4. UX surfaces

| Surface | Where | Notes |
|---|---|---|
| Desktop workspace | `apps/desktop/src/lib/crew/CrewWorkspace.svelte`, `CrewmateEditor.svelte`, `CrewBoard.svelte`, `CrewPalette.svelte`, `CrewSidebar.svelte` (`apps/desktop/src/lib/workspace/`) | chat, board, editor, settings (`settings/`), onboarding (`onboarding/CrewOnboarding.svelte`), automations (`automation/`) |
| Desktop backend | `crates/atlas-desktop/src/crew.rs` | server list (`crew-servers.json`), install, browser install, agent commands |
| Mobile | `apps/mobile/lib/features/crew/` | list (`list/`), chat (`chat/crewmate_chat_screen.dart`), profile (`profile/`), ops (`ops/`: audit, usage, routines, settings, engine sign-in), permissions (`permissions/`), automation editor (`automation_editor/`), avatar (`avatar/`) |
| Telegram | `crates/atlas-host/src/crew/telegram.rs` | commands, photos, voice notes |
| CLI | `crates/atlas-cli/src/commands/crew/` | install, update, uninstall, export, import, status, logs, login, pause, resume, pair, browsers, voice |
| Agent (MCP) | `crates/atlas-host/src/crew/mcp.rs` | `ask_user`, `set_profile`, `set_ui`, `capture`, `locate`, `ui_gate`, `approve` |

Voice: desktop has the mic in the composer; mobile has dictation bar, voice answer, spoken reply toggle. Both are partial (Known limits).

Look: `docs/crew.md` "Crewmate look".

## 5. Known limits (from docs, not closed)

These sit under "Not covered", "Not verified" or temporary wording in `docs/crew.md` and the issues.

| # | Limit | Where |
|---|---|---|
| N4 | Capture browser has no OS sandbox (`--no-sandbox`) and runs as the crew user. A page exploit reaches every crewmate. | `docs/crew.md` "Not covered (review 6)" |
| L2 | `capture`/`locate` are not under the crewmate cgroup cap; concurrency is not limited. | `docs/crew.md` "Not covered (review 6)" |
| L3 | Service trigger reads a log with blocking open that follows links; File trigger polls with `std::fs::metadata`. Both only after the owner confirms. | `docs/crew.md` |
| L4 | PR guard can reveal which issue numbers exist in a private repo, since the operator's login checks `#N`. | `docs/crew.md` |
| L5 | Owner's own git/file/LSP tools can run as the crew user inside an isolated crewmate's tree. | `docs/crew.md` |
| S-A | The hook port 47631 can be taken by a leftover process while hostd restarts; the fix (systemd `ListenStream` and leftover cleanup) is not done. | `docs/crew.md` |
| - | `/proc/<pid>/cmdline` visible to other crewmates on systems without `ProtectProc=invisible` support (systemd < 247). The prompt is on the command line. | `docs/crew.md` "Not covered" |
| - | Shared abstract unix sockets, SysV IPC, `/tmp` and `/dev/shm`. | `docs/crew.md` (shared kernel namespaces) |
| - | `gh` login is the crew user's and shared by every crewmate without its own token. | `docs/crew.md` "Not covered" |
| - | Engines in `engine-home` are shared and root-installed. | `docs/crew.md` "Not covered" |
| - | No per-crewmate egress allowlists; loopback is dropped for agents (`localhost` test servers fail inside an isolated agent). | `docs/crew.md` |
| - | Push not live (relay not deployed, app built without Firebase). | `docs/crew.md` "Push notifications" |
| - | Resume after restart works only 2 times per task. | `docs/crew.md` "What survives a restart" |
| - | A resumed task restarts the agent with a prompt; the agent continues its saved session only for headless engines. | `docs/crew.md` |

Other constraints stated in the doc: prompts are capped by the engine, not Atlas; the 2 h turn cap; the 5 min dictation cap; 16 combinations per `capture` call; 4 images per task.

## 6. Partially done

| Item | What is done | What is missing | Source |
|---|---|---|---|
| Bubblewrap/landlock sandbox | Per-uid isolation, egress, cgroup | No filesystem sandbox on top | Issue #200 (CLOSED) comment: "Not done: bubblewrap/landlock" |
| UI detectors | Seven detectors run and fail scenarios | Temporary local copy; swap to `@codeatlasdev/ui` pending ui#1290 | `docs/crew.md` "UI detectors (temporary)"; issue #215 (CLOSED, but the swap is open upstream) |
| Speech synthesis and recognition | Code and fakes tested | Not run on real hardware (iOS, Android, macOS, Windows, Linux) | Issue #648 comment: "Not verified on real hardware"; #647 comment |
| whisper.cpp end to end | Pinned build and tests with fake engine | No real whisper build on a Linux server verified | Issue #647 comment |
| Push | Client registration, relay client, grants | Relay not deployed; Firebase not in the app build | `docs/crew.md` "Push notifications" |
| GitHub App for isolated crewmates | Token per git process; `gh` shim | Existing servers need `atlas crew install` again (docs) | Issue #644 comment |
| Crew "Act as me" option | Mentioned in issue #644 design (`gh` as an explicit per-crewmate choice) | Not in `docs/crew.md` as a feature | Issue #644 comment |
| GitHub App manifest placeholder | Works without public URL with `active: false` | The issue #643 design asked this be verified against GitHub | Issue #643 comment (design) |
| Mobile audit screen | Present in mobile docs | `docs/crew.md` "Audit" says "Atlas Mobile has no audit screen"; the two docs disagree | `docs/mobile.md` table row "Crewmate profile → Audit"; `docs/crew.md` "Audit" |
| Telegram `/ui` and photos | Implemented | Photo tests use the same code as text; no real Telegram run verified | `docs/crew.md` "Telegram" |
| Monocode-style per-task cost | Per-crewmate usage only | No cost per task for all engines | Issue #106 review (P1 list) |

## 7. Open issues (as read from `issue://`)

Crew-specific open issues in the first page of open issues:

| # | Title | State | Labels | Notes |
|---|---|---|---|---|
| #528 | crew: one-click GitHub App setup (manifest flow, prefilled), pick which orgs/repos it can access | OPEN | type: feature, area: desktop, status: confirmed | Parent. Children #643, #644, #645 are CLOSED and shipped in #649 and #652. Still OPEN on the tracker. |
| #527 | crew: one-click bypass-permissions mode and a voice mode (dictate tasks, spoken replies) | OPEN | type: feature, area: desktop, status: confirmed | Parent. Children #642, #647, #648 are CLOSED. |
| #289 | Roadmap: Atlas end to end on Linux (stable core, native look, instant, crew-safe) | OPEN | type: feature, area: desktop, priority: 1 high, os: linux | Mentions crew-safe; not a crew issue itself. |

Crew-related items that are CLOSED and relevant (for the maintainer's list of what is not open):

| # | Title | Status | Source |
|---|---|---|---|
| #106 | feat(crew): Atlas Crew v1 | MERGED 2026-11-14 | `issue://codeatlasdev/atlas/106` |
| #199 | resume in-flight tasks after restart | CLOSED | `issue://…/199` |
| #200 | isolate crewmates | CLOSED (bubblewrap/landlock not done) | `issue://…/200` |
| #201 | append-only audit | CLOSED | `issue://…/201` |
| #202 | export/backup | CLOSED (not fetched directly) | `docs/crew.md` |
| #203 | `atlas crew update` and arm64 | CLOSED | `issue://…/203` |
| #204 | GitHub in and out | CLOSED | `issue://…/204` |
| #205 | push relay auth | CLOSED (relay not deployed) | `issue://…/205` |
| #215 | UI detectors | CLOSED (temporary copy; upstream ui#1290 open) | `issue://…/215` |
| #296 | per-agent memory cap | CLOSED (referenced in docs) | `docs/crew.md` |
| #385 | referenced in #200 comment | state not fetched | `issue://…/200` |
| #642 | permission modes | CLOSED | `issue://…/642` |
| #643 | GitHub App 1/3 | CLOSED | `issue://…/643` |
| #644 | GitHub App 2/3 | CLOSED | `issue://…/644` |
| #645 | GitHub App 3/3 (webhooks) | CLOSED | `issue://…/645` |
| #647 | dictation | CLOSED | `issue://…/647` |
| #648 | spoken replies and voice answers | CLOSED | `issue://…/648` |
| #650, #649, #652, #658, #659, #662 | PRs that shipped the above | referenced in issue comments | `issue://…/642`, `…/643`, `…/644`, `…/647`, `…/648` |

Checked afterwards with `gh issue list --search crew --state open --limit 100`: the only other open issue that mentions crew is #348 (agent coordination: ownership, claims and heads-ups).

## 8. Notes for a future pass

- The docs contradict each other on the mobile audit screen (see Section 6).
- `docs/crew.md` describes several pieces as "not done" (N4, L2, S-A) that are real risks for any crewmate that runs `capture`.
- Two open parent issues (#527, #528) are still open even though their children are closed. Close them or split the leftovers.
- The `approve` MCP tool is in the tools list (`mcp.rs:452`), but `docs/crew.md` "MCP tools for the agent" lists it under the note "Claude's permission prompt tool" only. Confirm whether agents should see it.
