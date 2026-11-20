# Study: agentes.chat (github.com/agentes-chat/agentes-chat)

Research done 2026-11-20 on a shallow clone (`git clone --depth 50`, HEAD `98c39c4`, "Merge pull request #1024"), the GitHub API (repo, issues, releases, discussions, org) and the public site (agentes.chat, its `.md` mirrors, `llms.txt`, `changelog.md`, docs pages). I did not run the code or install anything. Paths below are repo-relative; "ADR NNNN" = `docs/adr/NNNN-*.md`.

---

## 1. What it is, who it is for, business model, maturity

**One line.** "The open-source, self-hosted Slack alternative where your team and AI agents work together" (README). A multi-tenant team chat (channels, threads, DMs) where AI agents are workspace members with a role, an owner, a budget, scoped tools, memory, and a computer + browser of their own. Agents think through API keys **or through people's own subscription CLIs** (Claude Code, Codex, Cursor, OpenCode, Grok Build, Antigravity) run on a connected computer.

**Who for.** Teams of any kind, explicitly not only engineers (role templates: Dev, Ops, QA, PM, Support, Sales, Marketing, Content, Data, Finance, People, Research, Assistant; https://agentes.chat/about.md). Also a "Personal mode" for one person (ADR 0159). They position against "single-player agent apps such as Grok Bot, ChatGPT dots, OpenBot.run and OpenMausBot (all of which it imports from)" (README) and against OpenClaw/Hermes (homepage comparison table, https://agentes.chat/).

**What it is not** (their words, https://agentes.chat/about.md): not a customer-facing chatbot builder, not a workflow builder, not a model provider, not a Slack/Teams add-on ("there's no Slack connector").

**Company / licensing.**
- AGENTESCHAT, LLC, Wilmington DE (site footer). ADR 0002's 2026-11-06 amendment still names the earlier entity "Codari Global, LLC".
- AGPL-3.0-only for everything, plus a NOTICE §7(b) attribution term ("Based on agentes.chat by AGENTESCHAT, LLC" must stay in the About screen) and an app-store permission (README, `NOTICE`). `packages/setup` is MIT.
- Dual license: a commercial license by contract for closed forks, rebranding or companies that ban AGPL (`COMMERCIAL.md`, ADR 0180). A CLA (Apache-ICLA based, CLA Assistant Lite) has been required since 2026-11-13 (ADR 0180). No `ee/` folder and no license key: "Cloud-only behavior is public code switched on by env vars … or a separate service over the network, never a private in-process module" (ADR 0002 §5, `CLAUDE.md` Non-negotiables).
- Trademarks are reserved (`TRADEMARKS.md`).

**Pricing (Cloud)** (https://agentes.chat/ and https://agentes.chat/pricing.md): Personal $29/mo (1 person, unlimited agents, always-on cloud computer, 50 GB), Team $49/mo (up to 10 people, 15 with extras at $4/person), Business $99/mo (25, up to 50). "AI usage is never on our bill": you bring your own plan or keys. Self-host is free. There is a Partner Program for agencies that set up agents for clients (https://agentes.chat/partners.md) and a referral service (ADR 0192).

**Maturity / activity** (GitHub API, 2026-11-20):
| Fact | Value | Source |
|---|---|---|
| Repo created (public) | 2026-11-09 | `gh api repos/agentes-chat/agentes-chat` |
| First ADR | 2026-10-14 (ADR 0001-0011) | `docs/adr` |
| Open to everyone (Cloud + self-host) | 2026-11-11 | org profile README (`agentes-chat/.github`) |
| Stars / forks | 1,829 / 350 | GitHub API |
| Commits on main | ~10,549 | commits API `rel="last"` page |
| Main contributors | `heliocosta-dev` 7,355 commits, `wescld` 3,014, `claude` 38, then ~a dozen outside contributors with 3-27 each | contributors API |
| Issues | 157 created (many bug/feature reports from self-hosters, mixed English/pt-BR) | `gh issue list` |
| Discussions | 7 (all "Ideas") | GraphQL |
| Releases | server `v0.1.2`…`v0.1.10` (2026-11-15→11-20), desktop `desktop-v0.6.0`…`0.8.14` (2026-11-10→11-20) | `gh release list` |
| Cloud changelog | Date-based versions (`2026.11.20.6`), several releases per day, tagged App / Computers / Plans | https://agentes.chat/changelog.md, `changelog.json` |
| ADRs | 201 files (0001-0207, some gaps), several of them 60-140 KB | `docs/adr` |
| Code size | ~1M lines TS/TSX/Rust: engine 267k, web 210k, mobile 184k, client 75k, runner 64k, server 49k | `wc -l` by package |
| DB | 179 tables, 305 migrations | `packages/db/src/schema`, `packages/db/migrations` |

It is very young (about 5 weeks of public history) but huge and fast-moving. `CLAUDE.md` (62 KB) is written for "contributors and their coding agents", and the commit style (pt-BR UI words in English prose, very long, precise ADRs) suggests heavy use of coding agents [INFERENCE]. Brazilian origin is clear: the product name is Portuguese, UI terms in ADRs are pt-BR ("Aprovar sempre", "Quem pensa", "Esperando você"), and pt-BR is the first translation (ADR 0122).

---

## 2. The concept: agent, computer, "agent computing"

**Agents are teammates, not bots.** "Agents are members of the workspace, not a sidebar bot: they read the channel, answer when it's theirs to answer, and hand work back and forth" (README). An agent is a DB row with: role, instructions, a brain (model or CLI), a credential/account, connectors + scopes, a monthly budget, a responsible person (owner), an audience (who may talk to it), an approvals level, memory, skills, routines and a computer (`CLAUDE.md` Concepts; `packages/db/src/schema/agents.ts`).

**Mental model, in their own layering:**
1. **Conversation is the interface.** Everything happens in channels/threads/DMs, with cards for approvals, questions, choices, proposals, polls, live blocks. Long work becomes a thread, then a "mission" (a thread with a plan), then its own channel (ADR 0029 §1 "the ladder").
2. **One engine, swappable brains.** "One engine runs every agent turn. What changes is only the brain that thinks. Everything else, context, run states, tools, approvals, transcript, streaming, usage, events, lives in the engine" (ADR 0032). Brains: `model | claude_code | codex | cursor | opencode | grok | antigravity` (`packages/engine/src/brains/types.ts`).
3. **The server owns authority; the brain owns nothing.** For CLI brains "the executor on the machine is the brain. Our side is the interface and the guard … The executor never grants anything: approvals, reviews, access and merges stay with the server" (ADR 0029). Tools exist only in the server's Tool Gateway. The runner's MCP server is "a bridge with no tools of its own" (ADR 0032 §2).
4. **The computer is a separate, connected thing.** "An agent's computer is always a connected computer" (ADR 0077): any machine running their `agentes-runner` over an outbound WebSocket. That can be the server itself ("the server's computer", ADR 0060 §3), a VM/laptop someone pairs (ADR 0025), the desktop app's own machine (ADR 0066/0165), or, on Cloud, "a computer of its own, always on" per workspace from an external provisioner (ADR 0056 §10, https://agentes.chat/docs/workspace-computer.md). On Linux in `--system` mode **each agent is its own OS user** (`ag-<id8>`) with its own home, desktop (Xtigervnc+XFCE), Chrome profile, sign-ins, /tmp, containers (rootless Podman) and loopback ports (ADR 0057, 0058, 0096, 0169, 0185).
5. **People's AI subscriptions are first-class compute.** An account (Claude, ChatGPT, Cursor, Grok, Gemini, coding plans) is signed in *on the computer*, belongs to the person who added it, and answers only its owner unless they turn on "Atende outras pessoas" (ADR 0041, 0059).
6. **A built-in "right hand" (Lodi)** in every workspace that sets up and runs the team by talking, acting as the person in their own DM (ADR 0137, 0175, 0177).
7. **Safety by structure, not by prompt.** Scopes in the DB checked on every call; approvals bound to an action hash and run exactly once; one "who is behind this run" gate (`authority.ts`) that decides what person/routine/unattended runs may do (ADR 0015, 0053).

They do not use the phrase "agent computing". Their phrase for the computer is "the agent's computer" / "the workspace's computer": "Each agent has its own user on it, with its own folder, screen, browser and files. From an agent's page you can watch it work, and take control when you need to" (https://agentes.chat/docs/workspace-computer.md).

---

## 3. Feature inventory

| Feature | What the user can do | How it works underneath | Source |
|---|---|---|---|
| Channels, threads, DMs | Public/private channels, DMs with people and agents, threads from a message or a range; follow/close/archive | `channels`, `messages` (partitioned by month), `threads`; channel kinds `channel/dm/forum/task/file`; `insertMessageWithRuns` = one SQL statement that inserts the message and starts mention/DM runs | README; `packages/db/src/schema/chat.ts`; ADR 0009; `CLAUDE.md` "Hot path" |
| Agents | Create via a one-card dialog (role template, look, apps, skills, where it works, instructions); setup checklist in the agent's chat | `agents` row; `agent-setup.ts`; `NEW_AGENT_DEFAULTS` | ADR 0155, 0164 |
| Router (who answers) | Message with no @ is answered by 0-4 agents, picked automatically, with a receipt saying who picked | Code picks candidates (channel members with auto-reply); chain: Jev (TypeSafe classifier) → workspace's small model → the author's own plan via runner `brain.route` → code rules R1-R3 → nobody with reason; every decision stored in `routing_decisions` | ADR 0013, 0074; `packages/engine/src/router*.ts` |
| Bursts | Several quick messages = one run, one answer | `ROUTER_QUIET_MS` 2.5 s; a message mid-task updates the task | ADR 0112 |
| Steering | A message to a working agent can stop it, correct it, get an answer now or wait | `steering.ts` → `live-steer.ts` / runner `brain.steer` → waits → fast reply → triage | `CLAUDE.md`; changelog 2026.11.19.4 |
| `say_now` | Agent posts one line of what it will do before slow work | Tool, ≤200 chars, once per run, `meta.sayNow` | ADR 0207 |
| Approvals | Merge/deploy/rollback and connector-sensitive actions become a card; approve once, "Aprovar sempre", deny; run once exactly as approved; 24 h expiry | `approvals` row with `action_hash = sha256(canonical JSON)`, `executeApproved` re-checks hash + scope and claims `executed_at`; `approval_rules` for "always" | ADR 0015, 0173, 0182 |
| Approvals level per agent | Always ask / only risky (default) / never ask | `agents.approvals_level`; computer brains' own tools follow it (runner protocol 59) | ADR 0173 |
| Delegated approvals | Someone covers my approvals while I'm out | `approval_covers` | ADR 0139 |
| `request_approval` | Agent asks a role/person for a real approval | Tool → approval card + notification + push | ADR 0126 |
| Questions / choice cards | Agent asks a person; lettered options A/B/C, up to 5 questions | `ask_user` → `waiting_input`; `ask_choices`, `choice_answers` | ADR 0008 addendum, 0124 |
| Budgets & usage | Monthly budget per agent checked before every call; usage per person; plan usage meters per account | Model Gateway reserves budget, records tokens/cost with `org_id`+`agent_id`; `budget_reservations`, `usage_records`, `person_usage_months`; plan usage read from CLIs | ADR 0012, 0026, 0063, 0141 |
| Long work pauses near limit | Long work parks at 99% of a plan window or budget; "Continuar agora" | `planBinding`, `usage_pause` | ADR 0118 |
| Background work | A run "takes itself home": anchor message, updates in thread, report back; card on Tasks | `work_in_background` or safety net after 4 min / 20 calls; 3 per agent | ADR 0121 |
| Missions (beta) | Days-long work in a thread with a living checklist plan, reviews, decisions, participants | CLI executor in its own git worktree on the computer; `update_plan`, `request_review`, `ask_decision`, `complete_mission` as gateway tools | ADR 0029, 0044 |
| Task manager | See every agent's tasks, what each uses now, stop with a reason (agent is told) | `task-manager.ts`, polled 3 s | ADR 0128 |
| Agents talk to each other | `ask_agent` (private DM, waits, 2 deep) and `tell_agent` (note, no wait); results roll up | `agent_consultations`; `consultations.ts` | ADR 0034, 0048, 0095 |
| Agents keep going | No turn cap; "stretches" since last human action, held at 50 agent-started runs (10 unasked) or 20% of budget | `stretch_id`; `AGENT_MAX_HOPS` backstop 100 | ADR 0089 |
| Continuation controller | One function decides every turn end: done / continue / ask / wait | `decideTurnEnd` in `continuation.ts` | ADR 0097 |
| Autonomy brakes | Costly steps (PR, installs, >40 calls) need a person's go when nobody asked | `brakes.ts` | ADR 0131 |
| Agents propose | Propose a new agent, a change to a teammate's instructions/role/apps, a permission | `propose_agent`, `propose_instruction_change`, `request_permission` → cards decided by whoever may make the change | ADR 0081, 0092 |
| Right hand "Lodi" | Built-in agent per workspace; private DM per person acting as that person; builds the team, changes settings through cards with Undo; daily brief at 9h, weekly report to admins; welcomes people | `agents.builtin='right_hand'`, per-person seats (`right_hand_seats`) = per-person OS user on the workspace computer | ADR 0137, 0175, 0177 |
| Connectors (MCP) | Org connects an MCP server once (58 one-click presets for 56 vendors, or any URL); each agent gets a subset; tools classified read/write/approval/off | `connector_connections` (sealed), `agent_scopes`; MCP client (TS SDK, streamable HTTP); MCP-native OAuth (RFC 9728/8414/7591/8707, PKCE); tools auto-classified by annotations, verb, then a judge; custom tools hidden until a person confirms | ADR 0016, 0099, 0186; https://agentes.chat/about.md |
| Personal connectors | Connect a service just for yourself; agents use it only when you ask | Team / Personal sides | changelog 2026.11.18.10 |
| Composio | Google Drive/Calendar/Sheets/Docs, Slack, Teams, HubSpot, Salesforce, Zendesk, WhatsApp Business, socials… with a Composio key | Preset with `composio` way, Connect Links, explicit actions, per-call counts | ADR 0107; README |
| Native social/ads connectors | X, TikTok, Threads, Meta Ads, Telegram (bot token) | `connectors/native/*`; Telegram updates by webhook or polling into `telegram_updates`; deletes/bans ask | ADR 0170 (Proposed) |
| Postgres connector (beta) | Agents query a DB read-only; writes always ask with the exact SQL | One parsed statement (libpg-query) in a read-only transaction with time/row caps | ADR 0039 |
| Mailboxes (beta) | Agents read/draft/send from Gmail/Outlook (Composio) or IMAP/SMTP; send asks by default; inbound mail triggers routines | `packages/engine/src/mail/*`, IMAP `EXAMINE`/`BODY.PEEK`; MillionSend inboxes | ADR 0038, 0102 |
| GitHub App | Workspace's GitHub identity; agents commit as the app's bot with `Co-authored-by` | `git/github-app.ts`; short installation tokens | ADR 0033, 0156 |
| Git proxy | Agents' computers push through the server; branch policy `agentes/{handle}/*`, fast-forward only, no tags | Smart-HTTP proxy `/api/git/*`, per-computer HMAC credential; token never enters the machine; clones straight from GitHub with 1 h read token | ADR 0024, 0116 |
| Skills | `SKILL.md` from zip, GitHub link or editor, versions; loaded on demand; agents can update skills and learn new ones | `skills`, `skill_files`, `load_skill`; `run_skill_script` on the computer | ADR 0020, 0120 |
| Routines | Schedule ("every weekday at 9am"), events (`github.pr_opened`, `sentry.issue_created`, `issue.created`, `deploy.succeeded`, `email.received`, `webhook.received`), manual; delivers to channel/DM/Drive | `routines` (cron/rule/timezone/event, `report: when_new|always`, auto-pause after failures), `routine_runs` ledger, routine room + brief, no overlap, no silent skips | ADR 0053; `packages/engine/src/routines.ts` `ROUTINE_EVENTS`; `packages/db/src/schema/work.ts` |
| Webhook routines | A routine gets its own URL `/api/hooks/<token>`, optional HMAC signing, idempotency (GitHub delivery id) | `routine_hooks`, `routine_hook_calls` | ADR 0106 |
| Connector events as triggers | Server registers the webhook at the service through the connection | Filter before run, dedupe | ADR 0130 |
| Memory | Agents keep decisions, facts, preferences, lessons; learn from corrections and refused approvals; propose instruction changes a person accepts | `context_items` with trust/evidence/verification; reflect → consolidate → outcomes → human-gated promotion; Jev for semantic consolidation; hybrid Postgres search + optional embeddings | ADR 0017, 0028, 0120, 0167 |
| Person profile | One profile per person shared by every agent (≤30 facts), "what agents know about you" with delete | Extracted after runs, judged like memory | ADR 0052 |
| Wiki | Pages with history, `[[links]]` from anywhere, backlinks, graph, Obsidian import; agents write only on request | `wiki_pages`, `wiki_page_versions`, `wiki_links` | ADR 0023 |
| Drive | Files with versions, folders, trash, sharing; stored on disk, Postgres or S3 (or the org's own bucket) | `artifacts`, `artifact_versions`, `blobs`, `storage_configs` | ADR 0019, 0111, 0157 |
| Live files | Spreadsheets, presentations, canvases, documents edited together by people and agents | Yjs docs (`@agenteschat/live`), `/api/live` socket, `live_docs`/`live_doc_updates` + NOTIFY; agent tools `read_/edit_spreadsheet` etc. | ADR 0196, 0199, 0200 |
| Office files | xlsx/csv/pptx/docx open read-only, "edit as live file", export xlsx/pptx/pdf/docx | Server-side parsers | ADR 0198 |
| Image generation | Agents generate images into chat or a canvas | `IMAGE_CATALOG` through the Model Gateway | ADR 0197 |
| Tasks | Board with own columns, projects, several assignees; a task given to an agent moves as it works; a task's conversation is its own thread | `issues`, `issue_assignees`, `issue_projects`, channel kind `task` | ADR 0145, 0171, 0188 |
| Dashboards & widgets | KPI/goal/bars/line/list widgets kept current by an agent's own routine | `dashboards`, `dashboard_widgets`; widget routine runs read-only and calls `update_widget` | ADR 0135 |
| Live blocks & polls | Agent shows interactive HTML or a spec block in a sandboxed frame; polls | `show_block`, `start_poll` | ADR 0113 |
| Calendar | Events, find free time, proposals | `calendar_events`, tools `find_free_time`, `propose_event` | tool list in `packages/engine/src` |
| Notifications / Waiting on you | One inbox for mentions, replies, approvals, questions; push on phone and browser | `notifications`, `push_devices`, FCM/APNs direct, push relay for self-hosters, Web Push with own VAPID | ADR 0071, 0152, 0172, 0178 |
| Voice messages | Hold the mic; agents read the transcript | Speech adapters (Parakeet self-hosted, DeepInfra, Groq, OpenAI, AssemblyAI…), `voice.transcribe` job | ADR 0100 |
| GIFs, stickers, reactions | Agents send GIFs/stickers when asked | GIPHY adapter | ADR 0091 |
| Presence & status | People: auto/away/busy/hidden; agents: presence from their work, `set_status` line | ADR 0084 | ADR 0084 |
| Agent computers | Agents run commands/code on a connected computer; watch the desktop live, take control, terminal | Runner over WebSocket; `run_code`, `write_file`, `read_file`, `upload_file`, `import_file`, `start_task`; VNC relayed as raw RFB; terminal via `Bun.Terminal` | ADR 0025, 0058, 0065, 0077 |
| Agent browser (beta) | Agents browse, fill forms; watch live; take over for login/captcha and hand back with a note | `agent-browser` (pinned, hash-checked) on Google Chrome stable; per-agent profile; `browser_do` plans with targets in words and a judge that checks; JPEG live stream | ADR 0040, 0054, 0133 |
| Browser profiles | Named shared logins agents are granted (e.g. one WhatsApp Web for five agents) | `browser_profiles`, one driver at a time | ADR 0110 |
| Desktop computer use | Agent uses apps on its Linux desktop | Accessibility tree as text + screenshot close-ups, Jev vision judge | ADR 0062 |
| Phone simulator / real phones | Agent drives Android emulator / iPhone (WDA) for app testing | `phone_*` tools, scrcpy, adb, Maestro flows | ADR 0042 |
| Keep running | Agent keeps a process/browser alive up to 7 days, computer won't idle | `keep_running` → `systemd-inhibit` in the agent's slice | ADR 0176 |
| Vault | Logins, keys, contacts, personal or team; 1Password/Bitwarden sources; password typed by the runner, never seen by the model | `vault_items`; one-use key in runner memory (protocol 45); redaction | ADR 0144 |
| Secrets to computer | `request_secret` card; the browser encrypts to a per-request key made on the computer; server relays ciphertext only | `secret_requests`; env/file targets; redaction on the computer | ADR 0027 |
| Claude Code / Codex / others as brains | Agent thinks with a person's plan instead of an API key | Runner spawns the CLI per turn with `--resume`, stream-json parsed to `BrainEvent`s, MCP bridge to the gateway | ADR 0032, 0041, 0114, 0132, 0142, 0143 |
| Coding plans | GLM Coding Plan, Kimi Code, MiniMax, OpenCode Go keys through Claude Code | `packages/providers/src/plans.ts` | ADR 0059 §8 |
| Custom model endpoint | Any OpenAI- or Anthropic-compatible endpoint ("Outro provedor") | `custom` credential, tested with a tool call on save | ADR 0146 |
| agentes.chat as an MCP server | Use agents from Claude, ChatGPT, Cursor, VS Code etc. | `PUBLIC_URL/mcp`, own OAuth 2.1 AS (CIMD + DCR + PKCE); tools `send_to_agent`, `wait_for_reply`, `list_agents`, `list_channels`, `read_conversation`, `list_recent_tasks`, plus manage tools for imports | ADR 0119 |
| Outside chat surfaces (WhatsApp etc.) | Talk to Lodi from a chat app after linking | `/api/surface/v1` on the operator listener for a separate bridge service; link by code + in-app confirm; deliveries pulled | ADR 0187; changelog 2026.11.18.11 mentions WhatsApp delivery follow-up |
| Imports / export | Bring agents, skills, channels, routines, memory, files from Grok Bot, ChatGPT dots, OpenBot.run, OpenMausBot, Hermes, OpenClaw, Paperclip, another agentes.chat; export a workspace | `MigrationBundle`; parsers in the browser; plan then apply; `agentes-runner migrate` reads OpenBot's SQLite etc. | ADR 0055 |
| Roles & permissions | Additive roles, 23 keys in 5 areas, channel overrides per role/person | `org_roles`, `can(membership, key)` | ADR 0085, 0134, 0151 |
| Audit log | People's changes and every meaningful agent tool call | `audit_log`, `agent-audit.ts` | `CLAUDE.md` |
| Team actions | Every outside action searchable; agent warned before repeating one on the same target | `team_actions`, `team_action_checks` | ADR 0064 |
| Confidential results | A listed MCP server marks a result confidential, sealing the thread | `confidential_threads`, `meta.sealedBody` | ADR 0190 |
| MCP caller identity | Operator's own MCP servers get a signed JWT naming workspace/agent/run/person | `MCP_CALLER_IDENTITY` | ADR 0181 |
| Operator API | Separate listener for the hosting operator: reads, provisioning, standby, billing holds, review | `/api/operator/v1` on `OPERATOR_API_PORT` | ADR 0056 |
| Search | Hybrid Postgres search with filters `from: in: has: before:` | `search_documents`, optional embeddings | ADR 0017, 0123 |
| 14 languages | Whole app, emails, docs | i18n package, glossary | ADR 0122 |
| Apps | Web, desktop (macOS/Win/Linux, Tauri), phone (Expo iOS/Android) | see §5 | README |
| Self-host | One command, Docker compose, Tailscale HTTPS; or native single binary with PGlite | `install.sh`, `@agenteschat/setup`, `agentes-server` | ADR 0060, SELF_HOSTING.md |

---

## 4. System architecture

### 4.1 Runtime and topology
- **One Bun process** serves the web SPA, HTTP API (Hono + `@hono/zod-openapi`), WebSockets (Bun native) and in-process workers (Dockerfile; `apps/server/src/start.ts` `startServer`: config → DB → migrations → adapters + module overrides → HTTP + WS → workers).
- **PostgreSQL is the only required dependency** (pgvector image). No Redis/BullMQ: the job queue is a Postgres **outbox** (`outbox` table, `SELECT … FOR UPDATE SKIP LOCKED`, leases, backoff 5 s→3 h, dedupe keys, priority lanes: person waiting first) (ADR 0005, 0046, 0163). Realtime fan-out via Postgres LISTEN/NOTIFY behind a `PubSub` adapter (ADR 0010).
- **Tests run on in-memory PGlite** with fakes; no Docker or network in unit tests (ADR 0003).
- **Native mode**: `agentes-server` = Bun + PGlite with fsync on, one program, for a person or small team; also the desktop app's "Run agentes.chat on this computer" (ADR 0060 §2, 0066 §7).
- **Freeze watch**: a worker thread logs when the main loop stalls 5 s, naming jobs/tool calls in flight (`apps/server/src/freeze-watch.ts`).
- **Extension point**: `AgentesModule` (`packages/modules`) with `registerRoutes`, `onStart`, `onStop`, `adapters()` overriding SearchIndexResolver, PubSub, Presence, RateLimiter, KeyWrapper (KMS), PlatformCredentials, modelProviders (ADR 0002 §3).
- **Cloud = same image** configured by env vars; private Cloud pieces (billing, provisioner, surface bridge) are separate services that pull from the operator API, never pushed to (ADR 0056 §7, 0079, 0187).

### 4.2 Agent run lifecycle
- `messages.ts` (post + mentions + outbox) → `dispatch.ts` (create runs) → `executor.ts` (7.5k lines: context, gateway, deltas, persist) with states `queued → running → (waiting_approval|waiting_input → running)* → done|failed|canceled` as compare-and-set transitions (ADR 0008; `packages/db/src/schema/runtime.ts`).
- Runs are **resumable tool loops** with an append-only `agent_runs.transcript`, so a resume after an approval sends the same history + results (keeps thinking blocks valid and prompt caches warm) (ADR 0015).
- Steps recorded per run (`agent_run_steps`: context, model_call, tool_call, tool_result, approval, message, error) give the trail shown under each message.
- `run-inbox.ts`: nothing sent to an agent mid-run is lost; `stuck.ts` stops runs that go in circles; `continuation.ts` decides how a turn ends (ADR 0015 addendum, 0097).
- **Stable prompt prefix** per conversation (frozen sections, `# Update` blocks for changes), compaction with summaries (ADR 0050, 0051).
- **Tools on demand**: each run lists core tools plus groups the judge expects; the rest behind `find_tools`/`use_tool` (ADR 0043).

### 4.3 Brains and model providers
- `Brain` interface: turn in, neutral `BrainEvent` stream out (`text_delta`, `tool_call`, `usage {source: api|plan}`, `limit`, `done`, `error`); brains never touch the DB, tools or approvals (ADR 0032 §1).
- **Model brain**: the **Model Gateway** (`packages/engine/src/gateway.ts`) is the only path to a model; budget checked before the call, tokens and cost recorded with `org_id`/`agent_id` (`CLAUDE.md`). Providers in `packages/providers/src/catalog.ts`: Anthropic, OpenAI (Responses API), Gemini (Interactions API), DeepSeek, Kimi, Z.ai, MiniMax (Anthropic-compatible), OpenRouter, Command Code, custom endpoints. Prices change on a date and each call is priced at its time (ADR 0018, 0087, 0146, 0174).
- **Computer brains**: Claude Code, Codex (exec and app-server), Cursor (`agent -p`), OpenCode (`opencode acp`, Agent Client Protocol), Grok Build, Antigravity CLI, each a per-turn CLI process on the computer, sessions resumed by id, our MCP server injected as the only extra tool source, effort/context/model per agent (`apps/server/runner/brain*.ts`; ADR 0032, 0114, 0132, 0142, 0143, 0086). Claude Code turns get `--settings` with hooks, a permission-prompt tool for asks, deny globs for secrets (`packages/adapters/src/command-policy.ts`, 2.8k lines). A silent CLI is stopped and resumed (`brain-silence.ts`).
- **Accounts**: one folder per account (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GROK_HOME`, Antigravity's `HOME`), signed in by the CLI itself from the web (Claude link + code encrypted to the computer; Codex device code), owned by a person; one sign-in can be lent to all of an owner's agents; in system mode root holds it and grants it to each agent user (ADR 0041, 0059, 0150; `apps/server/runner/accounts.ts`). Plan usage is read from the CLI (rate-limit events, `/usage` probes) (`plan-usage.ts`).
- **Jev**: a fast external classifier model by TypeSafe used for routing, judging, memory consolidation, vision "Box" targeting; optional on self-host with the owner's key (or OpenRouter), everything falls back to small models/rules (ADR 0028, 0073, 0074; SELF_HOSTING.md `TYPESAFE_API_KEY`).

### 4.4 Tools, MCP, permissions
- `ToolDefinition` from a `ToolSource` (built-ins, MCP connectors, native connectors, sandbox, browser, Vault, etc.). The **Tool Gateway** (`packages/engine/src/tools/gateway.ts`) is the only path to execution: look up by name → check scope in `agent_scopes` (DB read every call) → validate JSON Schema → run policy table by authority (`tools/run-policy.ts`) → sensitive = approval → record steps + audit + team action (ADR 0015, 0053).
- ~134 built-in tool names (e.g. `run_code`, `upload_file`, `browser_do`, `desktop_do`, `phone_*`, `remember`, `search_context`, `create_issue`, `create_routine`, `schedule_action`, `request_approval`, `ask_agent`, `tell_agent`, `hand_off`, `keep_running`, `edit_spreadsheet`, `generate_image`, `open_pull_request`, `update_plan`) (grep of `packages/engine/src`).
- **Authority** (`authority.ts`): `person` (acts within that person's permissions and the agent's scopes), `routine` (acts for nobody; bounded by the routine's access and `own_actions`), `unattended` (no person at the root). Outside content (mail, webhooks, web) carries a `readOutside` mark through agents and into low-trust memory (ADR 0053, 0117).
- **Computer brains use the same table**: the runner's MCP bridge lists only tools the gateway allows; CLI-native tools are constrained by a per-turn command profile (`work`/`plan`), deny rules and the approvals level (ADR 0032 §2, 0173).
- Connectors: two layers (org connection → agent grants). New connections are every agent's by default; admins narrow (ADR 0016 addendum 2026-10-30).

### 4.5 Computer / sandbox model (the "agent computer")
- **Runner** (`apps/server/runner`, 64k lines): one JS bundle for Node 22+/Bun on Linux/macOS/Windows, served by the server at `/runner/agentes-runner.mjs`, installed by `GET /api/machines/install.sh`. Pairs with a code (`XXXX-XXXX`, 10 min) for a machine token; **outbound WebSocket** (`/api/machines/connect`), so no inbound port, IP or tunnel. Ping every 25 s (ADR 0025 §5).
- **Protocol**: `packages/adapters/src/machine-protocol.ts` (4.8k lines), currently version **73**, with ~75 feature gates. Ops include `exec`, `read`, `write`, `list`, `push`, `pull`, `secret`, `task.*`, `mcp.call/list/result`, `brain.turn/steer/route`, `browser.*`, `desktop.*`, `stream.*`, `phone.*`, `git.sync`, `keep.*`, `vault.key/password`, `cli.install`, `report.collect`, `room.freed` (grep of the file).
- **Files**: `pushFile`/`pullFile` move bytes straight between the computer and storage with single-use URLs; the model never sees the bytes (ADR 0025 §2/2a).
- **Isolation levels** (SELF_HOSTING.md "Agent isolation"): `--system` (Linux root + systemd with `BPF_FRAMEWORK`): each agent its own user `ag-<id8>`, home 0700, launched via `systemd-run --scope --slice=agents-<uid>.slice … setpriv`, cgroup memory/CPU shares (`MemoryMax=75%` of RAM for all agents together, 50% on the server's own computer), `OOMPolicy=continue`; a root **supervisor** keeps the server socket and routes ops to per-agent **workers** that never hold the machine token (ADR 0057). "Simple mode": one `agentes` user for all agents. Plain mode (Mac/Windows/laptop): agents run as the user who runs the runner. Level-2 OS sandbox (Seatbelt/bubblewrap) per agent was proposed (ADR 0030).
- **Network**: loopback guard so each agent reaches only its own local ports (cgroup-marked connections, eBPF) (ADR 0096); egress checks for addresses agents open (ADR 0117); optional root `networkLauncher`/`networkMark` hooks (ADR 0030 addendum, 0101).
- **Containers**: rootless Podman per agent (`docker` = Podman), subuid range `uid<<16` (ADR 0169).
- **Desktop**: Xtigervnc + XFCE per agent on demand, raw RFB relayed over a data socket, noVNC in web/phone, view-only by password, control as a handoff; restores open apps after restart (ADR 0058, 0065, 0098, 0168).
- **Resource admission**: brain turns admitted by memory (`MemAvailable` leaves 1.5 GiB, else `no_slot`), per-sign-in concurrency (Claude up to 6 turns), "In line · starts soon" queue (ADR 0025, changelog 2026.11.18.20).
- **Sleep/standby**: the workspace computer sleeps when unused; whatever wants it writes `wanted_at`, runs wait up to a minute (ADR 0161).
- **Shared folder**: `/srv/agentes/shared` with setgid group + ACLs, `bin/` on every agent's PATH (ADR 0115).
- **Git**: per-agent `GIT_CONFIG_GLOBAL`, agent authorship, credential helper, pushes via the server's git proxy (ADR 0024, 0030).
- Daytona cloud sandboxes were built and then **removed** (ADR 0021 → 0077): one computer model only.

### 4.6 Memory and knowledge
- `context_items` with `trust` (high/low), evidence kinds (human, agent, self, outcome), verified flag, relevance decay, related ids. Loop: reflect (job) → consolidate (inline + daily, Jev semantic same/refines/updates/contradicts) → outcomes as ground truth (PR merged, approval refused, reverts) → promote to instructions/org context only through a human-accepted proposal (ADR 0017, 0028).
- Agents save on their own (`remember` with basis), `context.learn` job at turn end, team memory read by every agent, a scorecard with holdout (ADR 0120, 0167).
- Retrieval: hybrid Postgres FTS + optional vectors (`EMBEDDINGS_*`, self-hostable TEI profile with Qwen3-Embedding-0.6B) (ADR 0167 addendum; `docker-compose.yml` `embeddings`).
- Wiki, Drive, live files, person profiles, `read_conversation` for message links (ADR 0023, 0052, 0138).

### 4.7 Scheduling and triggers
- Routines: schedule (`cron` or a structured `rule`, timezone, phrase reader), events (list above), manual, webhook; `routine_runs` ledger; interval routines spread over minutes (changelog 2026.11.20.5); auto-pause after failures; outputs to channel, DM or Drive; widgets have their own routines (ADR 0053, 0106, 0130, 0135).
- Reminders and scheduled actions (`schedule_reminder`, `schedule_action` with approval ahead) (ADR 0015 addendum).
- Lodi lanes: daily card at 9h local, weekly admin report, approvals digest (ADR 0177).

### 4.8 Multi-agent
- Router picks up to 4 agents; agents may pass silently (ADR 0013, 0049).
- `ask_agent`/`tell_agent`, hand-offs that come back, roll-up (ADR 0034, 0095).
- Agent groups, org chart, setores (departments), a supervisor agent, Lodi as coordinator (ADR 0083, 0148, 0137; changelog "an agent can report to a supervisor agent").
- Open request: a lead agent and its crew sharing one computer environment (issue #874).

### 4.9 Permissions, approvals, governance
- Workspace roles (additive), 23 permission keys, channel overrides, audiences for agents/tasks/wiki (ADR 0085, 0134, 0151).
- Agent rights: owner, "who can edit", audience (`audience.ts`).
- Approvals: hash-bound, exactly once, ordered, delegated, levels, "always" rules (ADR 0015, 0139, 0173, 0182).
- Plans answer only their owner unless widened (ADR 0059).

### 4.10 Observability
- Per-run steps, run timings/profiles (`run-timings.ts`), task manager, audit log, team actions, usage views per agent/account/person, a daily stall-and-noise report email (`autonomy-report.ts`), operator analytics (connector usage, work analytics), Sentry-compatible error reports, anonymous usage telemetry (opt-out), "Send a report" bundles (ADR 0056, 0103, 0104, 0127, 0128; SELF_HOSTING.md).
- Evals in-repo: `bun run eval:memory|behavior|resilience|router|browser|desktop|media…` (`package.json`).

### 4.11 Persistence and deployment
- 179 Postgres tables; every tenant table has `org_id` (test-enforced); `messages` partitioned by month; migrations immutable (`CLAUDE.md`, ADR 0009).
- Compose: `postgres` (pgvector pg18), `agentes` (read-only rootfs, `cap_drop: ALL`, `no-new-privileges`, mem 8 GB), optional profiles `backup` (S3, age-encrypted dumps), `speech` (Parakeet), `embeddings` (HF TEI), `tailscale` (serve/funnel), `https` (Caddy) (`docker-compose.yml`).
- Installer: `curl -fsSL https://agentes.chat/install | sh` → pinned Bun → `@agenteschat/setup` wizard; Tailscale HTTPS by default; makes the server the agents' computer by default (`HOST_COMPUTER_TOKEN`) (ADR 0060; SELF_HOSTING.md). Coolify path in `deploy/coolify`.
- Files: disk volume, Postgres, or S3/R2; per-org bucket after a test (ADR 0019).

### 4.12 Security model
- Provider keys, connector tokens, DB creds: envelope-encrypted per org with a key derived from `AGENTES_SECRET` (KMS via adapter on Cloud); never returned to clients; every model/MCP call through the backend (ADR 0007; `CLAUDE.md`).
- Machine tokens stored as SHA-256; git and GitHub tokens never on the machine (proxy) or short-lived and repo-scoped (ADR 0024, 0116).
- Secrets and Vault logins delivered encrypted end-to-end to the runner and typed by it; redaction of command output (ADR 0027, 0144).
- Injection hardening: one cleaner for outside text (NFKC, invisible chars, envelope markers), `htmlToText` dropping hidden elements, `readOutside` marks, egress guard, credential masking (ADR 0117).
- SSRF guards on every outbound fetch (MCP URLs, CIMD, web pictures) (ADR 0019, 0119).
- Session screening on Cloud computers (OCR of a person's session text, in memory only) (ADR 0076).
- Operator API on a separate never-published port with its own tokens; content never returned except through a separate "reveal" token (ADR 0056).

### 4.13 Pricing limits (Cloud)
- Billing knows no plan in the server: holds (`payment`, `unpaid`, `ended`) and four limits (members, stored bytes, bytes/day, platform AI/month) set by the operator (ADR 0079). Plans on the site: people caps 1/10/25 (+extras), storage 50/200/500 GB, computer size grows by tier (https://agentes.chat/pricing.md).

---

## 5. UX surfaces
- **Web** (`apps/web`, React + Vite + TanStack, 210k lines): pages for channels, DMs, threads, agents, approvals, task manager, tasks board, routines, dashboards, Drive, wiki, memory, calendar, team/org chart, settings, onboarding with Lodi (`apps/web/src/pages`).
- **Desktop** (`apps/desktop`, Tauri 2 + Rust 12k lines): loads the site itself in a window; tray/menu bar; can connect to Cloud or one's own server(s), or **run agentes.chat on this computer** (native mode); can pair this computer as an agents' computer (runner as child process) after the system dialog; signed updates from their server; macOS, Windows, Linux x64/ARM64 (ADR 0036, 0066, 0165).
- **Phone** (`apps/mobile`, Expo/React Native, 184k lines): iPhone and Android, chat, approvals, agent's screen through noVNC in a WebView with a trackpad, push direct to FCM/APNs, multiple servers/accounts, QR sign-in from "Open on your phone" (ADR 0035, 0098, 0154, 0178).
- **Chat apps**: an outside surface bridge for talking to Lodi from a chat app (operator-run separate service; WhatsApp appears in the Cloud changelog) (ADR 0187). Telegram as a connector agents use, not as a channel to the agent (ADR 0170).
- **Other AI apps**: agentes.chat as a remote MCP server for Claude, ChatGPT, Cursor, VS Code, Gemini CLI, Grok (ADR 0119).
- **Voice**: voice notes transcribed (no live voice calls) (ADR 0100).
- **Email**: transactional emails in 14 languages; agents use mailboxes as tools (ADR 0069, 0038).

---

## 6. What they do notably better or differently (with evidence)

1. **Server-side authority across every brain, including subscription CLIs.** Claude Code/Codex/etc. see only a bridge MCP server whose tool list and calls go through the same Tool Gateway, run policy and approvals as API-model runs (ADR 0032 §2, 0053). Most products either trust the CLI or only support API models.
2. **People's AI subscriptions as the compute, with ownership rules.** Accounts per person, lent to their agents, usage read from the CLI, plan windows respected (pause at 99%), "Atende outras pessoas" switch, the ToS reasoning written down (ADR 0059, 0118, 0074 "Plans are personal").
3. **OS-level per-agent isolation on one Linux box**: own uid, slice, /tmp, ports (eBPF loopback guard), rootless Podman, desktop, Chrome profile; root supervisor + per-agent workers that never hold the token (ADR 0057, 0096, 0169, 0185). Per-person isolation of the built-in assistant via "seats" (ADR 0175).
4. **Approvals as a real mechanism**: action hash, exactly-once execution, ordering across runs, delegated approvers, per-agent levels, "approve always" only for tools that name their action (ADR 0015, 0173, 0182, 0139).
5. **Postgres-only core** (outbox, NOTIFY, partitions, PGlite tests and native mode) keeps self-host to one container + Postgres or a single binary (ADR 0005, 0060).
6. **Memory with provenance and trust**: evidence types, outcomes as ground truth, human gate before anything becomes instructions, low trust for outside content, evals (ADR 0017, 0028, 0167).
7. **Router as an auditable chain**, every decision stored with candidates and who decided; works with no API key by using the author's own plan (ADR 0074).
8. **Watch and take over**: live browser JPEG stream, VNC desktop on web and phone, handoff cards for logins/captchas with notes (ADR 0040, 0058, 0098).
9. **Imports from competitors** including OpenBot.run's SQLite, OpenClaw/Hermes folders and Paperclip, via `agentes-runner migrate` (ADR 0055).
10. **Operator API + external provisioning** makes Cloud a thin layer over the same OSS server (ADR 0056 §10).
11. **Documentation discipline**: 201 ADRs, each with code paths, every amendment dated; a docs site with `.md` mirrors and `llms.txt` for agents (`docs/adr`, https://agentes.chat/llms.txt).
12. **Product breadth for non-developers**: live spreadsheets/slides/docs, dashboards, calendar, tasks, wiki, voice, 14 languages, Lodi onboarding (ADR 0196, 0135, 0122, 0162).

**Weak spots** seen in issues: CLI-brain edge cases (Cursor tool breach #767, Antigravity quota loops #690/#691, Codex model refusals #739), self-host topology friction (localhost GitHub App #418, single trusted origin #417, RPM distros #689, http MCP refused #766/#872), runner reliability (OOM leaves runner offline #679, stuck turns can't be cancelled #735), and a need for agent teams sharing one environment (#874). Velocity is very high, so breakage is frequent (multiple releases per day).

---

## 7. Code map (paths → what they do)

| Path | What it does |
|---|---|
| `README.md`, `SELF_HOSTING.md` (162 KB), `CLAUDE.md` (62 KB), `COMMERCIAL.md`, `CLA.md`, `NOTICE` | Product pitch + feature table; full self-host guide; contributor/agent map of the code and non-negotiables; licensing |
| `docs/adr/` (201 ADRs) + `docs/adr/README.md` | Every architecture decision with code pointers |
| `Dockerfile`, `docker-compose.yml`, `install.sh`, `deploy/` | Image (one Bun process, runner bundle built in), compose with optional profiles, one-command installer, Coolify/backup/speech deploy bits |
| `apps/server/src/start.ts`, `main.ts`, `app.ts` | Boot sequence, self-host entry (and CLI subcommands like `migrate`, `password-link`, `telemetry`), Hono app |
| `apps/server/src/routes/app/*` | Private web/app API (`/api/app/*`): agents, approvals, routines, connectors, machines accounts, vault, wiki, live files, etc. |
| `apps/server/src/routes/{machines,git,webhooks,oauth,push-relay}.ts` | Runner pairing/connect, git proxy, webhooks (`/api/hooks/{token}`, Composio), OAuth AS + `/mcp`, push relay |
| `apps/server/src/realtime/{ws,machines}.ts` | Person WebSocket; runner WebSocket |
| `apps/server/src/operator/*`, `src/surface/app.ts` | Operator API (`/api/operator/v1`), surface bridge (`/api/surface/v1`) |
| `apps/server/src/workers.ts`, `engine.ts` | In-process workers; engine wiring (tool sources, brains, server timing marks) |
| `apps/server/runner/` | The agents' computer runner: `main.ts` (CLI: pair, start, service, host, worker, mcp, migrate, folders, tools, computer, doctor, standby…), `connection.ts`, `supervisor.ts`/`worker.ts` (system mode), `agent-users.ts`, `brain*.ts` (one per CLI), `brain-route.ts`, `brain-silence.ts`, `accounts.ts`, `plan-usage.ts`, `mcp.ts` (bridge MCP server), `browser*.ts`, `desktop*.ts`, `terminal.ts`, `phones.ts`/`adb.ts`/`wda.ts`, `exec.ts`, `secrets.ts`, `vault.ts`, `git*.ts`, `loopback-guard.ts`, `egress.ts`, `shared-folder.ts`, `holds.ts`/`standby.ts`, `migrate/` |
| `packages/engine/src/executor.ts`, `dispatch.ts`, `runs.ts`, `messages.ts`, `context.ts`, `continuation.ts`, `steering.ts`, `run-inbox.ts`, `cancel.ts` | Agent run engine: dispatch, turn loop, state machine, context building, turn endings, steering |
| `packages/engine/src/brains/{types,select,model,claude-code,codex,prompt}.ts` | Brain interface, selection, per-brain glue |
| `packages/engine/src/gateway.ts` | Model Gateway: budget, usage, cost |
| `packages/engine/src/tools/{gateway,run-policy,scopes,permission,directory}.ts`, `approvals.ts`, `approval-*.ts`, `authority.ts` | Tool Gateway, policy by authority, scopes, approvals, who-is-behind-a-run |
| `packages/engine/src/router.ts`, `router-jev.ts`, `router-plan.ts`, `judge.ts` | Router chain and shared judge |
| `packages/engine/src/consultations.ts`, `hand-off.ts`, `stretch.ts`, `brakes.ts`, `background.ts`, `agent-work.ts` | Multi-agent talk, stretches, autonomy brakes, background work |
| `packages/engine/src/routines.ts`, `routine-*.ts`, `cron.ts`, `rrule.ts` | Routines, firing, ledger, events, webhooks |
| `packages/engine/src/context/*`, `search.ts` | Memory loop (reflect, consolidate, learn, outcomes, proposals, scorecard), search |
| `packages/engine/src/connectors/*` | MCP client, catalog/presets, classify, OAuth (MCP-native), Composio, personal connections, request headers |
| `packages/engine/src/machines.ts`, `machine-hub.ts`, `machine-provider.ts`, `machine-accounts.ts`, `machine-provisioning.ts`, `computer-queue.ts` | Computers: pairing, socket hub (cross-replica `machine_rpc`), SandboxProvider for connected machines, AI accounts, Cloud provisioning, queue |
| `packages/engine/src/browser/*`, `screen/*`, `desktop/*` | Browser tools/surface/handoff, provider-neutral screen layer (observation, targets, checks, plans) |
| `packages/engine/src/right-hand*.ts` | Lodi (right hand), seats, lanes, approvals, welcome |
| `packages/engine/src/vault/*`, `secrets` in `sandbox-tools.ts`, `crypto.ts`, `credentials.ts` | Vault, secret delivery, envelope encryption |
| `packages/engine/src/git/*` | GitHub App, git proxy policy, signatures |
| `packages/engine/src/missions*.ts`, `mission-*.ts` | Missions (plans, decisions, reviews, worktrees, MCP tools) |
| `packages/engine/src/live/*`, `dashboards/*`, `blocks.ts`, `issues*.ts`, `wiki*` | Live files, dashboards, blocks/polls, tasks, wiki |
| `packages/engine/src/apps/*` | agentes.chat as MCP server + OAuth AS for AI apps |
| `packages/engine/src/surfaces/*` | Outside chat surfaces (links, inbound, deliveries) |
| `packages/engine/src/imports.ts`, `exports.ts` | Migration bundle plan/apply, workspace export |
| `packages/engine/eval/*` | Evals (memory, behavior, router, browser, desktop, media…) |
| `packages/adapters/src/machine-protocol.ts` | Server<->runner wire protocol (v73) |
| `packages/adapters/src/command-policy.ts` | Command deny/ask rules, secret masks, profiles |
| `packages/adapters/src/{sandbox,search,pubsub,push*,email,mail,imap,composio,speech,vault/*}.ts` | Swappable adapters |
| `packages/providers/src/{catalog,anthropic,gemini,commandcode,compat,embeddings,plans}.ts` | Model catalog/prices, adapters, embeddings, coding plans |
| `packages/db/src/schema/*` (179 tables), `packages/db/migrations` (305) | Drizzle schema + migrations |
| `packages/config/src/schema.ts` | The one env var schema (`.env.example` generated from it) |
| `packages/modules/src/index.ts` | `AgentesModule` extension point |
| `packages/client`, `packages/ui`, `packages/tokens`, `packages/i18n`, `packages/live` | Shared client logic (web+phone), design system, tokens, 14 locales, Yjs live-file model |
| `packages/setup` (MIT) | Self-host setup wizard (`@agenteschat/setup`) |
| `apps/web`, `apps/desktop` (Tauri), `apps/mobile` (Expo) | The three apps |
| `scripts/check-boundaries.ts`, `check-migrations.ts`, `check-licenses.ts`, `isolation-vm.sh`, `smoke.ts` | Repo guards, isolation VM test, real-environment smoke |

---

## 8. Patterns worth noting for a "crew server" product [INFERENCE: mapping is mine]
Things agentes.chat had to build to make "named agents on a user's own Linux server, driven from desktop/mobile/chat/webhooks/GitHub" work, which is the same problem space:
- Agent = data row + brain + computer; the brain is replaceable (API vs subscription CLI), authority stays on the server (ADR 0032).
- One tool gateway + one run policy keyed by "who is behind the run" (person / routine / unattended), applied to CLI brains through an MCP bridge (ADR 0053).
- Hash-bound, exactly-once approvals with cards on every surface (ADR 0015).
- Outbound-only runner with pairing codes, versioned protocol, per-agent OS users, cgroup slices, loopback/egress guards, rootless containers (ADR 0025, 0057, 0096, 0169).
- Subscription accounts owned by people, lent to agents, usage-aware pausing (ADR 0059, 0118).
- Secrets/Vault delivered end-to-end to the runner, never seen by the model (ADR 0027, 0144).
- Git through a server proxy with branch policy and agent identity (ADR 0024, 0156).
- Routines with schedule/event/webhook triggers, a run ledger, no silent skips, auto-pause (ADR 0053, 0106).
- Memory with trust/evidence and human-gated promotion (ADR 0017).
- Watch-and-take-over for browser and desktop, including from the phone (ADR 0040, 0058, 0098).
- Chat-app surfaces as a separate bridge service on a token-scoped API (ADR 0187); inbound MCP server so other AI apps can drive the agents (ADR 0119).
- An operator API for whoever hosts it, never exposing content (ADR 0056).

---

## 9. Unknowns
- **Cloud computer infrastructure** (which hypervisor/provider backs "the workspace's computer"): the provisioner is private; ADR 0168 mentions Incus/QEMU only as an example and ADR 0042 says Cloud phones need KVM hosts. Not verifiable from the public repo.
- **The outside surface bridge service** (e.g. WhatsApp for Lodi) is a separate private service; only its server-side door is public (ADR 0187). Which chat apps the Cloud runs is inferred from a changelog line (2026.11.18.11).
- **Jev (TypeSafe)** is an external closed model; its quality and pricing are not in the repo beyond docs (https://agentes.chat/docs/jev.md, not read in full).
- **Real usage numbers** (workspaces, paying customers): not published. Star/fork counts are from GitHub only.
- **Who the people are**: contributors `heliocosta-dev` and `wescld` dominate; no team page was found.
- **Runtime behavior claims** (performance, isolation strength) come from ADRs and docs; I did not run anything, as instructed.
- **Discussions** have only 7 low-activity ideas; the roadmap is not public beyond ADR statuses ("Proposed") and issues.
- Some ADRs are 60-140 KB; I read decision sections and index summaries, not every addendum line.
