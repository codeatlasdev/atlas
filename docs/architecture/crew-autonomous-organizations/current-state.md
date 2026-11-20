# Atlas Crew current state

Verified baseline of Atlas Crew at `main` `72b47fc3`, taken on 2026-11-20. It is the starting point for the RFC "Atlas Crew: Autonomous Organizations & Agent Operating System" (`rfc.md`). This file says what exists, what a test proved, what is broken, and what any change must not break. The per-capability table is in `capability-matrix.md`. The raw evidence is in `research/`.

## 1. How to read this file

- Evidence comes from three code audits (`research/audit-runtime-authority.md`, `research/audit-organization-work.md`, `research/audit-surfaces-ax.md`), a repo survey (`research/dogfood-targets.md`) and two studies (`research/study-ag-ui.md`, `research/study-monocode.md`). I re-read the cited lines for the defects in section 17.
- Paths are repo-relative. `file:line` is as read on `72b47fc3`.
- `[INFERENCE]` marks a claim that was derived and not observed.
- Status words are the ones in the matrix: Implemented and verified, Implemented, incomplete, Prototype, Planned, Missing, Unknown.
- Layers follow the RFC capability model (section 5 of the RFC), regrouped into 14 sections.

### 1.1 What was run

All Rust runs went through the heavy queue from the main checkout with `CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 nice -n 15` on `cargo nextest run -p atlas-host --locked` and a filter. Filters overlap, so counts are not additive. The `/tmp` logs of these runs were local and were not kept.

| Filter (atlas-host unless noted) | Result |
|---|---|
| `approval` | 36 passed |
| `permissions` | 27 passed |
| `destructive or rail or protected` | 12 passed |
| `recover` / `restart` | 6 / 26 passed |
| `pause` | 21 passed |
| `audit` | 39 passed |
| `guard` | 50 passed |
| `hook_auth` / `hook` | 7 / 74 passed |
| `cgroup` / `ruleset` / `provision` | 13 / 1 / 23 passed |
| `spawn` / `isolation` | 107 / 30 passed |
| `secret` / `usage` / `capture` / `untrusted` | 30 / 13 / 63 / 10 passed |
| `tape or usage or a_task_that_` | 24 passed |
| `server_busy or a_limit_of` | 1 passed (the `max_agents` name matches no test, rc=4) |
| Organization batch 1 (triggers, schedule, push, telegram, summary, spoken, reporting, store, plus 21 `crew::tests::*` modules) | 259 run, 259 passed |
| Organization batch 2 (hygiene, actions, github_fake, github_app, answers, archive, relay, recover, trigger sources) | 58 run, 57 passed, 1 killed by the tool's 300 s limit |
| `crew::relay` alone | 11 run, 11 passed (one test takes 20 s) |
| Surfaces batch 1 (audit, payloads, mobile_payloads, protocol_golden, pause) | 31 passed |
| Surfaces batch 2 (capture, locate, uigate, prove_ui, engines, omp_approval, github_app, voice, permissions, approvals, relay, mcp, engine_login) | 246 passed |
| Surfaces batch 3 (telegram, github_*, triggers, automations, crewmates, tasks, push) | 136 passed |
| Desktop `bun test --conditions browser src/lib/crew` in a prepared copy | 833 pass, 0 fail, 67 files |
| Desktop the same command in the main checkout | all 67 files fail before any test (see D10) |
| `atlas-cli` `-E 'test(crew)'` | 177 run, 176 passed, 1 failed (`commands::crew::browsers::tests::the_install_scripts_leave_a_host_that_status_calls_ready`, reads the local Playwright cache; environmental) |

Not run by any audit: Flutter tests, `scripts/crew-e2e.sh` and its systemd workflow, every `#[ignore]` root test (`spawn/cgroup.rs:557,598,709`, `spawn/exec.rs:789,865`, `spawn/git.rs:355`, `spawn/provision.rs:1226`, `mirror/tests.rs:473`). Kernel-level isolation, cgroup OOM and nft packet drops are therefore known from code reading and CI, not from an audit run. `atlas dev` was not running.

## 2. Compatibility rules that apply to every layer

Any change to Crew must respect these. Each layer below repeats the ones that bite it.

