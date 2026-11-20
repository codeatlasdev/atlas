# Atlas Crew next to OpenBot, n8n Nod and agentes.chat: what is missing

Study of 2026-11-20. Research only: no code, no issues. It answers three questions:

1. What do [OpenBot](https://openbot.run/pt), [n8n Nod](https://nod.n8n.io/) and [agentes.chat](https://github.com/agentes-chat/agentes-chat) let people do, and what system sits behind each?
2. How does each one think about an agent and the computer it works on ("agent computing")?
3. What would Atlas Crew need (core, features, docs) to make the same things possible, and what should it not copy?

The evidence is in four reports in this folder, each with a source for every claim:

| File | What it covers | How it was researched |
|---|---|---|
| [atlas-crew-today.md](atlas-crew-today.md) | Atlas Crew on `main` today: concept, feature inventory with file paths, architecture, known limits, open issues | Read of `docs/crew.md`, `docs/mobile.md` and the code under `crates/atlas-host/src/crew`, `crates/atlas-cli/src/commands/crew`, `apps/desktop/src/lib/crew`, `apps/mobile/lib/features/crew` |
| [openbot.md](openbot.md) | OpenBot (Nightly Labs): about 120 features, architecture, security model, pricing, a 30-item parity checklist | Site in pt and en, the public repo at `fc628c4` (docs, architecture notes, source), store listings, reviews |
| [n8n-nod.md](n8n-nod.md) | Nod (n8n Labs) and the n8n agent platform under it: trust checker, Vault, memory, task sessions, evals, MCP, triggers, human-in-the-loop, queue mode | All 78 Nod docs pages, the `n8n-io/nod` repo, the launch post, about 40 n8n docs pages |
| [agentes-chat.md](agentes-chat.md) | agentes.chat: chat-first team workspace with agents as members, 201 ADRs, runner, tool gateway, approvals, memory loop, code map | Shallow clone at `98c39c4` (code, ADRs, `CLAUDE.md`, `SELF_HOSTING.md`), GitHub API, the public site |

None of the three products was installed or run. Behaviour is taken from their docs and source. Where the reports say `[INFERENCE]`, it is not verified.

## 1. The three products in one line each

| | OpenBot | n8n Nod | agentes.chat |
|---|---|---|---|
| What | Desktop app (Electron) that turns the agent CLIs a person already pays for into a team of named, persistent agents, reachable from desktop, web, phone and chat apps | Personal agent "with its own computer": one owner, one conversation, parallel task sessions, a Linux workspace and a browser in containers | Open-source, Slack-like team chat where agents are workspace members with a role, an owner, a budget, scoped tools, memory and a computer |
| Who for | Individuals and small teams with ChatGPT/Claude/Gemini plans; agencies | One person and their errands (mail, calendar, invoices, shopping, n8n workflows) | Whole teams, explicitly not only engineers (Sales, Support, Finance...) |
| License, model | PolyForm Noncommercial (source-available); hosted servers €30-100/month, commercial license €10-25/user | Apache-2.0, free, research preview from n8n Labs; you pay your model provider | AGPL-3.0 plus a commercial license; Cloud $29-99/month; bring your own plans or keys |
| Maturity (2026-11-20) | 100 days old, v0.75.0, 130 versions, 9.4k stars, 10K+ Android installs | 10 days public, v0.9.1, 2.8k stars, a release a day | 11 days public, ~10.5k commits, 1.8k stars, 201 ADRs, several Cloud releases a day |
| How the model runs | CLI wrapper: ~20 agent CLIs (Codex, Claude Code, Gemini, Cursor, Copilot, any ACP agent, local models) | Pi agent runtime with 30+ providers, a ChatGPT sign-in or a local model | One engine, swappable brains: an API model through a Model Gateway, or a person's subscription CLI on a connected computer |
| The agent's computer | The host itself: one OS user, shared filesystem and browser for all agents; Xvfb on servers; full access by default | A sandboxed Linux workspace container plus a separate browser container per owner, inside a private VM on desktop | A connected machine running their runner; in Linux system mode each agent is its own OS user with its own desktop (VNC), Chrome profile, `/tmp`, rootless Podman and ports |
| Where authority lives | Mostly in the CLI's own sandbox; "Ask first" cards only for tools that support it | On the server: a checker must quote the owner's exact words before any outside action; approvals bind to the exact action | On the server: a Tool Gateway checks every call against DB scopes and a policy by "who is behind this run"; approvals are hash-bound and run exactly once |

## 2. What each one means by "an agent with a computer"

All three, and Atlas Crew, agree on the same shift: **an agent is a persistent identity that does work on a real computer, not a chat answer**. They differ on three axes, and those axes are where Atlas has to choose.

| Axis | OpenBot | Nod | agentes.chat | Atlas Crew today |
|---|---|---|---|---|
| **Identity** | Agent owns a workspace folder, a durable thread, an identity (`AGENTS.md`) and a private provider session. "A model is just a setting": switch Codex to Claude and keep everything | One owner, one thread, many task sessions; there is no "agent" to name | Agent is a DB row: role, instructions, brain, account, scopes, budget, owner, audience, approvals level, memory, skills, routines, computer | Crewmate: name, role, engine, permission mode, look, `memory.md`, task queue, automations, own Linux user |
| **Computer** | The host machine, shared by all agents | A container computer per owner, isolated from the host | A connected machine; per-agent OS user on Linux | The crew server; per-crewmate Linux user with nftables egress and a cgroup memory cap |
| **Authority** | Full access by default ("a product decision, not a security boundary") | "Policy lives on the server"; "authority comes only from you"; pages, mails and tool results are data, never permission | "The server owns authority; the brain owns nothing": CLIs reach tools only through a bridge with no tools of its own | hostd decides each tool call: rails, then the mode (Ask, Accept edits, Bypass), then task grants, then a person |
| **Work unit** | A message in a FIFO mailbox; side threads; routines; channels with one task owner | "Everything is a turn"; a task session per substantial job; turns are never retried | A run with a state machine, resumable after approvals; long work becomes background work, then a mission | A task, one at a time per crewmate, up to `max_agents` per server |
| **Team** | Agents message each other, channels with a lead and single-owner delegation | No team: one person | Agents are members; a router picks who answers; `ask_agent`/`tell_agent`, hand-offs, supervisors | No crewmate-to-crewmate messages; only "after crewmate" chaining |

The closest to Atlas is agentes.chat's runtime: CLI brains on a machine the user owns, a separate OS user per agent, every permission decided by the server. The closest to Atlas's trust model is Nod. OpenBot is the closest to Atlas's product shape (named agents on your own machine, reached from every surface) but with the opposite security default.

## 3. Where Atlas Crew is already ahead, and should stay

These are things none of the three does, or does as well. They are the base the missing pieces should be built on, not replaced.

- **Isolation on a server you own, with no hosted dependency.** Per-crewmate Linux users, nftables egress per uid, cgroup memory caps, a socket-activated spawn helper, systemd hardening of hostd, sudo only at install. OpenBot shares one user; Nod isolates per owner, not per agent; agentes.chat matches this only in its "system" mode. ([atlas-crew-today.md](atlas-crew-today.md) §3.13)
- **Permission decided per tool call, with rails no mode removes.** Protected paths (`.git`, agent config, shell rc, `.env*`, keys), destructive commands always ask, untrusted text from GitHub and webhooks quoted and never allowed to run unasked. OpenBot defaults to full access. ([approval.rs](../../../crates/atlas-host/src/crew/approval.rs))
- **Guards against the agent's own output** before work is called done: weakened tests, stray files, invented issue numbers, unproven claims, and the UI gate that needs real captures. None of the three has an equivalent.
- **GitHub as a first-class teammate surface:** a per-crew GitHub App with per-repo short-lived tokens, a `gh` shim, a git credential helper, signed webhooks with reconciliation, co-author attribution. agentes.chat is the only one close (GitHub App plus git proxy).
- **The dev environment underneath.** A crewmate works on projects that Atlas itself runs (`atlas.yaml`, services, logs, MCP status tools, worktrees, captures of the running app). None of the three knows the project it works on beyond files and a browser.
- **Voice that stays on the user's machines:** whisper.cpp on the crew server, on-device recognition on the phone, spoken replies, voice answers decided by hostd with exact phrases.
- **Operations:** install, update with rollback, export/import, pause/resume, uninstall, all over SSH with a preflight that names every missing piece.

## 4. What is missing

Each item says what the others do (with the report section that proves it), what Atlas has today, and what it would take in Atlas. **Size** is a rough guess: S (days), M (one to two weeks), L (several weeks). **Priority** is P0 (blocks the comparison or a known risk), P1 (core parity), P2 (breadth).

### 4.1 Core runtime

| # | Missing | Who does it | Atlas today | What it would take | Size | Pri |
|---|---|---|---|---|---|---|
| C1 | **Parallel work per crewmate.** One crewmate runs several tasks at once (side threads, task sessions), with a per-crewmate and a server-wide limit | OpenBot side threads, 4 per agent by default ([openbot.md](openbot.md) §3.1); Nod task sessions with no fixed limit ([n8n-nod.md](n8n-nod.md) §3 #2); agentes.chat background work, 3 per agent | One task at a time per crewmate (`RunnerMap`), `max_agents` per server | Runner keyed by task, not crewmate; one worktree per running task (the desktop already does worktree-per-task); per-crewmate concurrency in `crewmate.json`; board and phone show several running tasks | L | P1 |
| C2 | **Steer a running task.** Send a message into a running turn: correct it, ask for status, stop it with a reason the agent sees | OpenBot steer and edit-queued-message; Nod injects queued messages after each step; agentes.chat steering with live steer | A message to a busy crewmate becomes a new queued task | Route a message to the running engine (headless: next turn input; PTY: write to the terminal); "stop with a reason" handed to the agent | M | P1 |
| C3 | **Memory-aware admission.** Start new turns by available memory, give a person's message priority over automations, unload idle engines | OpenBot reads cgroup v2 every 5 s and reserves 300 MB per turn, person first ([openbot.md](openbot.md) §3.7); agentes.chat admits by `MemAvailable` | Fixed `max_agents` (default 4), per-crewmate `memory.max` | A scheduler in hostd that reads the crew slice's memory, keeps a reserve, orders the queue (owner messages before automations) | M | P1 |
| C4 | **Engine failover and plan limits.** When a plan hits its usage limit or the provider errors, move to the next engine or model, return when the hold ends; never retry a turn that already acted | OpenBot up to 8 backup models with failure classes; agentes.chat parks long work at 99% of a plan window | A crewmate has one engine; a usage limit fails the task | `engines: [..]` ordered per crewmate; classify engine exits (limit, auth, network); retry only if no tool ran; show the switch in the chat | M | P1 |
| C5 | **More engines through one protocol.** Gemini/Antigravity, Cursor, Copilot, Grok, Kimi, Hermes, local models | OpenBot ~20 CLIs via ACP plus pinned managed copies; agentes.chat 7 brains | Claude Code, Codex, OpenCode, omp | An ACP (Agent Client Protocol) driver as the generic engine, so a new CLI is a table row; pinned, hash-checked engine installs per crew | M | P2 |
| C6 | **Handoff when the engine changes.** Switching engine keeps the work: a bounded summary of the thread goes to the new session | OpenBot 32k-character handoff ([openbot.md](openbot.md) §3.1) | Changing a crewmate's engine starts from the prompt and `memory.md` | A bounded handoff built from the task log and summaries | S | P2 |
| C7 | **Outcome you can trust after a crash.** An outside action interrupted mid-way is shown as "may have run" and checked before any retry; approved actions never run twice | Nod claim rows: Running, Submitted, Uncertain ([n8n-nod.md](n8n-nod.md) §3 #18); agentes.chat `executed_at` claim on the approval | A task resumed after a restart re-runs the prompt (up to 2 times); pending approvals are lost | Record a claim before each approved outside call; on resume, mark unfinished ones uncertain and ask | M | P1 |
| C8 | **Pending approvals survive a restart** | Nod and agentes.chat keep approvals in the DB | Lost on hostd restart ([atlas-crew-today.md](atlas-crew-today.md) §3.3) | Persist held calls with the task; re-offer them on resume | S | P1 |

### 4.2 Trust, approvals and authority

| # | Missing | Who does it | Atlas today | What it would take | Size | Pri |
|---|---|---|---|---|---|---|
| T1 | **"Who is behind this run" as one rule.** A person, an automation, or nobody; what each may do is one table, applied to every tool | agentes.chat `authority.ts` with policy by authority ([agentes-chat.md](agentes-chat.md) §4.4) | Rails for GitHub/webhook tasks (`no_auto`), mode per crewmate | Make the task's origin (owner message, automation, external event) an input of `approval.rs`, with a documented table | M | P1 |
| T2 | **Approvals bound to the exact action.** The card holds a hash of the exact call; it runs once, as approved; a changed instruction invalidates it | Nod (bound to saved action and arguments), agentes.chat (`action_hash`, exactly once, ordered) | Allow once / for this task / deny per tool call | Hash the call in the held request; refuse a resumed call whose arguments differ | S | P1 |
| T3 | **Owner rules and standing instructions.** "Always ask before email outside my company", "whenever X sends an invoice, forward it": rules that apply to every task and automation | Nod owner rules and standing instructions ([n8n-nod.md](n8n-nod.md) §3 #16-17) | Mode per crewmate, `allow_bypass` per crew | A rules list per crew and per crewmate, applied after rails and before the mode; changing a rule asks | M | P2 |
| T4 | **Server-side check against the owner's words.** Before an outside action, a checker must quote the owner's message that allows it; the server verifies the quote | Nod trust check ([n8n-nod.md](n8n-nod.md) §3 #14) | Per-tool modes, no semantic check | An optional checker in hostd for outside actions (sends, deletes, payments), using the crewmate's own engine at low effort, with the quote verified against the task's messages | L | P2 |
| T5 | **Autonomy brakes and stretch limits.** Costly steps (a PR, installs, more than N calls) need a person's go when nobody asked; agent-started chains stop after a budget | agentes.chat brakes and stretches ([agentes-chat.md](agentes-chat.md) §3) | 2 h cap per turn, chain cycle guard | Per-task limits (calls, minutes, cost) with a "continue?" card | S | P1 |
| T6 | **Delegated approvers and roles.** Someone else answers a crewmate's approvals while the owner is away; per-member access to a crewmate (none, view, chat, manage) | agentes.chat delegated approvals and roles; OpenBot per-agent access levels and OIDC SSO | One owner; phone scopes (`read`, `approve`) per paired device | Members of a crew with roles, per-crewmate access, delegation; this is the "Crew for a team" step | L | P2 |

### 4.3 Secrets and credentials

| # | Missing | Who does it | Atlas today | What it would take | Size | Pri |
|---|---|---|---|---|---|---|
| S1 | **The model never sees a secret.** Secrets are typed or injected by the platform at the point of use, redacted everywhere | Nod Vault (server types into the field after an exact origin check); agentes.chat secrets encrypted to the runner, typed by it; OpenBot Vault shows only a card's last 4 digits | `secrets.json` is injected as environment variables into the agent, so the agent can read them ([launch.rs:144](../../../crates/atlas-host/src/crew/engine/launch.rs)) | Secret references instead of values: a helper the agent calls that performs the authenticated request or fills the field, with values only in hostd; keep env injection as an explicit, labelled opt-in | M | P0 |
| S2 | **Ask for a secret in a safe card.** The agent asks; the person types it into a private form; it never enters the chat or the transcript | agentes.chat `request_secret`; OpenBot `request_api_key`; Nod private pages | Secrets are set from Desktop settings | A `request_secret` MCP tool and a card on Desktop and phone that writes straight to the crewmate's secrets | S | P1 |
| S3 | **Password manager sources** | 1Password and Bitwarden in all three | None | A read-only connector that resolves references at use time | M | P2 |

### 4.4 Apps, integrations and MCP

| # | Missing | Who does it | Atlas today | What it would take | Size | Pri |
|---|---|---|---|---|---|---|
| A1 | **Connect any MCP server to a crewmate.** URL plus OAuth (or token), tools classified read/write/ask, a subset per crewmate, every call through the approval path | All three. agentes.chat: 58 one-click presets, MCP-native OAuth (RFC 9728/8414/7591/8707, PKCE), tools auto-classified ([agentes-chat.md](agentes-chat.md) §3); Nod: deferred loading with `tool_search` | The engine's MCP config has only Atlas's own server (`mcpServers: {atlas: ...}`, [launch.rs:471](../../../crates/atlas-host/src/crew/engine/launch.rs)) | A crew-level MCP registry in hostd (servers, OAuth tokens kept by hostd, a token broker like the GitHub App helper), a per-crewmate allowlist, calls proxied through hostd so rails and modes apply | L | P0 |
| A2 | **App connections for non-coding work** (Gmail, Calendar, Drive, Slack, HubSpot...) | Nod and agentes.chat through Composio; OpenBot 254 MCP apps in a marketplace | None (GitHub and Telegram only) | On top of A1: presets for the common apps, and optionally Composio as one connector | M | P1 |
| A3 | **n8n as a first app.** A crewmate builds, tests and fixes n8n workflows as drafts through n8n's instance MCP server, never touching n8n credentials | Nod ([n8n-nod.md](n8n-nod.md) §6.3) | None | A preset of A1 for n8n's `/mcp-server/http` with OAuth; drafts only | S (after A1) | P1 |
| A4 | **Crew as an MCP server for other AI apps.** Claude, ChatGPT, Cursor call `send_to_crewmate`, `wait_for_reply`, `list_crewmates` | agentes.chat remote MCP with its own OAuth 2.1 server ([agentes-chat.md](agentes-chat.md) §3); n8n's MCP Server Trigger | `atlas mcp` serves the dev environment; crewmates are not exposed | A remote MCP endpoint on the crew server (behind the same operator-supplied HTTPS as webhooks) with OAuth and per-client scopes | M | P2 |
| A5 | **Connector events as triggers** (a new email, a Stripe event, a Linear issue) | agentes.chat registers webhooks at the service through the connection; Nod polls Gmail and Calendar | GitHub, webhook, file, service, schedule | Event subscriptions per connection in A1/A2, feeding the existing trigger path | M | P2 |

### 4.5 The crewmate's computer: browser and desktop

| # | Missing | Who does it | Atlas today | What it would take | Size | Pri |
|---|---|---|---|---|---|---|
| B0 | **Sandbox the capture browser first.** It runs as the crew user with `--no-sandbox`; a page exploit reaches every crewmate | Nod runs the browser in its own container with no capabilities; agentes.chat per-agent Chrome under the agent's user | Known gap N4 ([atlas-crew-today.md](atlas-crew-today.md) §5) | Run the browser as the crewmate's own user (or a dedicated user) with Chromium's sandbox on, or in a namespace/container with no capabilities; cap it with the crewmate's cgroup (gap L2) | M | P0 |
| B1 | **An interactive browser for the agent.** Navigate, read an accessibility snapshot with refs, click, type, fill forms, download, keep tabs between turns, per-crewmate profile | All three (OpenBot ~35 browser tools over CDP; Nod `agent-browser`; agentes.chat `browser_do` with a judge) | `capture` and `locate`: screenshots with up to 20 scripted steps, no persistent session | A browser service per crewmate on top of B0, exposed as MCP tools, sessions kept between turns | L | P1 |
| B2 | **Live handoff.** When a site needs a login, 2FA or a captcha, the person takes over the live tab from Desktop or the phone, then hands it back | OpenBot `request_takeover`; Nod live tab in chat; agentes.chat JPEG stream and takeover | None | Frames streamed over the existing hostd connection, input back, a "needs you" card | M | P1 |
| B3 | **A desktop the crewmate can use and you can watch.** A virtual display per crewmate, accessibility-tree computer use, live view on Desktop and phone | OpenBot Xvfb plus `cua-driver` and Sunshine; agentes.chat Xtigervnc plus XFCE per agent, noVNC on the phone | None | Optional per-crewmate Xvfb/VNC under its Linux user, a computer-use MCP driver, a viewer in Desktop and Mobile | L | P2 |
| B4 | **Rootless containers per crewmate** (the agent runs `docker compose` for its project) | agentes.chat rootless Podman per agent user | Not available inside an isolated crewmate; loopback is dropped for agents | Rootless Podman per crewmate uid, loopback per crewmate (agentes.chat uses an eBPF loopback guard) so local test servers work | M | P1 |

### 4.6 Memory, skills and knowledge

| # | Missing | Who does it | Atlas today | What it would take | Size | Pri |
|---|---|---|---|---|---|---|
| M1 | **Structured memory with sources.** Notes with a type (fact, preference, lesson, person), where they came from, a trust level, and a person's approval before anything becomes an instruction | Nod: notes only from owner messages, each with a verified quote, 0/216 planted false memories; agentes.chat: trust and evidence kinds, outcomes as ground truth, human-gated promotion | One `memory.md`, 64 KiB, edited by the agent and the apps | A memory store per crewmate (and per crew) with source and trust; outside content can only produce low-trust notes; a review card to promote | M | P1 |
| M2 | **Background memory review and lessons from corrections.** After N turns or a refused approval, a pass proposes memory changes | OpenBot every 10/20/40 turns; agentes.chat reflect and consolidate; Nod hourly repair | None | A low-effort review turn after tasks, writing proposals to M1 | S (after M1) | P2 |
| M3 | **Skills the crewmate keeps.** Versioned `SKILL.md` per crewmate and per crew, loaded on demand, proposed by the agent, locked by owner edits | All three (OpenBot skill learning, Nod learned skills, agentes.chat skills with versions) | Atlas installs skills for coding agents on the user's machine (`atlas skills`), not per crewmate | Skills folder per crewmate on the crew server, written to each engine's skills directory, with revisions and an owner lock | M | P2 |
| M4 | **Shared knowledge between crewmates.** Team memory, a shared folder, shared data | OpenBot shared folder and shared SQLite tables; agentes.chat wiki, Drive, team memory | Each crewmate is isolated; no shared space | A crew-shared folder (setgid group, as agentes.chat does) and crew-level memory | S | P2 |

### 4.7 Working as a team

| # | Missing | Who does it | Atlas today | What it would take | Size | Pri |
|---|---|---|---|---|---|---|
| W1 | **Crewmates talk to each other.** Ask a teammate and wait for the answer, tell it something, hand work off and get it back | OpenBot `send_message` and file transfers; agentes.chat `ask_agent` (waits, 2 deep), `tell_agent`, hand-offs that come back | Only "after crewmate" chaining with the summary | `ask_crewmate` / `tell_crewmate` / `hand_off` MCP tools in hostd, with depth limits, loop guards and audit | M | P1 |
| W2 | **Rooms with several crewmates and people.** A shared room with a purpose, a lead, one task owner at a time and explicit delegation | OpenBot channels; agentes.chat channels where a router picks who answers | One chat per crewmate | A crew room on Desktop and the phone, with a router (rules first, then a small model) and a lead crewmate | L | P2 |
| W3 | **Missions: work that lasts days.** A plan as a living checklist, reviews, decisions, a final report | agentes.chat missions; OpenBot workflows | A task with a 2 h turn cap | A mission object grouping tasks with a plan file the crewmate updates, decision cards, and resume across turns | L | P2 |
| W4 | **A "right hand" that sets up the crew by talking.** Creates crewmates, automations and connections through cards with undo | agentes.chat Lodi; OpenBot creates agents from one prompt | Crewmates are created in Desktop forms or on the phone | A built-in crewmate with tools to manage the crew (`crewmate_create`, `automation_set`...) gated by owner cards | M | P2 |

### 4.8 Automation

| # | Missing | Who does it | Atlas today | What it would take | Size | Pri |
|---|---|---|---|---|---|---|
| R1 | **Missed-run policy.** What happens to a schedule that was due while the server was down: run once, skip, run each | n8n durable scheduler with a misfire policy; Nod runs a missed schedule once and notes the skipped count | Missed ticks are skipped (`MissedTickBehavior::Skip`, [schedule.rs:77](../../../crates/atlas-host/src/crew/schedule.rs)) | Persist `last_fired`; on start, apply the automation's policy (default: run once, say how many were skipped) | S | P1 |
| R2 | **Quiet runs.** A scheduled run can end with "nothing to report" and notify nobody; or report a failure though the turn completed; or pause its own automation | OpenBot `routine_no_update`, `routine_report_failure`, self-pause; agentes.chat `report: when_new` | Every run is a task that notifies | `report: always | when_new` per automation and three MCP tools for the run to say so | S | P1 |
| R3 | **Watches that skip when nothing changed**, without calling a model | Nod watches with source change tokens | File trigger polls mtime; schedules always run | A cheap "did it change" probe per automation (a command, an HTTP ETag, a GitHub query) before starting the engine | S | P2 |
| R4 | **Signed webhooks in the standard format** (HMAC over timestamp, delivery id and body; Standard Webhooks; dedupe; filters) | OpenBot (HMAC, Standard Webhooks, JSON-Pointer filters, 7-day dedupe); agentes.chat optional HMAC | Token in the path (`/crew/<id>/hook/<token>`); GitHub deliveries are signed | Optional HMAC secret and Standard Webhooks support on the generic webhook trigger, with filters and dedupe | S | P1 |
| R5 | **Local scripts API and CLI.** `atlas crew run <crewmate> "..." --wait`, answer a card, wait for an event | OpenBot local scripts API and `openbot-cli` | `atlas crew` manages the server, not tasks | `atlas crew task`/`ask`/`wait` subcommands on the existing hostd protocol | S | P1 |
| R6 | **A workflow graph.** Agent, model, condition and approval steps, loops, versions, run-from-step | OpenBot workflows canvas; n8n itself | Chaining only | Do not build a canvas: use n8n (A3) for graphs, and keep Crew's automations as triggers plus missions (W3). Recorded here so the choice is explicit | - |, |

### 4.9 Cost, usage and limits

| # | Missing | Who does it | Atlas today | What it would take | Size | Pri |
|---|---|---|---|---|---|---|
| U1 | **Budgets that stop spending.** A monthly budget per crewmate and per crew, checked before each turn; a check-in card after a task spends N | Nod monthly limit, $5 task check-in, cost reserved before each request; agentes.chat budget per agent checked before every call | `usage.jsonl` per crewmate, shown in the apps; no limit | Read cost or tokens per turn from each engine; budgets in `crewmate.json` and `crew.json`; refuse new turns over budget with a card | M | P1 |
| U2 | **Plan usage meters.** How much of each Claude/ChatGPT plan window is used, per account | OpenBot plan usage clocks; agentes.chat reads plan usage from the CLIs | None | Read the CLIs' rate-limit events and usage commands; show per crew server | S | P2 |
| U3 | **Cost per task and per automation**, including projected monthly cost of a schedule | Nod per-schedule projected cost; issue #106 review asked for cost per task | Per-crewmate usage only | Attach usage to the task record; sum per automation | S | P2 |

### 4.10 Reach: where people talk to the crew

| # | Missing | Who does it | Atlas today | What it would take | Size | Pri |
|---|---|---|---|---|---|---|
| X1 | **Push notifications live** | All three ship push | Built but not deployed: the relay is not up and the app has no Firebase config | Link billing to `atlas-codeatlas`, deploy the relay, set APNs, release Atlas Mobile 1.0.1 (`docs/mobile.md`, "Bringing push up") | S | P0 |
| X2 | **WhatsApp** | OpenBot (WhatsApp Web device, approvals by reply); Nod (Baileys or the Cloud API); agentes.chat (bridge to the built-in assistant) | Telegram only | A `MessagingDriver`-style abstraction over the Telegram code, then WhatsApp Cloud API (official) as the second driver; Baileys only as a labelled opt-in | M | P1 |
| X3 | **Slack and Discord** | OpenBot (with an orchestrator agent that routes) | None | Drivers on the same abstraction; a thread maps to one task | M | P2 |
| X4 | **Web client for the crew** (no app install), and an iPhone Live Activity for running tasks | OpenBot web app and Live Activity; agentes.chat web | Desktop and the phone app | A small web UI served by hostd behind the operator's HTTPS; Live Activity in Atlas Mobile | M | P2 |
| X5 | **Live voice** (talk with a crewmate in real time) | OpenBot realtime voice over WebRTC | Dictation and spoken replies | Out of scope until a local realtime engine exists; recorded so the choice is explicit | - | P2 |

### 4.11 Observability, evals and quality

| # | Missing | Who does it | Atlas today | What it would take | Size | Pri |
|---|---|---|---|---|---|---|
| E1 | **Evals for crewmates.** Scenario runs against simulated services with a judge, pass@k, a regression gate in CI | Nod publishes 98% task completion, 0/678 unsafe actions, 93.2% LongMemEval-S, with open runners; agentes.chat `eval:*` suites; n8n dataset evaluations | Unit and integration tests of hostd; guards on finished work | An eval runner in the repo (fake GitHub, fake webhook, fake mail) running real engines on fixed tasks, scoring safety (no rail bypassed) and completion | L | P1 |
| E2 | **A usage and actions page.** Every outside action, send, sign-in and approval with who allowed it and what it cost; CSV export | Nod Usage page; agentes.chat team actions and task manager | Audit logs per crewmate and crew, shown in Desktop | A crew-wide activity view on Desktop and the phone built from the existing audit, with filters and export | S | P2 |
| E3 | **Live status line while working** (last few tool calls, no extra model call) | Nod; agentes.chat `say_now` | Task state and the board | Stream the last N tool calls from the hub to the chat | S | P2 |

### 4.12 Operations

| # | Missing | Who does it | Atlas today | What it would take | Size | Pri |
|---|---|---|---|---|---|---|
| O1 | **Import from other platforms** (OpenBot, OpenClaw, Hermes, Grok Bot exports) | agentes.chat imports from all of these, including OpenBot's SQLite; OpenBot imports Grok Bot and Hermes | Atlas's own export/import | Importers that map an agent's instructions, memory, skills and routines to a crewmate | M | P2 |
| O2 | **Hook port held by systemd** so a leftover process can't take it during a restart | none (Atlas-specific gap S-A) | Known gap ([atlas-crew-today.md](atlas-crew-today.md) §5) | systemd `ListenStream` for the hook port and leftover cleanup | S | P1 |
| O3 | **A sleep/wake model for small servers** (wake for automations and incoming messages) | OpenBot hosted servers and agentes.chat Cloud | Always on | Only relevant if Atlas ever offers hosted crew servers; recorded, not proposed | - |, |

### 4.13 Documentation

| # | Missing | Who does it | Atlas today | What it would take | Size | Pri |
|---|---|---|---|---|---|---|
| D1 | **A guide per job**, not one reference page: "a crewmate that answers GitHub issues", "a nightly dependency updater", "a support crewmate on Telegram" | OpenBot guides per surface; Nod "use" pages per job | `docs/crew.md` is one 1,093-line reference | Split `docs/crew.md` into concepts, how-tos per job and the reference; keep the reference as is | M | P1 |
| D2 | **A published security model with its limits and a comparison** | Nod security model, security review and an honest compare table, including where others are ahead | "Security model" section and the "Not covered" list in `docs/crew.md` | One page with the threat model, what each layer stops, what it doesn't, and a comparison like Nod's | S | P1 |
| D3 | **Architecture decisions written down** (why one Linux user per crewmate, why hostd decides, why no hosted servers) | agentes.chat 201 ADRs with code pointers; OpenBot architecture notes with a "Not confirmed" section each | Decisions live in PRs and issues | `docs/adr/` for Crew decisions from now on, starting with the ones this study makes | S | P2 |
| D4 | **A glossary and llms.txt for Crew** | All three publish `llms.txt` and glossaries | Glossary terms exist in `i18n/glossary.yaml`; the site has `llms.txt` | Add Crew terms (crewmate, crew server, automation, permission mode, rail) to the site's glossary and llms files | S | P2 |
| D5 | **Fix the two docs that disagree**: the phone audit screen (present in `docs/mobile.md`, absent per `docs/crew.md`) | - | Contradiction found by this study | Check the app, fix whichever doc is wrong | S | P1 |

## 5. What not to copy

- **Full access by default** (OpenBot). Its own docs call it "a product decision, not a security boundary", and Linux confinement for most CLIs fails closed. Atlas's per-call decisions and rails are the reason to choose Crew.
- **A shared computer for all agents** (OpenBot). Atlas already isolates per crewmate; keep that the default.
- **A closed bridge or a classifier service in the trust path** (agentes.chat's WhatsApp bridge and Jev routing; Nod's optional Jev checker). Anything that decides permission must run on the crew server or be the crewmate's own engine.
- **A workflow canvas** (OpenBot, n8n). n8n already does it; connect to it (A3) instead.
- **Unofficial WhatsApp Web clients as the default** (OpenBot, Nod's Baileys mode). They risk the user's number; offer the Cloud API first.
- **Noncommercial or source-available licensing** (OpenBot's PolyForm). Atlas is MIT or Apache-2.0; nothing here needs a license change.

## 6. Suggested order

Built on what exists, smallest risk first. Each phase is independently shippable.

1. **Close the open risks (P0):** B0 browser sandbox, S1 secrets never in the model's environment, X1 push live, and A1 the MCP registry (everything after depends on it).
2. **Trust you can rely on (P1):** C7 uncertain outcomes, C8 approvals survive restarts, T2 hash-bound approvals, T1 authority table, T5 autonomy brakes, S2 secret cards, U1 budgets.
3. **More work per crewmate (P1):** C1 parallel tasks, C2 steering, C3 memory-aware admission, C4 engine failover, B4 rootless containers per crewmate.
4. **Automations that behave (P1):** R1 missed runs, R2 quiet runs, R4 signed webhooks, R5 scripts CLI.
5. **Beyond code (P1):** A2 app presets, A3 n8n, B1 interactive browser, B2 live handoff, X2 WhatsApp, M1 structured memory.
6. **A crew that works together (P1/P2):** W1 crewmate-to-crewmate, then W2 rooms and W3 missions.
7. **Prove it (P1):** E1 evals in CI, with the safety score as a gate, then D1/D2 guides and the security page.
8. **Breadth (P2):** C5 ACP engines, M2/M3/M4 memory review and skills, X3/X4/X5, A4/A5, T3/T4/T6, O1, D3/D4, U2/U3, E2/E3.

## 7. Open questions for the maintainer

- **Who is Crew for next:** one developer's agents (today), a team sharing a crew (T6, W2), or people who don't code (A2, B1, W4)? The order above assumes developers first, then teams.
- **Is the browser in scope** beyond UI proof? B1-B3 are the largest items; they are what makes "errands" possible, but they also widen the attack surface.
- **Should Atlas ever host crew servers?** Every competitor sells hosting. Atlas's model is "your own server"; O3 stays recorded, not proposed.
- **A semantic trust checker (T4)** needs a model call per outside action. Using the crewmate's own engine keeps it on the user's subscription, but it adds latency and cost.

## Method and limits

- Four research agents worked in parallel on 2026-11-20, one per product plus one on Atlas's own code; their full reports are the other files in this folder. This summary was then checked against the code for every "Atlas today" claim it depends on: one task per crewmate (`runner/mod.rs`), secrets as environment variables (`engine/launch.rs:144`), only Atlas's own MCP server (`engine/launch.rs:471`), missed schedules skipped (`schedule.rs:77`), no budget and no crewmate-to-crewmate tools (no match in `crates/atlas-host/src/crew`).
- The open crew issues were listed with `gh issue list --search crew --state open`: #527 and #528 (parents whose children are closed), #348 (agent coordination), and #289 (Linux roadmap, mentions crew).
- None of the three products was installed or run. Their own benchmark numbers (Nod's evals) are self-reported.
- The products move daily (several releases a day). The reports are a snapshot of 2026-11-20.
