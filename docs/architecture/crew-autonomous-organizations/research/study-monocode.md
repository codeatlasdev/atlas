Raw evidence for docs/architecture/crew-autonomous-organizations/ (2026-11-20, main 72b47fc3).

The `/tmp/...` logs and clones cited below were local to the audit run and were not kept.

# Study: Monocode (hardbeat920/monocode), and what Atlas Crew should take from it

Studied 2026-11-20 from a shallow clone (50 commits, HEAD `09d0443`, 2026-11-12) under `/tmp/monocode`, deleted afterwards. I did not run its code and did not install dependencies. Paths below are relative to the Monocode repo unless they start with `atlas:`, which means the Atlas checkout at `origin/main` `72b47fc3`. Line numbers come from that snapshot. Anything I deduced instead of reading is marked [INFERENCE].

## 1. Scope

- **Monocode itself:** what it is, its architecture, how it talks to agents, how it streams to the UI, its UI/UX, state, persistence, multi-agent support, and its project activity.
- **RFC sections it informs:** §8/§18 agent AX, §9 communication, §10 delegation, §11 durable runs, §12 autonomous work loop, §14 memory, §15 authority/policy, §16 reporting/notifications, §17 UX, §19-20 interop, §21 observability, and §31 cost.
- **Atlas comparison points:** `atlas:crates/atlas-host/src/crew/` (the crew server, task log and events) and `atlas:apps/desktop/src/lib/crew/` (the crew UI).

## 2. What it is, who it is for, activity