1. **Wire protocol.** `PROTO = 2` (`crates/atlas-host/src/protocol.rs:77`). A new command, optional argument or reply key is additive and needs no bump (`protocol.rs:69-76`). A removed or renamed field needs a `PROTO` bump and the phone's `kAppProto` and `kMinHostProto` (`apps/mobile/lib/core/protocol/protocol_version.dart:6-10`).
2. **New command checklist.** Regenerate `testdata/hostd/commands.json` with `UPDATE_FIXTURES=1` (`protocol_golden`); classify the command in `client.rs` `QUICK`, `LONG` or `SPECIAL` (`client.rs:120-240`); give it a phone scope in `required_scope` or an explicit refusal (`crates/atlas-host/src/mobile/mod.rs:498-612`).
3. **Capabilities.** Clients check `info.caps` (`protocol.rs:84-92`) and today test only for `"crew"`. A feature that older servers lack needs a new cap before a client relies on it. Servers are updated one by one with `atlas crew update`.
4. **Phones.** A phone reaches a crew server only through the Mac relay (`crates/atlas-host/src/crew/relay/mod.rs`). The relay forwards a push only if `push_scope` names its event (`mobile/mod.rs:456-471`); an unlisted event is dropped (`mobile/session.rs:88`). Terminals, GitHub App, Telegram, voice setup and secrets are refused for phones (`mobile/mod.rs:554-576`).
5. **Contract fixtures.** Rust `*_fixture_is_real_payload` tests write `apps/mobile/test/fixtures/*.json`. The Flutter tests and the desktop tests read the same files (`apps/desktop/src/lib/crew/crewmateTaskResumed.contract.test.ts:8`). A payload change must move Rust, desktop and mobile together.
6. **Closed sets in clients.** Desktop declares `TaskState` as a closed union (`apps/desktop/src/lib/crew.ts:42`) and `TaskAction.kind` as a closed union (`crew.ts:48-55`). The board switches on state with no default branch (`apps/desktop/src/lib/crew/board.ts:~146`, `apps/mobile/lib/features/crew/board.dart:208-225`, `apps/mobile/lib/features/crew/chat/task_thread.dart:228-245`), so an unknown state silently drops the card. Read from code, not tested. New `TaskState` values, `TaskAction` kinds and `TriggerKind` types are therefore not additive for clients [INFERENCE for trigger kinds: the Dart models were not read].
7. **Serde round trip loss.** Types use `#[serde(default)]` and do not deny unknown fields, so an older hostd reads a newer `crewmate.json`, `crew.json` or `tasks.jsonl` without error. But an older hostd that saves one of them (`set_profile`, `crewmate_update`, `save_crew_settings` at `store/mod.rs:1123`, or `tasks.jsonl` compaction past 4 MiB) rewrites it with its own struct and drops fields it does not know [INFERENCE: struct round trip]. A rollback after adding fields silently loses them. Prefer separate files per concern, `skip_serializing_if` defaults, or `#[serde(flatten)] extra`.
8. **Archive.** `FORMAT_VERSION = 1` (`crates/atlas-host/src/crew/store/archive.rs:31`). Import refuses an unknown format and never exports `<trigger>_seen.jsonl` or sessions. A new kind of artifact needs a version bump or an explicit exclusion.
9. **Spawn helper.** The root helpers `atlas-crew-spawn` and `atlas-crew-provision` are separate from hostd and are installed by `atlas crew install`. A new helper request, cgroup field or nft rule needs the operator to re-run `install`. `update` swaps the binary and restarts `atlas-crew` only [INFERENCE: `swap_script` restarts `SERVICE`]. An old helper refuses an unknown request, so tasks stop (fail closed).
10. **Task log and index.** `tasks.jsonl` is append-only, last full record per id wins, step deltas are separate lines. `tasks.idx` has `IDX_VERSION = 1` and is rebuilt when the version differs (`store/tasklog.rs:47`). Adding an optional `Task` field is safe in both directions.
11. **Audit.** `audit.jsonl` readers skip bad lines and tolerate new fields. `seq` continuity is read from the files on restart (`audit/mod.rs` `last_seq`), so a change of line shape needs a version field. Rotation is 1 MiB times 5 files.
12. **Names.** Legacy `bots/`, `bot.json`, the `approval: ask|auto` alias, the `bot` trigger alias and the `/bots/...` webhook paths still work and must keep working. The webhook URL `/crew/<id>/hook/<token>` is stored in external systems. The Linux user slug comes from the crewmate id, so an id format change orphans users and directories. The pane key `crew:<crewmate id>:<task id>` is parsed in several places (`crew/pane.rs`).
13. **Isolation.** Where crewmates run as their own Linux user, hostd opens files in a crewmate tree only through `spawn::layout` `*_beneath` and `*_nofollow` helpers (`store/mod.rs:917-919`). A shared store must be hostd-owned.
14. **Concurrency.** Handlers load, modify and save `crewmate.json` without a lock (comments at `triggers/mod.rs:218-222`, `schedule.rs:130-134`). More writers raise the lost-update risk.
15. **Push is not live.** `docs/crew.md:524`: the relay is not deployed and the app has no Firebase. Do not design on mobile push.

## 3. Host authority and security

**What exists.**
- One Linux user per crewmate (`atlas-cm-<12 hex>`), created by a root provision helper, with a spawn helper that validates every request: peer credentials, user shape, engine binary allowlist, cwd inside the crewmate dir, no `LD_*`, `PATH` or `HOME`, signal list, 12 git subcommands (`crates/atlas-cli/src/commands/crew/isolation.rs:71-152`, `crates/atlas-host/src/crew/spawn/validate.rs`). Fail closed: isolation installed and socket missing means every task fails.
- Cgroup leaf per crewmate with `memory.max`, `memory.oom.group=1`, `memory.swap.max=0`, `pids.max=4096` (`spawn/cgroup.rs:306-314`). Cap is 75 percent of memory divided by `max_agents`, minimum 1 GiB (`cgroup.rs:65-71`).
- nftables table `inet atlas_crew`, one chain per crewmate: drops loopback, RFC1918, link-local, CGNAT, ULA and NAT64, then allows TCP 22, 80, 443 (`spawn/provision.rs:243-331`).
- Every claude or omp tool call ends in `approval::decide` (`crew/approval.rs:665`): rails, then mode, then task grants, then a person. Modes `ask`, `edits`, `bypass`. Rails: protected paths, `.mcp.json`, shell rc, `.env*`, keys, and `no_auto` tasks. Agents start in ask mode, never with bypass flags (`runner/pty.rs:30-41`, `engine/omp.rs:86-90`).
- Held approvals wait 30 minutes then deny (`agents/hook_server.rs:37`). Answers are bound to `(pane, req)` (`agents/mod.rs:432-522`). Only owner connections change a mode (`approval.rs:941-955`).
- Kill switch: `crew_pause` and `crew_resume` (`crew/hub.rs:114-137`), stored in `crew.json`; an unreadable file reads as paused (`store/mod.rs:1071-1086`).
- Per-crewmate secrets in `secrets.json` mode 0600; hook token per crewmate (`store/hooktoken.rs:15`); constant-time token compare on the hook server (`hook_server.rs:257-272`).
- Admission: `max_agents` slots (default 4) and one running task per crewmate (`runner/slots.rs`, `runner/mod.rs`).
- Install, update and rollback scripts with health wait and automatic restore (`crates/atlas-cli/src/commands/crew/update.rs:97-196`, `binary.rs:289-330`).

**Verified by tests.** `approval` 36, `permissions` 27, `destructive or rail or protected` 12, `hook_auth` 7, `hook` 74, `spawn` 107, `isolation` 30, `provision` 23, `cgroup` 13 (arithmetic only), `ruleset` 1 (rule text only), `pause` 21, `secret` 30, `capture` 63, `untrusted` 10, `guard` 50, `a_limit_of_two_runs_at_most_two_agents...` 1. `atlas-cli` crew tests 176 of 177.

**Known broken or contradictory.**
- `destructive()` gates voice approvals only (D8). In `bypass`, `rm -rf` runs without a prompt.
- Webhook token compare is not constant time (D3).
- codex and opencode cannot ask, so they run unattended in `bypass` (`approval.rs:921`).
- The capture browser has no OS sandbox and runs as the crew user (`capture/runner.mjs:263-266`; `docs/crew.md:618`, N4). The doc marks the fix as planned.
- Egress is IP and port only and always uses `Policy::default()` (`provision.rs:409-415`).
- Port 47631 can be taken by a detached crewmate process (`docs/crew.md` S-A, not fixed).
- On a Mac host, or a Linux host without the helper, crewmates share the crew user [INFERENCE: `layout::active()` is false off Linux].

**Compatibility constraints.** Rules 2, 3, 7, 9, 12, 13. Specific: keep `Mode` as the fallback when policy layers are added; keep fail-closed behavior (`crew.json` unreadable means paused, isolation fault means no run, approval error means deny); policy files must live where the crewmate user cannot write; a new helper request needs a re-install step in `atlas crew update` docs and release notes.

## 4. Durable execution

**What exists.**
- `Task` and `TaskState {Queued, Running, Done, Failed, Cancelled, WaitingAction}` (`crew/store/mod.rs:340-447`). State, steps and usage are saved on every change (`runner/outcome.rs:263-298`). Engine sessions are saved after each turn (`runner/conversation.rs:250-258`).
- Recovery at start (`runner/recover.rs`): queued stay queued; running goes back to the queue head with `resumed: true` and `restarts += 1`; the third interruption fails with a named error (`MAX_RESTARTS = 2`). A terminal agent runs again from the start. A `waiting_action` task stays waiting. A paused crew resumes nothing.
- Idempotency at specific points: GitHub progress comment reuse (`runner/reporting.rs:41-70`), the GitHub seen file written before firing (`triggers/github.rs:4-11`), webhook delivery ids remembered across restart (`github/app/webhook.rs`).
- Pending tool approvals are in memory by design (`approval.rs:476-518`). A resumed task asks again.

**Verified by tests.** `restart` 26, `recover` 6, `queued_task_survives_restart`, `a_task_is_resumed_until_the_cap_then_fails`, `a_running_conversational_task_resumes_its_session_after_a_restart`, `a_running_terminal_task_runs_again_after_a_restart`, `a_task_interrupted_three_times_fails_with_a_clear_error`, `a_paused_crew_holds_the_interrupted_task_until_resume`, `a_waiting_action_task_can_be_answered_after_a_restart`, `a_delivery_id_is_taken_once_and_remembered_across_a_restart`. `crew-e2e.sh` restarts hostd mid-task in CI (not run by an audit).

**Known broken or contradictory.**
- No tool-call journal: `tool_call` audit lines record intent only, and `Tape` keeps results in memory and drops them at task end (`runner/tape.rs`). So hostd cannot say which external action already happened.
- No uncertain-outcome state or reconciliation. `git push`, PRs and messages sent before a crash can be repeated.
- No parent and child run, no run id (a task is the run), no explicit `WaitingApproval`, `Recovering` or `OutcomeUncertain` states. Pause is `Cancelled` with error `paused` (`hub.rs:129`): the work is lost, not suspended.
- `restarts` counts every interruption alike (upgrade, reboot, crash). Only the last 50 tasks per crewmate are scanned at recovery (`recover.rs:92`).

**Compatibility constraints.** Rules 6, 7, 10. Do not add `TaskState` values: older hostds park the line as `unknown` and the task disappears from their view [INFERENCE from `tasklog.rs`], and clients drop it. Use an optional `run` object on `Task`. `Task.chain` holds crewmate ids, not task ids.

## 5. Agent runtime and engines

**What exists.**
- `Crewmate` (`store/mod.rs:263-296`): id (UUID), name, role (2000 chars), agent, engine, mode, project, triggers, look, created_at. Identity text is free and the agent can rewrite name and role with `set_profile` (`agents/hook_server.rs:570-596`).
- Four headless engine drivers: claude, codex, opencode, omp (`crew/engine/*.rs`), each started one-shot. omp uses `--mode rpc` and hostd reads it only until `prompt_result`. Terminal-agent crewmates run in a pty (`runner/pty.rs`).
- Engine sign-in status (`crew_engine_status`, `engine_login.rs`).
- One engine session per crewmate, resumed across tasks (`CrewmateSession`, `store/mod.rs:161-170`).
- Guards on finished work: weakened tests, strays, invented or overclaimed results (`runner/guard.rs`, `gate.rs`).
- Runtime state `idle`, `working`, `needs_you` computed on read (`handlers/crewmates.rs:82-93`).

**Verified by tests.** `engines`, `omp_approval`, `engine_login` (inside the 246 batch), `guard` 50, `hook_auth` 7, `isolation` 30, `crewmate_state` 4, `crew::tests::tasks::crewmate_tasks_steps`.

**Known broken or contradictory.**
- omp gets no atlas MCP server: `engine/omp.rs` never uses `McpServer`, so an omp crewmate has no `ask_user`, `capture`, `locate`, `ui_gate`, `set_ui` or `set_profile` (`engine/launch.rs:481-508`). omp is the first engine in `crates/atlas-core/data/agents.json`.
- codex and omp report no `cost_usd` (`engine/codex.rs:129`, `engine/omp.rs:254`); terminal agents report no usage at all.
- `engine_limits` is a stub (`handlers/settings.rs:295-300`).
- No definition version and no per-run identity: a run carries the crewmate id and pane key only. The session is per crewmate, so context can bleed between tasks.
- No per-crewmate pause, disable or archive; delete removes the Linux user and the directory.

**Compatibility constraints.** Rules 7, 9, 12. A new `Crewmate` field must be optional. The Linux user and directory names derive from the id.

## 6. Organization model