- **What it is:** "A GUI for your coding agents" (`README.md:4`). It is a desktop app that drives the agent CLIs a user already has and is signed in to: Claude Code, Codex, Cursor, Grok Build, OpenCode, Antigravity, Pi, omp, fx, Hermes Agent and Devin. It sells no tokens (`README.md:15`). Tabs are sessions and the composer is the input. It also has split panes, worktrees per task, an editor, diff review and Git.
- **Who it is for:** individual developers who already pay for one or more agent subscriptions and want one window for all of them.
- **Products:**
  - The production app is React 19 + Tauri 2 (`package.json`, `src-tauri/`).
  - An experimental native Rust + GPUI app (Zed's UI framework, vendored) shares the same database (`README.md:57-83`, `crates/monocode-gpui/`, `crates/monocode-core/`).
  - An Expo iOS/Android companion (`mobile/`).
  - A headless Node "host" runs agents on a remote machine (`host/`, `docs/remote-access.md`).
- **License:** MIT, "Copyright (c) 2026 Nick" (`LICENSE`). `NOTICE` lists the Apache-2.0 GPUI pieces vendored under `vendor/gpui-pre*`.
- **Activity (GitHub API, 2026-11-20):**
  - Created 2026-08-20 and last pushed 2026-11-12.
  - 4,107 stars, 426 forks, 696 open issues.
  - About 1,558 commits (from the commits pagination `rel="last"`) and 88 contributors. The top contributor is `hardbeat920` with 1,190 commits. `CONTRIBUTING.md` says "I'm the only maintainer".
  - 86 releases. The latest Tauri release is `v0.16.0` (2026-10-19); the latest native build is `native-v0.16.0-preview.14` (2026-11-05).
  - 18 commits since 2026-11-05.
  - `CHANGELOG.md` is 170 KB, in Keep a Changelog format.
- **Size:** about 332k lines of TS/TSX in `src/`, 58k lines of Rust in `src-tauri/src`, 280k in `crates/`, 40k TS in `mobile/` and 11k in `host/`. There are 570 `*.test.ts*` files and about 1,732 Rust `#[test]`.

## 3. Code map (paths → purpose)

| Path | Purpose |
|---|---|
| `src/main.tsx`, `src/app/App.tsx` (15,104 lines) | Composition root. It wires sessions, agent events, the orchestration host (`App.tsx:11000-11060`), panes and shell. |
| `src/app/shell/*` | Window chrome: `ProjectRail`, `Sidebar`, `TitleBar`, `UsageFooter` (provider plan-usage chips), `MonoRailSection`, update cards. |
| `src/integrations/harness/core/types.ts` | **The normalized event contract** `HarnessEvent` (lines 13-154) and the turn inputs. |
| `src/integrations/harness/core/registry.ts` | `HarnessAdapter` lifecycle interface (lines 51-120). It also holds per-session operation and steer queues, and idle-parking of warm CLI children (`HARNESS_IDLE_PARK_MS`, line 122). |
| `src/integrations/harness/core/apply.ts` | Reducer from `HarnessEvent` to transcript `Block[]`. `applyHarnessEvents` (line 28) applies one delivery batch "without copying the transcript for every token". It also settles stale approvals (546) and lost tool calls (580), promotes a final message to a plan (619), and mirrors subagent steps (1100-1112). |
| `src/integrations/harness/core/child.ts` | `ChildBackend` abstraction: process I/O comes from either the desktop (Tauri) or the headless host (lines 8-26). Spawn, write and kill. |
| `src/integrations/harness/core/acp.ts`, `jsonRpc.ts` | Agent Client Protocol JSON-RPC client. Cursor, Grok, Devin, Hermes, fx and Antigravity use it. |
| `src/integrations/harness/providers/<id>/` | One folder per agent: `*Protocol.ts` (wire parsing), `*Adapter.ts` (registry entry), `*Catalog.ts` (model discovery), `*Text.ts` (one-shot prompts: titles, commit messages), plus `*Live.test.ts` (against the real CLI). |
| `providers/claude/claudeProtocol.ts:255-259` | Claude Code runs with `--output-format stream-json` and `--input-format stream-json`. |
| `providers/codex/codex.ts:569-760` | Codex runs as `codex app-server` (JSON-RPC: `initialize`, `thread/start`, `turn/steer`, `thread/compact/start`). |
| `providers/pi/piFamily.ts:218`, `providers/omp/omp.ts:23-28` | Pi and omp run as `--mode rpc` with the user's extensions loaded. They share one core. |
| `providers/opencode/*` | OpenCode HTTP server plus SSE (`harness_http`, `harness_sse_open` in `src-tauri/src/harness.rs`). |
| `src-tauri/src/harness.rs` (4,319 lines) | Spawns and owns child processes. Stdout and SSE are batched to the webview (`OUTPUT_BATCH_BYTES = 1 MiB`, line 67; events `harness-stdout`/`harness-sse`, lines 18-22). Also MCP add/list/remove for Claude. |
| `src/features/sessions/model/session.ts` | Domain model: `Session`, `Block` (lines 316-404), `ToolPreview` (193-206), `AgentRunMeta` (230-238), `RuntimeMode` (406-439), `PlanBlockMeta`, `QueuedMessage`, `TurnMetrics`. |
| `src/features/sessions/model/sessionStateStore.ts` | External store with per-session subscriptions, so token delivery wakes only that session's consumers (line 15). |
| `src/features/sessions/ui/AgentTranscript.tsx` (5,437 lines) | Transcript: turn folding, tool rows, subagent trails, the turn footer (`TurnDuration` 1946, `TurnMetricsBadge` 2060), and Allow/Deny inline (~5090). |
| `src/features/sessions/ui/*` | `Composer`, `ApprovalToasts`, `QuestionForm`, `PlanPreview`, `TaskListPreview`, `ToolDiffPreview`, `SessionReview`, `LiveAgentsPreview`, `MessageQueue`, `ContextMeter`, `BtwSheet`, `SecondOpinionCard`, `HandoffMiniCard`, `UsageLimitNotice`, `TranscriptPool`, `wordFade`, `PromptOutline`, `MonoWorkTicker`, `OrchestratorConstellation`, `PlanStepsBurst`. |
| `src/features/orchestration/model/orchestration.ts` (2,162 lines) | `Orchestrator`: lead→worker runs, the scheduler ("pump"), persistence, write-scope enforcement (`observe`, line 2094), and the control-CLI action handlers (`FIELDS`, lines 200-213). |
| `src/features/orchestration/model/orchestrationState.ts` | `OrchestrationRun`, `OrchestrationTask`, `OrchestrationDispatch` state machines (lines 5-110). |
| `src/features/orchestration/model/orchestrationPlan.ts` | Proposal schema and validation: `<monocode_proposal>` JSON (lines 244-265), 1-4 workers (124), scopes made project-relative. |
| `src/features/orchestration/ui/*` | `OrchestrationPreview` (the editable proposal card) and `OrchestrationSidebarAgents` (worker tree under the lead). |
| `src-tauri/src/control.rs`, `control_cli.rs` | Authenticated loopback TCP control channel and the JSON CLI (`<exe> control …` for leads, `<exe> app …` for `/operator` and Monos). Usage text is at `control_cli.rs:8-79`, action lists at 81-117. |
| `src/features/monos/*`, `src-tauri/src/mono.rs` | "Monos": long-lived personal agents with `SOUL.md`, `MEMORY.md`, `habits.json` and `memory/` in app data, never in a repo (`mono.rs:1-18`). |
| `src/features/monos/model/monoHabits.ts` | Scheduled habits: hidden runs, `NOTHING_TO_REPORT` quiet marker (71), a missed-run grace period (73), a 12 h approval wait that ends in a deny (77). |
| `src/features/monos/model/monoMemory.ts` | Memory entries: dated bullets that are superseded rather than deleted, secrets redacted (`redactSecrets`, 34). |
| `src/features/automations/*`, `src-tauri/src/automations.rs` | Automations: schedule or inbox-event triggers. SQLite tables `automations`, `automation_runs`, `automation_event_claims` (`automations.rs:189-215`). |
| `src/features/inbox/*` | Inbox of GitHub, GitLab, Linear, Jira and Azure DevOps items, PR checks, and a "repair CI" flow (`model/ciRepair.ts`). |
| `src/features/notifications/*`, `src-tauri/src/notifications.rs` | OS notifications (UNUserNotificationCenter, freedesktop, WinRT), per-project mute, approval toasts, resource pressure. |
| `src-tauri/src/session_store.rs` | SQLite store: `sessions` table with `blocks_json`, numbered `schema_migrations`, `in_flight_sessions`, `workspace_snapshot` (lines 13-30, 688-760). |
| `src-tauri/src/checkpoint.rs` + `checkpoint/` | Per-turn file checkpoints, used for Undo All/Keep All and for applying worker changes to the lead checkout. |
| `src/features/connections/model/protocol.ts` | Remote host protocol v1: `HostCommand` with `commandId` (144-221), and `SessionSync` revision deltas or chunked transfers (92-142). |
| `host/*` | Headless Node host: `server.ts` (HTTP on loopback 3774), `engine.ts`, `store.ts`, `service.ts` (systemd, Task Scheduler), `sync-transfer.ts`. |
| `crates/mobile-link/` | QUIC + mutually authenticated TLS 1.3 between phone and desktop (`lib.rs:1-30`: `PORT 38416`, `ALPN monocode-mobile/1`). It does a native transcript projection (`transcript.rs`). |
| `mobile/src/app`, `mobile/src/features/sessions` | Expo routes (projects, session, mono), and `LiveSessions`, `SessionNotifications`, `QuestionCard`, `TurnMetricsSheet`. |
| `crates/monocode-core`, `crates/monocode-gpui` | Native preview: core logic (orchestration, mono, automations, visual) and a GPUI UI (298 files). |
| `src/styles/index.css` (4,360 lines) | Tailwind v4 `@theme` tokens (colors, diff, syntax, motion), 65 `@keyframes`. |
| `docs/*.md`, `docs/native/*.md` | User docs: providers, `/operator` agent access, remote access, native parity, visuals, verification. |

## 4. Architecture

```
React UI (App.tsx) ──subscribe──► SessionStateStore ◄── applyHarnessEvents (core/apply.ts)
      │                                             ▲  HarnessEvent (core/types.ts)
      │ adapter.sendTurn/steer/cancel/respondApproval│
      ▼                                             │
HarnessAdapter registry ──► provider/<id> protocol parser ◄── lines/SSE batches
      │                                                     ▲ (Tauri events harness-stdout / harness-sse)
      ▼ ChildBackend (desktop Tauri | headless host)        │
src-tauri/harness.rs  spawn CLI: claude stream-json | codex app-server | pi/omp --mode rpc | ACP agents | opencode HTTP+SSE
```

- **Adapters, not a protocol.** Each agent CLI keeps its native wire protocol. Monocode writes a per-provider parser that turns it into one internal `HarnessEvent` union (`core/types.ts:13-154`). That union is the real integration contract. Six of eleven providers speak ACP (`core/acp.ts`); the rest use their own protocol.
- **AG-UI and A2A: none.** `grep -i ag-ui|agui` finds nothing. MCP appears in two ways only:
  - Monocode manages users' MCP server configs per provider (`src-tauri/src/mcp.rs`, `harness.rs:485-763`).
  - In the native app it injects its own session MCP tools `visual_docs`/`visual_check`/`visual_show` (`docs/native/visuals.md:13-21`).
- **The app owns orchestration, not the agents.** Multi-agent work runs in the UI process. A lead agent drives workers through a CLI that talks to the app over authenticated loopback (`control.rs:1-2`: "App windows own execution; callers never receive arbitrary Tauri command access"). Workers are ordinary sessions marked with `orchestrationLeadId`.
- **Streaming path:**
  1. Rust reads the child's stdout and batches lines per window (1 MiB cap), because "each `emit` is a JS eval in every window" (`harness.rs:40-105`).
  2. The provider parser emits `HarnessEvent`s.
  3. `applyHarnessEvents` folds a batch into blocks without copying the transcript per token.
  4. `SessionStateStore` notifies only that session's subscribers.
  5. `wordFade.tsx` paces uneven provider bursts into a smooth word fade-in, keyed to the average gap between bursts (lines 7-53).
- **Process economy.** A warm CLI child is kept for 3 minutes after a turn and then "parked" (killed, with resume state kept). There is an LRU limit on idle children (`registry.ts:117-140`). The comment explains why: "Each warm child is a CLI process of a few hundred MB, plus its MCP servers".

## 5. Integration surfaces in detail

### 5.1 The event contract (`core/types.ts`)

| Group | Events |
|---|---|
| Lifecycle | `session.started`, `session.ended{code}`, `session.error`, `session.providerBound`, `turn.started{providerTurnId}`, `turn.ready`, `session.configChanged` |
| Text | `message.delta`/`completed`, `reasoning.delta`/`completed`, `image.generated` |
| Tools | `tool.started`/`tool.updated` carry `callId`, `kind`, `status`, `detail`, `background`, `paths[]` and `preview: ToolPreview`. A `ToolPreview` has a kind (read/write/shell/search) plus path, start line, additions/deletions, diff `lines[]` (add/del/context) or `output` (`session.ts:183-206`). |
| Subagents | `agent.step{callId, stepId, kind, text, toolKind, preview, agentName, agentType}`. It is mirrored onto the parent Agent/Task tool block as `AgentRunMeta.steps`, merged by `stepId`, with a bounded window (`apply.ts:1100-1112`). |
| Human-in-the-loop | `approval.requested{requestId, title, kind, callId, preview}`, `approval.resolved{allow/deny/cancelled}`, `question.asked{questions, autoResolveAt}`, `question.updated`, `question.resolved{answered/skipped/cancelled}` |
| Planning | `tasks.updated{key, merge, authoritative, items[{id,text,status}]}` (todo list) and `plan{text, key, append, streaming}` (a plan document that turns reviewable when `streaming:false`) |
| Background | `background.updated{tasks[]}` (lines 28-32): "the agent has yielded but the turn is not over" |
| Cost/telemetry | `context{used, window}`, `turn.metrics{input/output/cacheRead/cacheWrite tokens, cacheHitPercent}`, `usage.limited{resetsAt}`, `status{text,key}`, `interjection{severity: nit/concern/blocker}` (omp advisor notes) |

### 5.2 Control CLI for agents (`control_cli.rs`)

- **Lead actions** (`ACTIONS`, line 81): `list`, `delegate`, `get`, `wait` (long-poll 0-25 s), `respond` (approve for a worker), `answer`, `steer`, `message`, `retry`, `cancel`, `review`, `finish`.
- **App actions** (`APP_ACTIONS`, line 85, 32 of them):
  - `models.*`
  - `sessions.list/read/send/draft/start/stop/archive/delete`
  - `worktrees.*`, `folders.*`, `notes.*`, `artifacts.*`
  - `soul.*`, `memory.*`, `habits.*`
  - `chat.card`
- **AX rules written into the usage text** (lines 8-79):
  - Output is one JSON line, and the exit code is 0 only when `ok` is true.
  - Unknown fields are rejected, not ignored.
  - "Every call carries a request ID, and the run applies each ID at most once." To retry an uncertain call, reuse `--request-id`.
  - "Exiting this CLI … never cancels a task that was already accepted."
  - When the run is paused, the lead is told not to poll and to ask the user to click Resume.
  - The endpoint and token arrive as env vars, with "never print them".
- **Error text says what to do next** (for example `orchestration.ts:214-225`, `questionAnswers` 239-262).

### 5.3 Remote host protocol (`connections/model/protocol.ts`, `host/`)

- **Versioning:** `HOST_PROTOCOL_VERSION = 1` and a descriptor with `capabilities[]`. A mismatch fails with "incompatible MonoCode Host" (lines 6, 21-28, 236-247).
- **Commands are idempotent.** Every command (`create`, `configure`, `send`, `cancel`, `approve`, `answer`, `draft`, …) carries a `commandId`. The host returns a `CommandReceipt{commandId, sessionId, revision}`. "Retry asks the host for that same command's receipt, so a lost response does not create another conversation" (`docs/remote-access.md`). Pending requests survive a desktop restart.
- **Sync is by revision.** The client reads with `sessions.sync` and gets back `unchanged`, `snapshot`, or `delta{base, blockIds, blocks}` with only the blocks that changed. Large payloads are `chunked`. The client verifies the base and asks for a snapshot if it does not match (`applySessionSync`, 117-142). Clients poll; there is no push [INFERENCE from docs: "Git changes are refreshed every few seconds", "Workspace lists refresh while the phone is active"].
- **Security:** the host binds 127.0.0.1 only and is reached through an SSH forward. Device tokens can be listed and revoked, and the data dir is 0600 or ACL-restricted.

### 5.4 Mobile (`mobile/`, `crates/mobile-link`)

- **Transport:** direct QUIC with mutually authenticated TLS 1.3. The desktop identity is pinned by the pairing QR, codes are single-use for 15 minutes, keys live in the OS keyring or SecureStore, and NAT-PMP/UPnP maps UDP 38416 (`mobile/README.md`).
- **What works:** the phone reads the latest 200 blocks, sends, starts sessions, stops turns and views real Git changes. It shows "Needs your approval" states and notifications (`LiveSessions.tsx:470`, `SessionNotifications.tsx:40-47`). The README says "Agent approvals and questions still need attention on the desktop", and there is no offline command queue.

## 6. UI and UX in detail

### Screens

Described from `docs/screenshot.jpg` (Tauri, classic layout) and the README hero image (native GPUI, new layout):

- **Classic layout** (`docs/screenshot.jpg`):
  - A left "Sessions | Files" sidebar. Each card shows the agent icon and model ("Claude Opus 5", "Cursor Grok 4.6"), age ("22m", "7h 59m"), title, `branch` and a green/red `+478 −2` diff count.
  - Above it, a project header chip shows "6 Files • +949 −10".
  - The center is split into two session panes. Each turn ends with a check-marked "Worked for 2m 39s" fold line. Collapsed history reads "+1 previous tool call".
  - A user correction bubble ("no that was not a good change actually revert…") is followed by an inline file diff card (`src/surfaces/gridArcade.ts +100 −217`, red/green lines with line numbers).
  - Below the turn, a review bar reads "2 Files · Undo All · Keep All · Review".
  - Each pane's composer shows the cwd plus a branch chip, a "+" attach button, a model pill ("Cursor Grok 4.6"), an effort pill ("Extra High"), a "Fast" toggle, a permission pill ("Supervised") and a send arrow.
  - On the right, a code editor pane and a "Changes" panel with a commit message box, a Commit button with a dropdown, and M/U status per file.
- **Native GPUI layout** (README hero):
  - The left rail has a project switcher, then Search ⌘K, Inbox (unread dot), Notes, Automations.
  - A **Monos** section lists named agents with mascot icons: "Big Boss", "Nox (performance)", "Milo (bugs)".
  - Then Pinned and Today session lists with branch and model.
  - The center conversation is a `/visualize` request answered with an interactive star-growth chart rendered natively. Its footer reads "GPT-6.1-Sol worked for 1m 56s" with copy/save/handoff icons.
  - The composer pills read "GPT-6.1-Sol Extra High" and "Auto". Below sit "Current checkout" and `main`, and a bottom bar shows "31% 9h 19m" (plan usage left and reset time).
  - Glass/blurred backgrounds over a user wallpaper.

### Interaction patterns, with code

- **Turn folding and an "activity trail."** Tool calls, thinking and edits fold into a trail under each turn. Status pings join the trail; errors and interruptions never fold, because they are a `notice` (`session.ts:389-393`, `transcriptActivity.ts:120-165`). An edit still waiting for approval stays unfolded: "you cannot judge a diff you cannot see" (`transcriptActivity.ts:137-140`).
- **Tool rows** read as verb + target ("Edit src/app.ts", "Read", "Find"). A click opens the path the label showed, never a different `preview.path` (`transcriptActivity.ts:203-238`). `ToolDiffPreview` is "a quiet file chip that reveals the tool's own edit, independent of git" (`ToolDiffPreview.tsx:18`).
- **Subagent trails.** An Agent/Task tool row expands to "Show <name>'s work", which lists the child's steps inline without opening a second session (`AgentTranscript.tsx:4071`, `session.ts:225-238`).
- **Approvals in three places:**
  - Inline Allow/Deny in the transcript (`AgentTranscript.tsx:~5090`).
  - Toasts in the top right. Each shows the session title, agent icon, an amber "Approval"/"Question" tag and a three-line label, with Allow/Deny buttons, and the toast respects per-project notification mutes (`ApprovalToasts.tsx`).
  - OS notifications. These fire only when the user is looking elsewhere: "A banner only earns its place while the user is looking elsewhere: another app, or another session" (`notifications.ts:91-94`).
- **Questions** render as a form with options and Skip. When the agent set a deadline, a countdown reads "Continues without an answer in Ns", and starting to answer keeps the question open (`QuestionForm.tsx:32-39, 173-180`; `registry.ts` `keepQuestionOpen`).
- **Plan → Build.**
  - The plan card has a title, a three-line summary, Open (in a pane) and Build. Build has a model dropdown, so a plan made with one model can be built with another (`PlanPreview.tsx`).
  - `PlanBlockMeta` keeps `originalText`, `approvedText` and `edited` (`session.ts:100-111`): the exact approved text is saved.
  - Agents without a plan event get their final message promoted to a plan only after the turn ends (`apply.ts:619-625`).
- **Todo list.** `TaskListPreview` shows provider-reported items (pending, in_progress, completed, cancelled) that merge by id.
- **Review after the turn.** `SessionReview` hides while the turn runs ("The card represents the result of a turn", `SessionReview.tsx:62-64`). Afterwards it shows N files with Undo All / Keep All / Review. The data comes from per-turn checkpoints, not from git.
- **Multiple running agents:**
  - `LiveAgentsPreview` is a "Working · N" panel with a pulsing dot. It shows up to 4 agents and expands for more. Each card shows mascot, agent, current activity, elapsed time or "Done", and needs-approval (`LiveAgentsPreview.tsx:19-20`, `liveAgents.ts`).
  - Unseen finished sessions stay listed until seen.
  - Orchestration workers are excluded and live in the lead's panel (`liveAgents.ts:23-29`).
- **Orchestration UX:**
  - `/orchestrator <request>` makes the lead investigate and return a `<monocode_proposal>` JSON. The app renders it as an **editable card**: a title, a summary, and per task a title, instructions, a model + effort picker, files and dependencies.
  - A "Parallel workers" stepper (1-4) has a help popover explaining the trade-off (`OrchestrationPreview.tsx:380-410`).
  - Nothing runs until the user approves ("Awaiting confirmation" → "Approved").
  - Invalid proposals keep the raw response and get a repair turn (`orchestrationPlan.ts:296-306`).
  - During the run, `OrchestrationSidebarAgents` hangs the workers off a rail under the lead's row ("tree" variant). Several workers can be expanded side by side.
- **Background work.** `background.updated` sets `Session.backgroundTasks`, and the footer says "Worked for … · <background label>" while the agent waits on its own background jobs (`AgentTranscript.tsx:1919`).
- **Cost and limits:**
  - `ContextMeter` is a ring gauge that turns amber, then red. It "renders nothing until the runner reports both halves … a ring guessing at a number is worse than no ring" (`ContextMeter.tsx:14-27`).
  - `TurnMetricsBadge` is a per-turn icon whose popover shows "Cache hit X% · Output N tok/s" and the input/output/cached tokens (`AgentTranscript.tsx:2060-2120`).
  - `UsageFooter` chips show each provider's plan usage and reset time.
  - On a usage limit, the session can "resume at reset" or "continue on another provider account", which copies the native conversation (`session.ts:291-297`, CHANGELOG 0.15.0).
  - **No dollar cost is shown anywhere.** It is tokens and plan percentages only.
- **Steering and queue.** Messages typed while a turn runs either steer (`turn/steer` for Codex, `canSteer`) or go to a visible `MessageQueue`. The queue can be edited, paused after Stop, and resumed (`messageQueue.ts:32-55`).
- **Side questions and second opinions:**
  - `/btw` opens a read-only side thread anchored to a turn, in a sheet that "morphs out of the session's composer box" (`BtwSheet.tsx:82-84, 468-471`).
  - "Second opinion" sends the turn's diff to another model for review.
  - "Handoff" continues the session on another agent with a recap (`SecondOpinionMeta`, `HandoffMeta`).
- **Monos (personal agents):**
  - A name, mascot and colour, one conversation, several projects (`mono.ts:29-55`).
  - Habits run in hidden ephemeral sessions and post to the chat only when worth it. Quiet runs post nothing (`monoHabits.ts:17-21, 337-372`).
  - Spawned sessions report back to the Mono's chat when they finish (`MonoSessionCompletion`).
  - The native preview adds a "heartbeat" that checks in on an idle Mono every 30 minutes by default, with backoff to 4 h on quiet results. It proposes at most two follow-ups, and "new work is proposed for the user to accept" (`docs/native/README.md:38-55`).
  - An Overview tab has a contribution calendar and a "Sub-agents" list.
- **Keyboard:** about 50 rebindable commands (`settings.ts:1151-1255`). Examples:
  - ⌘K search, ⌘P go to file, ⌘⇧P command palette.
  - ⌘. switch model, ⌘I toggle Mono, ⌘D split right, ⌘⌥ arrows to focus panes.
  - ⌘1-9 tabs, ⌘J terminal dock, Escape stops a run, ⌘L focuses the prompt (`e67521e`).
  - A global Quick Composer floating window (`quick-composer.html`, `features/quick-composer`).
  - IME composition is respected (`appShortcuts.ts:29-36`).
- **Animations as feedback:**
  - Tool rows enter one after another. A long burst speeds up so it catches up (`AgentTranscript.tsx:187-193`, `STEP_ENTRANCE_MS = 480`).
  - The paced word fade.
  - `MonoWorkTicker` is a one-line status that pushes the previous status up (`MonoWorkTicker.tsx:14-17`).
  - `ComposerRunner` shows a project pixel mascot running along the composer's top edge while a turn is live (`ComposerRunner.tsx:71`).
  - One-shot "turn celebrations": `PlanStepsBurst` and `OrchestratorConstellation`, where a lead fans out to agents inside the sent bubble. They play only for a freshly sent turn and never replay on remount (`turnCelebration.ts:3-6`).
  - There are 54 `prefers-reduced-motion` references.
- **Design system:**
  - Tailwind v4 `@theme` tokens: background/content lightness driven by hue and saturation variables, `--color-stroke` as a 7% content mix, a selection strength ladder, an accent of `hsl(211 92% 62%)`, diff add/del colours, VS Code Dark+/Light+ syntax colours, motion durations and easings (`index.css:30-224`).
  - Optional glass with native macOS blur, and project background images.
- **Performance work that shapes the UX:**
  - `TranscriptPool` keeps recently shown transcripts' DOM mounted and moves them between panes, with a node budget of about 40k (`TranscriptPool.tsx:13-43`).
  - `content-visibility` for turns off screen, and workers for highlighting.
  - A "Resource pressure" monitor (3 high samples to enter, 6 to recover, 60 s cooldown, `resourcePressure.ts:21`) with a process view that can stop agents or individual tools.

## 7. State, persistence, durability

- **Client state:** the `SessionStateStore` external store plus `useSyncExternalStore`. The `Orchestrator` singleton has its own subscribe and snapshot (`orchestration.ts:273-336`).
- **SQLite** through `rusqlite`, in WAL mode with a separate read-only connection:
  - `sessions(blocks_json)`, a whole transcript as JSON per row, with numbered migrations (`session_store.rs:13-30, 688-760`).
  - `in_flight_sessions`: busy chats are recorded, and after a crash or quit they resume with "Continue from where you left off." (`inFlight.ts:7-40`).
  - `workspace_snapshot`, `notes`, `session_reminders`, `automations*` and `mono_transcripts/turns/blocks`.
- **Orchestration runs** are saved as a JSON blob per lead (`control_save`/`control_load`). The format is versioned 1 or 2 and migrated on load (`orchestration.ts:91-105`).
  - Every state change goes through `commit()`, which saves before the run continues.
  - If saving fails, the run pauses, workers are stopped, and each task gets a `recoveryPrompt` so Resume continues in the retained worker checkout (`orchestration.ts:337-420`, `recoveryTurn` 159-160).
  - The dispatch stages `accepted → session_prepared → turn_submitted → settled → integration_started → integrated → cleaned`, plus the `start_unknown`/`stop_unknown` states (`orchestrationState.ts:29-61`), make restarts explicit.
- **Mono files** are written atomically with mode 0600, and saves are hash-checked so the user never overwrites what the agent wrote while they were typing (`mono.rs:143-172, 270-296`).

## 8. Multi-agent and orchestration

- **Model:** one lead session and up to 4 parallel workers. Tasks carry `files` (write scope) and `dependsOn` (a DAG checked for cycles).
- **Workspace policy:** `shared`, `isolated-child` or `isolated-top-level` (`orchestrationState.ts:15-16`). An isolated worker gets its own git worktree and a scratch `TMPDIR`.
- **Scheduler:** tasks whose scopes overlap are not run at the same time (`scopesOverlap`, `orchestration.ts:146-157`).
- **Scope enforcement at runtime.** `observe()` watches every `tool.started`/`tool.updated` write preview. It resolves each path on disk (`control_write_path`) and **blocks only that worker** when the path is outside its scope or scratch dir. The worker's checkout is kept, and the lead receives an actionable message (`orchestration.ts:2094-2160`).
- **Worker prompt envelope.** The worker prompt carries a hidden `<monocode_assignment>` envelope: stay in scope; never spawn agents, commit, push, install, or run broad formatters; report blockers and stop (`orchestration.ts:162-178`). The transcript removes it (`visibleUserPrompt`).
- **Lead decides for workers.** Workers' approvals and questions go to the **lead** as `needsInput` through `list`/`get`/`wait`; workers never prompt the user (`control_cli.rs:31-37`).
- **Integration:** the worker's agent is stopped, its checkpoint is flushed, and the app checks that the worker and lead `HEAD` did not move. It then applies the worker's checkpoint files into the lead checkout, and the lead must `review` each task (`App.tsx:11000-11035`). If a branch moved, the worktree is kept for manual review.
- **`/operator`** is a lighter mode: one thread gets the `app` CLI for that thread only, to start sessions in panes, read other sessions, and write notes. The CLI works only during an active turn (`control.rs:15`, `docs/agent-access.md`).

## 9. Notable engineering choices

1. **Normalize at the edge.** Eleven agents map to one event union. The UI never branches on provider ("App.tsx dispatches through the registry instead of per-agent branches", `registry.ts:48-50`).
2. **Live tests against real CLIs** for each provider (`*Live.test.ts`, `*Real.test.ts`, `antigravitySoak.test.ts`).
3. **Idempotency everywhere:** `commandId` on the host, `--request-id` on the control CLI, `appRequestId` on blocks (`session.ts:340`).
4. **Long-lived fields are separated from in-memory ones.** `Session` comments mark each field "In-memory only", for example `pendingQuestion`: "request ids do not survive restarts" (`session.ts:524-528`).
5. **Supervised really means supervised.** The default mode is explicit, because sending nothing let Claude fall back to the user's `permissions.defaultMode`, "so a session the picker called Supervised could silently run as `auto`" (`claudeProtocol.ts:108-125`). The four modes map onto each provider's native sandbox and approval knobs (`codexProtocol.ts:62-95`).
6. **The app holds memory, outside repos.** A soul kept in a repo "could be rewritten outside the app's controlled CLI or by a pull request" (`mono.rs:1-3`).
7. **Docs carry verification state.** `docs/native/verification.md`, `parity.md` and the preview change audits [INFERENCE: I did not read these in depth].

## 10. Atlas comparison (verified points only)

- **Atlas crew steps are text-only.** `Step{ts, text, group}` is built as `"{tool_name} {first string arg, 80 chars}"`, capped at **50 steps per task** (`atlas:crates/atlas-host/src/crew/store/mod.rs:350-356`, `atlas:crates/atlas-host/src/crew/runner/outcome.rs:71-93`). Monocode keeps typed tool calls with `callId`, `status`, `paths` and a structured `ToolPreview` that includes diff lines, merged by id.
- **Atlas task state is coarse.** `TaskState` is `Queued | Running | Done | Failed | Cancelled | WaitingAction` (`store/mod.rs:340-347`). There are rich typed `TaskAction`s for human input: `ConnectGithub`, `ProvideSecret`, `Input`, `Choose`, `SignInEngine`, `AddTrigger`, `Refused` (`store/mod.rs:300-330`). There is also `ask: AskRequest` and a persisted `usage`.
- **Atlas push events are a handful.** `crewmate_task` (with or without the whole task), `crewmate_updated`, `crew_paused`, `crewmate_approval_unanswered` (`atlas:crates/atlas-host/src/crew/events.rs`). A client gets the whole `Task` again on each change.
- **Atlas already has stronger policy than Monocode in places.** hostd decides every tool call: fixed rails (protected paths, `no_auto` for text from strangers), then mode, then per-task grants, then a person. Every decision is audited, and a timeout means deny (`atlas:crates/atlas-host/src/crew/approval.rs:1-25`). Monocode relies on each provider's own approval flow and on the lead to answer for workers. Its only cross-provider enforcement is the write-scope observer.
- **Atlas's task log is durable and append-only.** `tasks.jsonl` with step deltas, an index file and compaction (`atlas:crates/atlas-host/src/crew/store/tasklog.rs:1-25`). Restart recovery is capped by `restarts` (`store/mod.rs:420-428`).
- **Atlas desktop folds steps by the agent's own title** (`atlas:apps/desktop/src/lib/crew/TaskWork.svelte:1-20`), much like Monocode's trail. The difference is that the rows carry no diff or preview.

## 11. Lessons for Atlas Crew

### 11.1 UI/UX patterns to adopt

1. **The turn is the unit, and its result appears only when it settles.**
   - Show a review card after a task finishes (N files, +/−, Open diff, Keep/Undo, or Approve/Request changes for crew). Hide it while the task runs, as `SessionReview.tsx:62-64` does.
   - Crew tasks run in worktrees, so the card should be fed by the worktree diff at the end of the task. That is where human approval of what matters (RFC §15/§17) gets a concrete surface.
2. **Each tool row should be a typed chip that reveals the agent's own edit,** not only "Edit foo.ts" text (`ToolDiffPreview.tsx`). Errors and interruptions should never fold away (the `notice` rule).
3. **Keep an edit that waits for approval expanded,** with its diff visible next to Allow / Allow for task / Deny. Atlas's `ApprovalPrompt.svelte` should carry the tool preview, because "you cannot judge a diff you cannot see".
4. **Use one global "Working · N" strip across crewmates.** Show at most 4 rows with expand; each row has mascot, task title, current activity, elapsed time, and needs-approval or Done-unseen. Keep finished-unseen rows until opened (`LiveAgentsPreview.tsx`, `liveAgents.ts`). Atlas's board is per crewmate; this strip answers "what is my org doing right now" (RFC §16/§17).
5. **Offer approval toasts with inline actions,** and send an OS banner only when the user is elsewhere. Toasts follow per-project mute rules (`ApprovalToasts.tsx`, `notifications.ts:91-94`). For Crew, apply this to `WaitingAction` tasks and to held tool calls, on desktop and phone.
6. **Questions with a visible deadline.**
   - Atlas denies an unanswered call after `CREWMATE_DECISION_WAIT`. Show that countdown on the card ("Denied automatically in 4m") and pause it once the person starts typing (`QuestionForm.tsx:173-180`, `keepQuestionOpen`).
   - Today `crewmate_approval_unanswered` only reports the deny afterwards.
7. **Plan cards that save the exact approved text.** If the crew RFC has plan-then-execute (§10/§12), save `originalText`, `approvedText` and `edited` like `PlanBlockMeta`, so the audit log (§21) records what was approved, not what the agent later claimed. Offer "Build with <model>".
8. **An editable delegation proposal before anything runs.** The `/orchestrator` card (tasks, instructions, model, files, dependencies, parallelism, with an explanation) is the best UX in the repo for RFC §10 delegation with human approval. Its "Parallel workers" help text is a good model for explaining a policy setting in place.
9. **Cost: show only measured numbers.**
   - Follow the ContextMeter rule: render nothing until real data exists.
   - Show per-turn token metrics behind an icon popover (cache-hit %, tok/s) rather than inline noise.
   - Show plan-usage chips with reset time. Atlas already stores `usage: UsageRecord` per task and has `UsageMeter`/`UsageSparkline`.
   - Monocode shows **no dollar cost**. RFC §31 needs budgets, so Atlas should add money where engines report it and not fake it.
10. **Make "quiet" a first-class outcome for autonomous loops.**
    - Habits and heartbeats post only when there is something worth telling, using a quiet marker. Quiet results back off the interval (30 min up to 4 h), and missed runs older than a grace period are skipped, not caught up in a burst (`monoHabits.ts:17-21, 71-77`, `docs/native/README.md:38-55`).
    - Atlas triggers and schedules (`crew/schedule.rs`, `triggers/`) should adopt `posted | quiet | failed` per run and the no-burst-on-wake rule. This addresses RFC §12 notification fatigue directly.
11. **Proposals, not actions, for self-started work.** The native heartbeat proposes at most two follow-ups, and the user accepts them. This fits Atlas's existing `TaskAction::AddTrigger` confirm pattern, and is the safe default for §12/§13.
12. **Feedback motion with discipline:** staggered rows that catch up, a one-line ticker, celebrations only on fresh sends that never replay on remount, and support for `prefers-reduced-motion`. Atlas's `CrewTyping`/art timeline can borrow the "never replay on remount" rule (`turnCelebration.ts:3-6`).
13. **A keyboard map for the operator:** stop the run with Escape, focus the prompt with ⌘L, switch model with ⌘., use a palette, and keep a global quick composer that hands work to a crewmate from anywhere.

### 11.2 Integration and protocol patterns to adopt

1. **Typed run events from hostd, as deltas.**
   - Replace "the whole `Task` on every change" plus text `Step`s with a small typed event stream per task. The shape is: `tool.started/updated{call_id, kind, status, paths, preview{kind, path, additions, deletions, lines?}}`, `approval.requested/resolved{request_id, preview}`, `question.*`, `tasks.updated` (todo), `plan{streaming}`, `context`, `turn.metrics`, `background.updated`, `agent.step` (subagent mirror).
   - This is the smallest change that gives Desktop and Mobile diffs, todo lists and subagent trails. It can live alongside the current `crewmate_task` event: old clients keep refetching the task (`events.rs` already supports `task: None`).
   - Persist it as new `tasks.jsonl` delta kinds next to `step_of`, which older hostd already ignores until the final full record (`tasklog.rs:24-25`).
2. **Merge by provider identity, and bound the windows.**
   - Tool calls merge by `call_id`, subagent steps by `step_id`, todo items by `id`, with `authoritative`/`merge` flags (`types.ts:129-140`).
   - Keep a fixed-size window of recent subagent steps (`apply.ts:1100-1108`), and load large tool output on demand (`detailOmitted` + `loadToolDetail`).
   - Atlas's 50-step cap and 80-char truncation lose exactly the evidence the guards and reviewers need.
3. **Revisioned sync for clients.** Add a `revision` per task or conversation and a `sync(since)` call that returns `unchanged`, `snapshot`, or `delta{base, ids, items}`, plus chunking for big payloads (`protocol.ts:92-142`). This makes phone reconnects cheap and lets an old client fall back to a snapshot.
4. **Idempotent commands with receipts.** Every hostd command that mutates crew state (`crewmate_task`, answer, approve, cancel) should accept a client `command_id` and return a receipt that the client can re-ask for after a lost response. Today a resend on a flaky phone can create a duplicate task [INFERENCE: I did not check whether hostd already dedupes `crewmate_task`].
5. **Agent CLI AX rules.** For Atlas's MCP tools and `atlas` CLI used by crewmates (RFC §8/§18):
   - one JSON result;
   - non-zero exit unless ok;
   - unknown fields rejected with the list of allowed fields;
   - errors that say the next step;
   - `request_id` for retries;
   - "exiting never cancels accepted work";
   - "when paused, do not poll; tell the user why";
   - a long-poll `wait` (≤25 s) instead of polling loops.
   These are all spelled out in `control_cli.rs:8-79` and are directly reusable as acceptance criteria.
6. **Runtime write-scope guard for delegated work.** Watch the write previews of each tool call, resolve paths on disk, and stop only the offending worker while keeping its checkout (`orchestration.ts:2094-2160`). Atlas's guard layer (`runner/guard`) and Tape already see hook calls, so a delegated sub-task with `files[]` scope could be enforced the same way (RFC §10/§15).
7. **Integrating worker results must be safe.** Refuse to merge a worker's output if the lead's `HEAD` moved, and keep the worktree for manual review (`App.tsx:11027`). Use explicit dispatch stages (`accepted … integrated → cleaned`) plus `start_unknown`/`stop_unknown`, so a hostd restart knows whether a sub-task really started (RFC §11 durable runs).
8. **A recovery turn, not a silent restart.** On interruption, resume in the retained checkout with a prompt that says what happened, tells the agent not to repeat destructive or external operations, and asks it to report done vs. now-done (`orchestration.ts:159-160`). Atlas resumes up to `restarts` with `resumed: true`; adopting this wording is a cheap safety gain.
9. **Map permission modes to each engine's native knobs, and always send the mode explicitly** (`claudeProtocol.ts:108-125`, `codexProtocol.ts:62-95`). Atlas's `approval.rs` already insists on ask mode. Keep doing that, and test it per engine the way Monocode's `*Live.test.ts` do.
10. **Keep memory outside the repo, with hygiene.** Use dated bullets that are superseded rather than deleted, optional `until` dates, a per-entry size cap, regex secret redaction before disk, atomic 0600 writes, and hash-checked saves (`monoMemory.ts`, `mono.rs`). This is useful for RFC §14 if crewmate memory becomes editable by both agent and human.

### 11.3 What not to copy

1. **Orchestration in the UI process.** Monocode's `Orchestrator` lives in the React app, and runs stop when the window closes (`control.rs` `close_window` drops the window's grants and pending calls, lines 101-118). Atlas's hostd-owned, always-on crew is the right architecture for autonomous organizations. Keep the engine in hostd.
2. **Polling for remote and mobile state.** Monocode's host sync and phone lists are pull-based. Atlas already pushes over hub broadcast; keep push and add revisions only for catch-up.
3. **Approvals that don't work on the phone.** Monocode's mobile still sends approvals and questions back to the desktop. Atlas's phone already answers actions; don't regress.
4. **Whole transcripts as one JSON column** (`sessions.blocks_json`). It works for a single-user GUI, but every save rewrites the transcript. Atlas's append-only JSONL with deltas and compaction is better for long autonomous runs.
5. **A 15k-line `App.tsx` and a 5.4k-line transcript component.** The density of features in one file is a maintenance risk. Atlas should keep crew run rendering split by event kind.
6. **Animation-heavy surfaces as identity** (mascots running on the composer, constellations, word fade). They are pleasant for a solo user, but each costs CPU (Monocode added resource-pressure tooling and a transcript pool to compensate). For an org dashboard with many agents, prefer the quiet patterns (ticker, staggered rows) and make celebrations opt-in.
7. **Multi-provider breadth as a goal.** Eleven adapters cost Monocode about 59k lines in `src/integrations/harness/`, and the maintainer froze new providers (`CONTRIBUTING.md`). Atlas crew engines (claude, codex, omp, opencode in `atlas:crates/atlas-host/src/crew/engine/`) should grow only where the typed event contract can be met fully.
8. **Letting the lead answer workers' approvals with no rails.** Monocode lets the lead `respond allow` for any worker approval. In Atlas, a delegating crewmate must not be able to raise a delegate's authority past the policy rails (`approval.rs` order: rails → mode → grants → person).
9. **No money view.** Don't stop at tokens. RFC §31 needs spend per crewmate, project and goal.

## 12. Dependencies between lessons (what must exist first)

1. Typed hostd run events (11.2-1), plus merge-by-id and bounded windows (11.2-2), come before the tool chips, diff-in-approval, todo, subagent trails and post-turn review card (11.1-1/2/3/7).
2. A revision per task and `sync(since)` (11.2-3) come before cheap phone catch-up and an offline cache.
3. `command_id` receipts (11.2-4) come before safe retries from the phone and from Telegram or voice.
4. Delegation with `files[]` scope (RFC §10) comes before the write-scope guard (11.2-6) and safe integration (11.2-7).
5. A run outcome of `posted | quiet | failed` comes before heartbeat backoff and the notification-fatigue rules (11.1-10/11).

## 13. Compatibility notes for adopting this in Atlas

- **Disk format.** New delta kinds in `tasks.jsonl` must be ignorable by an older hostd. The existing rule, "a file with deltas read by an older hostd just misses the in-flight steps until the task's final full record" (`tasklog.rs:24-25`), means new kinds must never replace the final full `Task` record.
- **Wire.** Keep `crewmate_task` with the full task for old desktop and mobile clients. Add typed events behind a capability flag in the hello or handshake, as Monocode's `HostDescriptor.capabilities` does. The golden fixtures (`atlas:apps/mobile/test/fixtures/crewmate_task_event.json`, `apps/desktop/src/__fixtures__/crew_paused_event.json`) need siblings for every new event.
- **Step field.** `Step.text` and `group` must stay for old clients. A typed preview can be an optional field on the same record.