**What exists.**
- One crew server is one implicit organization: a flat list of crewmates under one `crew.json` (`CrewSettings`: `paused`, `max_agents`, `agent_memory_max`, `default_approval`, `allow_bypass`, `voice`, `public_url`; `store/mod.rs:496-527`).
- A listing of crewmates (`list_crewmates`, `store/mod.rs:741`; `crewmate_list`, `handlers/crewmates.rs:101-215`). The desktop keeps a registry of servers in `crew-servers.json` and the relay can call another server (`crew/relay/mod.rs:70-90`).
- Export and import of a crew: manifest, `crew.json`, per-crewmate definition, memory, tasks, usage (`store/archive.rs`).
- A setup chat that configures one crewmate (`apps/desktop/src/lib/crew/chat/setup.svelte.ts`, `Crewmate.setup_step`).

**Verified by tests.** `archive_tests` 22, `pause` 12, `limits`, `relay` 11, `crewSetup.test.ts` (in the 833 desktop tests).

**Known broken or contradictory.** Nothing contradictory. Missing: organization entity, departments, structured responsibilities, organization graph, blueprint, organization architect, per-agent lifecycle states beyond idle, working and needs_you. A grep for `department`, `reports_to`, `manager`, `team_id`, `hierarch` and `blueprint` in `crates/atlas-host/src/crew` finds only unrelated hits.

**Compatibility constraints.** Rules 7, 8, 12. New org data must be optional and have an export and import path and a `crew-e2e` uninstall check (`scripts/crew-e2e.sh:665,694`). Cross-server organization has no home: hostd cannot reach another hostd.

## 7. Communication and delegation

**What exists.**
- The only inter-crewmate edge is the `Crewmate{from_crewmate, on}` trigger (`store/mod.rs:104-112`) fired from `triggers::on_task_finished` (`triggers/mod.rs:250-340`), called from every finish path. `Task.chain` blocks repeats.
- Routing is explicit: the sender names the crewmate (`crewmate_task`, `handlers/tasks.rs:25-72`; Telegram `/task <name>`).
- Per-crewmate FIFO queue and a server cap (`runner/mod.rs:77-110,320`).
- Human escalation: MCP `ask_user` (`agents/hook_server.rs:513-567`), `TaskAction` cards, `needs_you` state.
- An agent may propose an automation through `ATLAS_ACTION add_trigger`, which a human must accept (`runner/action.rs:204-228`).

**Verified by tests.** `crewmate_chain_trigger_fires_on_finish`, `hygiene` 5 (a mutual cycle stops after one round with `chain_refused`; a long A to B to C chain still runs), `actions` 6, `automations` 7.

**Known broken or contradictory.**
- No agent-to-agent message, no delegation route or tool, no mailbox. The MCP server has 7 tools and none addresses another crewmate (`crew/mcp.rs:166-174,345`).
- The chain event is one line, `Crewmate {name} done|failed`; it does not carry the result or the task id, and `cancelled` does not fire (`triggers/mod.rs:~270`).
- No depth limit, no fan-out limit, no queue cap; the queue is an unbounded `VecDeque` [INFERENCE].
- Two crewmates whose triggers both match a GitHub repo both start (no assignment arbitration).
- No client reads `Task.chain`; there is no delegation view.

**Compatibility constraints.** Rules 5, 6, 7, 10. A delegated task must reuse the pane key and the `fire` order (pause check, store, `last_fired`, enqueue, broadcast; `triggers/mod.rs:460-540`). `no_auto` must propagate from parent to child: no field carries it across tasks today. Agent text must pass `untrusted_event`. A delegation route needs rate and depth limits before it exists, because the crewmate token is the only credential and its process controls what it sends (`hook_auth.rs` `scrub` comment).

## 8. Autonomous work loop and triggers

**What exists.** `TriggerKind` has schedule, interval, webhook, github, file, crewmate and service (`store/mod.rs:76-128`). A 30 s ticker runs `tick_schedule` (`triggers/mod.rs:176-246`). GitHub has 8 event kinds, a 120 s poll and a signed webhook with delivery-id claim. Untrusted text is wrapped (`untrusted_event`, `author_allowed`, `no_auto`). A paused crew skips every trigger. A missed schedule fires once after downtime.

**Verified by tests.** `time_contract` 10, `old_crons` 1, `automations` 7, `triggers::tests` 9, `schedule::tests` 3, `github_in` 20, `github_out` 12, `github_webhook` 10, `github_hook_events` 6, `github_e2e` 5, `untrusted` 9, `a_trigger_does_not_fire_while_paused`, `webhook_wrong_token_rejected`, `webhook_correct_token_enqueues_task`, file watch 4 units, service 2 units (tail reading only), `routine_every_minute_fires_within_90s` (real 30 s).

**Known broken or contradictory.**
- Two schedulers: legacy `Routine` through `schedule::tick` (`schedule.rs:90-148`) and triggers through `tick_schedule` (D7).
- File triggers are registered once and never unregistered or reconfigured until restart (D6).
- Webhook token compare is `==` (D3); payload cut to 200 chars; no delivery id or rate limit.
- Service trigger is log scraping, in-memory state, no test of the fire edge.
- Every trigger starts a paid run unconditionally. No relevance check, no cooldown, no per-trigger cap, no `trigger_skipped` audit. An enabled 60 s interval can enqueue without bound [INFERENCE].
- Only GitHub has typed events and dedupe keys. No deadline, goal review, condition or MCP-event trigger. Missed-run behavior is fixed, not configurable.

**Compatibility constraints.** Rules 6, 7, 12. Webhook URLs `/crew/<id>/hook/<token>` and `/bots/...` must keep working (`triggers/webhook.rs:14-15`). A missed-run policy must apply to both schedulers or the routines must be migrated. A new `TriggerKind` variant is a client risk (rule 6).

## 9. Goals and budgets

**What exists.** `UsageRecord {model, tokens, cost_usd, duration_ms}` per task and in `usage.jsonl` per crewmate, rotated by day (`store/usage.rs`, `store/mod.rs:1014-1060`), read by `crewmate_usage`. Limits that bound spend today: `max_agents`, the 2 h task cap (`runner/mod.rs:85`), the 2-resume cap, the kill switch.

**Verified by tests.** `usage` 13.

**Known broken or contradictory.** No goal object, no hierarchy, no metrics, no review cadence, no budget of any kind (a grep for `budget` in `crew/**` finds only a config-key list). Cost data is never aggregated server-side or compared with a limit. Product text about the Grand Line vision lives in a GitHub discussion, not in crew code. USD budgets cannot be enforced for codex, omp or terminal agents.

**Compatibility constraints.** Rules 7, 8. Put goals and budgets in separate files, not inside `crew.json`, to avoid rollback loss. Archive needs an entry type or an exclusion.

## 10. Memory

**What exists.** One `memory.md` per crewmate, truncated at 64 KiB by the human and API path (`store/mod.rs:915-943`). The prompt tells the agent its path and to append durable learnings (`runner/prompt.rs:230-232`). Humans read and replace it through `crewmate_memory` and `crewmate_memory_set` (`handlers/settings.rs:11-23`) and the desktop and mobile profile screens. It travels in the archive. Isolated crewmates are read and written through no-follow helpers.

**Verified by tests.** Two isolation tests cover memory (`a_crewmate_without_a_user_is_provisioned...keeps_its_memory`, `an_export_of_an_isolated_crewmate_carries_its_current_memory`). The organization audit read them but did not run them; whether the runtime audit's `isolation` run (30 passed) included them was not checked. No test of the 64 KiB cap was found. No test shows the agent acting on memory text.

**Known broken or contradictory.** No scopes beyond one crewmate, no knowledge records, no provenance, no expiry, no retraction, no sharing, no search. `set_memory` writes no audit line (D4). The cut keeps the head and drops the newest lines and may split a UTF-8 character [INFERENCE from `write_memory`].

**Compatibility constraints.** Rules 8, 13. Shared knowledge must be hostd-owned: isolated crewmates cannot read each other by design. Do not replace the no-follow write path.

## 11. Reporting and notifications

**What exists.** `finish_task` stores the visible reply and broadcasts `crewmate_task` (`runner/outcome.rs:149-257`). Summary of long replies (`runner/summary.rs`, 600-char threshold, 25 s cheap model). Spoken line (`runner/spoken.rs`). GitHub reply comment posted and edited as progress (`runner/reporting.rs`, `github/reply.rs`). Telegram gets every done and failed task; push gets only done, plus needs_you for approvals (`outcome.rs:211-262`, `hub.rs` `forward_approvals`). Push code with grants and 403/410 cleanup (`crew/push.rs`).

**Verified by tests.** `tasks` 12, `payloads` 6, `mobile_payloads` 3, summary 4, spoken 7, `github_out` 12, `push` 7 plus `tests::push` 5, `telegram` 13 + 2 + 2, `relay` 11.

**Known broken or contradictory.**
- Push is not live (relay not deployed, no Firebase): `docs/crew.md:524`.
- A failed task sends Telegram but no push. A parked card (`connect_github`, `choose`, `ask_user`) sends neither push nor Telegram [INFERENCE: `park_task` and `handle_ask` call neither].
- A 60 s interval trigger sends one Telegram message per run. No quiet hours, preferences, aggregation, digest or categories. No `Report` object.
- Push `kind` set is closed: `task_done`, `needs_you`, `agent_*`.

**Compatibility constraints.** Rules 4, 5, 15. A new push kind needs the relay (`services/push-relay`, Go) and the app. Push payloads carry `kind` only, by privacy design.

## 12. Organization UX (desktop, mobile, Telegram)

**What exists.**
- Desktop: `CrewWorkspace.svelte` with a sidebar (server, project, crewmate), a Board and chat (`crewView.svelte.ts:9` allows only `chat` and `board`). Board columns Queued, Working, Needs you, Done (`board.ts`). Settings sections for memory, usage and audit. Setup chat.
- Mobile: `list/crew_screen.dart`, `crew_deck.dart`, `board.dart` (a port of `board.ts`), `needs_you_card.dart`, ops screens for audit, usage, settings.
- Telegram: one bot per server, linked chats, approval buttons, `/crew /mode /task /ui`, photos, voice notes (`crew/telegram.rs`).
- One state function drives attention: `crewmate_state` gives `needs_you` for a tool approval or a parked card (`handlers/crewmates.rs:84-93`).

**Verified by tests.** 833 desktop tests in a prepared copy (`board.test.ts`, `sidebar.test.ts`, `crewPalette.test.ts` and others). Rust `approvals`, `actions`, `prove_ui`, `omp_approval`, `telegram`, `mobile_payloads`. Flutter `board_test.dart` and `crew_board_golden_test.dart` were read, not run. Other mobile crew screens: Unknown.

**Known broken or contradictory.**
- `crew_paused` and `crew_engine_auth` never reach a phone (D1).
- `docs/crew.md:817` says Mobile has no audit screen; `apps/mobile/lib/features/crew/ops/crew_audit_screen.dart` exists (D2).
- Scenario H: 5 of 12 inspect items exist. No organization, department, goal, report, event or policy view. No cross-crew attention inbox with priority or SLA. No organization graph. No health score.
- Telegram broadcasts to all linked chats and cannot be reached from a phone through the relay.

**Compatibility constraints.** Rules 2, 4, 5, 6, 15. Desktop normalizes old servers in `apps/desktop/src/lib/crew/wire.ts:23-39` and `audit.ts` still reads `by` for pre-alpha.28 entries: any new field must be optional with a client default. All user-visible text goes through `i18n/messages/` in en and pt-BR.

## 13. Agent experience and MCP tools

**What exists.**
- Operating context in the first prompt: `You are {name}. {role}`, the permission fragment (mode, rails, what runs without asking, what to do if denied), time zone line, branch instruction for GitHub tasks, the `ATLAS_ACTION` grammar with 8 kinds, memory path (`runner/prompt.rs:204-249`, `approval.rs:430-470`). The fragment is sent again on a mode change (for omp as a steering message).
- MCP server `atlas mcp --crewmate <id>` with 7 tools: `ask_user`, `set_profile`, `capture`, `locate`, `ui_gate`, `set_ui`, `approve` (`crew/mcp.rs:159-166`, schemas `345-470`). Each calls hostd's local HTTP hook with the crewmate token. Wired for claude, codex, opencode.
- Feedback: allow once, allow for task, deny with a comment the agent reads; `refused` and `prove_ui` cards.