## 14. Open questions

1. Should crew run events reuse Monocode's `HarnessEvent` names (and so AG-UI-like terms), or follow Atlas naming? The StudyAGUI peer's findings should decide the vocabulary.
2. Where should diff previews come from for engines that don't report them? Atlas hooks give `tool_input`; a write preview needs either the hook's before/after or a worktree diff per step.
3. Does hostd already dedupe a resent `crewmate_task` from the phone? I did not verify this; it decides whether 11.2-4 is a fix or a hardening.
4. Should crew show dollar estimates computed from token counts and model prices, or only report what the engines say?

## 15. Evidence log

- `git clone --depth 50 https://github.com/hardbeat920/monocode /tmp/monocode` gave HEAD `09d0443` (2026-11-12).
- GitHub API: `repos/hardbeat920/monocode` (stars, forks, issues, dates, MIT), `releases` (86, latest `v0.16.0` 2026-10-19 and `native-v0.16.0-preview.14` 2026-11-05), `commits?per_page=1` Link `last=1558`, contributors (88).
- Images viewed: `docs/screenshot.jpg` (Tauri classic layout) and the README hero `https://github.com/user-attachments/assets/89e04066-69de-473d-9aab-88b9ca3e3b7b` (native GPUI layout).
- I ran no Monocode code or tests, and no Atlas tests (no claims here depend on running them). The Atlas refs were read at `origin/main` `72b47fc3`.
- `/tmp/monocode` and the downloaded image were deleted at the end.