**Verified by tests.** `crew::mcp` units including `tools_list_names_every_tool_exactly_once`, `engines`, `capture`, `locate`, `uigate`, `approvals`, `permissions`, `omp_approval`, `actions` 6, `guards`, `prove_ui`.

**Known broken or contradictory.** No responsibilities, charter or goals in the prompt. The prompt lists no tools, teammates or budget. An agent cannot read its own audit, tasks or usage. `set_profile` writes no audit line and lets the agent rewrite its own role (D4). omp has no atlas tools (section 5). Feedback stays inside one task and is never aggregated.

**Compatibility constraints.** Rules 7, 9. A new agent route needs an `AGENT_ROUTES` entry (`agents/hook_auth.rs:16-25`), authorization by caller id, and a body cap. A new MCP tool needs `tools_list()` and a test for it.

## 14. Capabilities and integrations

**What exists.**
- GitHub: App via manifest flow, installation tokens, credential helper, bot identity; `gh` device login; signed webhook; triggers; reply comments (`crew/github/**`).
- Browser: `capture` (Playwright chromium and webkit, widths and themes, DOM dump), `locate`, `ui_gate` with a regressions file (`crew/capture`, `locate`, `uigate`). Opt-in install, about 500 MB.
- Voice: whisper.cpp or a hosted engine, spoken replies, voice answers to cards (`crew/voice/**`, `voice_card.rs`).
- Static engine data (`crates/atlas-core/data/agents.json`) and a desktop catalog (`apps/desktop/src/lib/crew/integrations.ts:13-190`).

**Verified by tests.** `github_app`, `github_in`, `github_out`, `github_webhook`, `github_hook_events`, `github_e2e`, `capture` 63, `locate`, `uigate`, `prove_ui`, `voice`, `voice_answers`. `crew-e2e.sh` checks capture refusals and ui_gate isolation in CI (read, not run).

**Known broken or contradictory.** No capability registry or per-crewmate grants. No MCP client to external servers: the only server an engine receives is `atlas` (`engine/launch.rs:182-196`). Only GitHub has `status: "real"` in the catalog; Linear, Notion, Slack, Figma, 1Password and Google Docs are `coming-soon` (`integrations.ts:105-155`), and most GitHub skills are `available: false` (`integrations.ts:76-94`). No computer use. The webhook needs a public HTTPS URL the owner supplies.

**Compatibility constraints.** Rules 2, 3, 9, 13. `.mcp.json` is a protected rail path; an external MCP server list must go through it and through `approval::decide`. Secrets for those servers would be env vars only (`launch.rs:149-156`).

## 15. Interop (MCP, A2A, ACP, AG-UI)

**What exists.**
- MCP server for agents (section 13) and `atlas mcp` for the project dev environment (`docs/mcp.md`, protocol revision 2025-11-25; not exercised by the audits).
- hostd NDJSON wire protocol with `PROTO = 2`, 126 commands of which 45 are `crew*`, pinned by `protocol_golden`. Pushed events carry a tool-call lifecycle (`agent_step`, `agent_tool_output`, `agent_approval` and `agent_approval_done`), a status machine (`agent_status`) and task snapshots (`crewmate_task`).
- Phone relay: `crew_call{server, cmd, args}` with scopes.

**Verified by tests.** `protocol_golden` (31 in batch 1), `relay` 11.

**Known broken or contradictory.** No A2A, no ACP, no AG-UI: a grep for `a2a`, `agent2agent`, `agent-client-protocol` finds nothing in `crates`, `apps` or `docs`. Engines are driven as CLIs (`claude -p`, `codex exec --json`, `opencode run`, `omp --mode rpc`) and run one-shot, so hostd has no token deltas. There is no run id, only a pane key and task id. Events are not sequenced or replayable; a slow client is dropped and refetches (`protocol.rs:12-16`). `agent_step` reaches desktop connections but no client reads it, and phones never receive it (D11). `research/study-ag-ui.md` finds that AG-UI 1.0 events can be produced from these streams with an adapter and recommends it at the edge, not as the internal protocol.

**Compatibility constraints.** Rules 1, 2, 4. `crewmate_task` carries the whole task on every step (up to 50 steps and the prompt); a run stream must be its own event with its own scope, not more fields on it. The per-connection queue is 8 MiB.

## 16. Observability, audit, evaluation

**What exists.**
- Audit: `audit.jsonl` per crewmate and `crew-audit.jsonl` crew-wide, redacted (secret values become `[secret:NAME]`, strings capped at 8 KiB), rotated at 1 MiB times 5 (`crew/audit/mod.rs`, `audit/redact.rs`). Kinds include `tool_call`, `approval_asked`, `approval_decision` (decision, source, device, `wait_ms`, rail, comment), `approval_mode_changed`, task lifecycle, `config_changed`, `secret_set`, `task_refused`, `refusal_overridden`, `task_unproven_ui`, `chain_refused`, `audit_gap`, pause and resume.
- Readers `crewmate_audit` and `crew_audit`; desktop `settings/AuditSection.svelte` and mobile `ops/crew_audit_screen.dart`.
- Per-task steps (up to 50), usage records, `crewmate_usage`, desktop `UsageSection`, mobile `crewmate_usage_screen.dart`.
- End-to-end CI: `.github/workflows/crew-e2e.yml` runs `scripts/crew-e2e.sh` on a privileged systemd container (nightly and on crew changes). 48 Rust test files, about 361 test attributes under `crew/tests/`.

**Verified by tests.** `audit` 39 (including approval attribution, redaction, restart persistence), `crew::tests::audit` 8, `mobile_payloads::audit_fixtures_are_real_payloads`, `usage` 13, `crewmate_tasks_steps`.

**Known broken or contradictory.**
- Audit is a convention, not tamper evident: O_APPEND without fsync (`store/jsonl.rs:60`), no hash chain, the crew user can rewrite it, the fifth file is deleted at rotation. No `run_id`, `policy`, `result` or person identity (`device` is a connection class).
- `set_memory` and `set_profile` leave no audit line (D4).
- Headless-engine tasks record no steps and, as far as code shows, no `tool_call` audit lines unless the engine's own hooks call `atlas hook` (D5, status Unknown).
- No cross-crewmate query, per-task view or export. The desktop `AuditKind` union lists 9 kinds while the host writes more.
- No traces, no model-call records, no evals, no dry-run or simulation. The fake engine `run_fake` (`engine/mod.rs:241-264`) is test only.
- The desktop crew test suite fails from a clean checkout (D10).

**Compatibility constraints.** Rules 5, 11. New high-volume kinds (traces) need their own file or limit because rotation caps the audit at about 5 MiB. A line-shape change needs a version field.

## 17. Defects found by this audit

Each item was observed in source on `72b47fc3`. Items D1 to D4 and D6 to D8 were re-read for this file.

| # | Defect | Evidence | Effect | Smallest fix |
|---|---|---|---|---|
| D1 | `crew_paused` and `crew_engine_auth` are missing from `push_scope`, so the relay drops them for phones. The doc says scope `act`. | `crates/atlas-host/src/mobile/mod.rs:456-471` lists no such arm and ends in `_ => return None`; `docs/crew.md:1003` says "Pushes `{"ev":"crew_paused","paused":bool}` (Mobile scope `act`)"; `mobile/session.rs:88` drops unscoped lines; `apps/mobile/lib/features/crew/crew_provider.dart:46` lists `crew_paused` as a refresh event that never fires from a push. No test covers it. | A phone does not learn that the crew was paused until it refetches. Nothing consumes `crew_engine_auth` in `apps/desktop/src` or `apps/mobile/lib`. | Two match arms, a relay test beside `crew/relay/tests.rs:233`, a fixture. |
| D2 | `docs/crew.md:817` says Atlas Mobile has no audit screen. It has one. | `docs/crew.md:817` versus `apps/mobile/lib/features/crew/ops/crew_audit_screen.dart` and `crew_ops_provider.dart`. | Users and agents get wrong guidance. | Fix the doc sentence. |
| D3 | The generic webhook token is compared with `==`, not in constant time. | `crates/atlas-host/src/crew/triggers/webhook.rs:42-44` (`TriggerKind::Webhook { token: tk, .. } if tk == token`). `constant_time_eq` exists at `crates/atlas-host/src/crew/store/hooktoken.rs:50` and the hook server uses it (`agents/hook_server.rs:262-263`). | Timing side channel on the only secret of that route (no HMAC, no rate limit). | Use `constant_time_eq`; add a rate limit. |
| D4 | `set_memory` and `set_profile` write no audit line. | `crates/atlas-host/src/crew/handlers/settings.rs:17-23` (no `audit.record`); `crates/atlas-host/src/agents/hook_server.rs:570-596` (no audit call in the file at all by grep). `handlers/crewmates.rs` `audit_changes` records only `role_chars` for updates through `crewmate_update`. | An agent can rewrite its own role, and memory can be poisoned, with no trace of old text or task. | `memory_changed{task_id, bytes, sha}` and a `config_changed{field: role}` line. |
| D5 | Headless-engine tasks record no steps. | `crates/atlas-host/src/crew/runner/conversation.rs` has no `Step` or `append_step` call; omp tool results go only to the guards' `Tape` (`runner/tape.rs:69`); `atlas crew install` sets up no hooks. `docs/crew.md:796` claims `tool_call` audit covers "terminal agents and headless engines alike". | Activity feed and `tool_call` audit may be empty for headless crewmates. Status Unknown until a live run with `claude` headless. | Emit steps from engine streams through `ToolSink` (`engine/mod.rs:174`); correct or confirm the doc. |
| D6 | File triggers are never unregistered or reconfigured after an edit until restart. | `crates/atlas-host/src/crew/triggers/file_watch.rs:19,34-46`: `registered` set only grows; rescan every 120 s only adds new trigger ids. | Editing the glob, disabling or deleting a file trigger has no effect until hostd restarts [INFERENCE from `register_new`]. | Key by trigger id plus config hash; drop watchers for removed ids. |
| D7 | Legacy routines and triggers run two schedulers. | `crates/atlas-host/src/crew/schedule.rs:90` (`tick`, routines) and `crates/atlas-host/src/crew/triggers/mod.rs:176` (`tick_schedule`, triggers). | Two overlapping code paths for cron; a new missed-run or budget rule must be applied twice. | Migrate routines to triggers. |
| D8 | `destructive()` gates only voice approvals. | One caller outside tests: `crates/atlas-host/src/crew/voice_card.rs:81`. `classify` (`approval.rs:69-83`) never calls it. | In `edits` and `bypass`, `rm -rf`, `git push --force`, `DROP TABLE` run without a prompt. | Call it from `classify`. |
| D9 | codex and omp report no `cost_usd`; terminal agents report no usage. | `crates/atlas-host/src/crew/engine/codex.rs:129` and `engine/omp.rs:254` set `cost_usd: None`; only claude (`claude.rs:78`) and opencode (`opencode.rs:85`) set it. | USD budgets cannot be enforced for those engines. | Token or wall-time budgets; read cost from omp output if it prints it. |
| D10 | Desktop crew tests fail without `bun install` of `@inlang/paraglide-js`. | In `apps/desktop`, `bun test src/lib/crew` fails all 67 files with `Cannot find module '@inlang/paraglide-js'` from `apps/desktop/scripts/paraglide.ts`, imported by `test/setup.ts:10`; `apps/desktop/node_modules/@inlang` is absent in a fresh worktree; pinned at `apps/desktop/package.json:19`. | A clean worktree cannot run the suite. The suite also needs `crates/**` and `apps/mobile/test/fixtures/**`. | Document the pre-step; keep reading the shared fixtures on purpose. |
| D11 | `agent_step` is broadcast but read by no client. | `crates/atlas-host/src/agents/mod.rs:355-363` emits it; `server.rs:700-712` sends it to every desktop connection; `grep agent_step apps/` is empty; not in `push_scope`. | The only per-tool-call event is unused by the UI, and phones never see it. | Add it to `push_scope` after redaction and carry `task_id`. |
| D12 | omp crewmates get no atlas MCP tools. | `crates/atlas-host/src/crew/engine/omp.rs` never uses `McpServer`; `docs/crew.md` says omp reaches hostd through the approval extension. | An omp crewmate cannot call `ask_user`, `capture`, `ui_gate`, `set_profile`; a UI task cannot pass the two-capture gate [INFERENCE]. | Add an omp renderer in `engine/launch.rs`. |
| D13 | Some trigger sources can flood notifications. | Telegram gets every done and failed (`runner/outcome.rs:211-234`); no cooldown on crewmate pushes (`agents/push.rs` cooldown is for plain panes). | A 60 s interval trigger sends one message per run. | Single `notify` policy. |
| D14 | Test names overstate coverage in two places. | `interval_fires_after_duration` re-implements the match inline instead of calling `tick_schedule` (audit-organization-work); the `max_agents` name filter matches no test (rc=4); `egress` filter matches `uigate` tests, not network egress. | Pass counts for these names do not prove the tick or network drops. | Rename or point tests at the real tick. |
| D15 | A failed or parked task may not push or notify. | `outcome.rs:240-262` pushes only `done`; `park_task` (`runner/action.rs:114-120`) and `handle_ask` (`hook_server.rs:513-567`) call neither push nor Telegram [INFERENCE]. | A task waiting on a card can sit unseen if the owner is not looking at the app. | Notify on `waiting_action` and `failed`. |

Not defects, but known and documented gaps that the RFC work must plan around: the capture browser has no OS sandbox (`docs/crew.md:618`), the push relay is not deployed (`docs/crew.md:524`), and port 47631 can be squatted (`docs/crew.md` S-A).

## 18. Readiness of the four target repos

Source: `research/dogfood-targets.md`, a static read of each repo. No service was started and no test was run there. Ratings are the survey's judgment.

| | Atlas | ac | zelvo | wi |
|---|---|---|---|---|
| Product | Dev platform and Atlas Crew | Operating system of a label printer (ERP, stock, production, store) | CRM for external salespeople | WhatsApp API platform |
| Stack | Rust workspace, Tauri and Svelte, Flutter, Astro, Go relay | Bun, Turborepo, Elysia and oRPC, React 19, Drizzle, BullMQ, Flutter, Go, Rust | Bun, Turborepo, SvelteKit 2, Elysia and oRPC, Drizzle, BullMQ | Bun, Elysia, SvelteKit, Drizzle, Go engines, NATS |
| Gate | `bun run check` (heavy, 3 OS in CI) | `bun run check` plus serial `bun run test` | `check-types`, `test`, `check` | `bun run check`, `test`, `test:integration`, `test:engine` |
| CI trigger | push and PR | `workflow_dispatch` only | push and PR to main | push and PR to main |
| Deploy | `bun run release alpha` (maintainer), `site:deploy` | push to `main` is production | `bun run deploy`, needs `sim` | `bun run deploy` (Go CLI) |
| Dev stack | `lab`, `site`; `desktop` is the user's screen | 8 services; agents may not start it | `redis`, `server`, `web`, `worker`, shared Neon dev DB | `postgres`, `nats`, `server`, `web`, engines |
| Hard stops for an agent | never merge, never push `main`, no release | no commit or push to `main` without a request, no deploy, no prod writes, no `bun install` | no deploy without request, `db:reset` human only | deploy and infra human only, no real WhatsApp or Asaas |
| **Readiness** | **High** for branch, PR and tests; merge and release blocked by design | **Low** | **Medium** | **Medium-high** for issue work; low for WhatsApp and Asaas work |
| Main blockers today | heavy gate needs a warm Rust cache and the shared queue; Windows checks need zig; crew servers assume Linux with systemd | agents may not run the dev stack, so tests that need Postgres and Redis cannot run; push to `main` deploys | dev DB is shared Neon, only Redis is in compose, e2e needs a running stack | no paired WhatsApp session in reach; Go and `air` needed; Asaas sandbox returns 400 on Pix QR |

### 18.1 Common requirements (each needed by at least two repos)

Status is what the survey observed, not what it tried.

1. A repo-scoped working copy per crewmate with a clean, pushed base; no destructive git (ac and Atlas ban stash, reset, clean).
2. Branch and PR only. Merge and deploy are human gates in all four. Crew has no merge gate today; it can open PRs.
3. A gate command and an explicit definition of done per repo, with evidence on the PR or issue.
4. Dev stack owned by a human or started on explicit request. Crew does not run `atlas dev` on the crew server today, and ac forbids agents from starting it.
5. A shared heavy-job queue across crewmates and the human (`atlas run --heavy`).
6. Per-crewmate dev secrets, never the maintainer's and never production. Crew stores per-crewmate `secrets.json` and the export omits it by default.
7. Read-only production access. Crew approval modes do not block network or SSH commands: a `bypass` crewmate holding an SSH key can write to production. A deny rule for the production hosts is needed.
8. Human approval for external effects (Tiny, Kommo, ClickUp, Asaas, WhatsApp, e-mail, phone calls). Crew approval covers tool calls, not what a script sends to an API.
9. Destructive database actions are human only (`db:refresh-local`, `db:push` with data loss, `db:reset`, non-expand migrations). Deny rules are needed per repo.
10. Dependency installs from the lockfile only (`bun install --frozen-lockfile`); no lockfile edits.
11. Output language per repo: pt-BR for product text and reports, English for code; use each repo's i18n path.
12. Evidence in the artifact: CHANGELOG line, release note, resolution comment with SHA, UI captures. Crew `ui` tasks already carry proof for Atlas.
13. A per-repo toolchain check that refuses a task early (Flutter for ac, Go and `air` for wi, Docker for zelvo, a desktop agent instance for Atlas UI).
14. A stopped dev stack is a blocker to report, not a thing to fix by starting it (ac `dev:status` exit 3).

Mapping to this baseline: items 7, 8 and 9 need the policy engine, the destructive gate (D8) and per-crewmate egress; item 4 and 13 need capability grants; item 2 needs an approval gate for `gh pr merge` and deploy commands. Open questions for the maintainer are listed in `research/dogfood-targets.md` section 7.
