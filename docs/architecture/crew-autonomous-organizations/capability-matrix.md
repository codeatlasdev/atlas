# Atlas Crew capability matrix

Baseline `main` `72b47fc3`, 2026-11-20.

One table of every capability the RFC names that the three code audits classified. Companion to `current-state.md`. Sources are the files in `research/`: `audit-runtime-authority.md`, `audit-organization-work.md`, `audit-surfaces-ax.md`. Where two audits classified the same capability the row is merged, keeps the stricter status and cites both. Where one row covers several closely related capabilities, the primary capability sets the status and the gap column names the rest.

Status values: **Implemented and verified** (code exists and a test run by an audit passes through it), **Implemented, incomplete** (real code with a stated hole), **Prototype**, **Planned** (written down in a doc or catalog, no code), **Missing**, **Unknown** (the audits could not decide; the evidence column says why). Evidence marked `[INFERENCE]` was derived, not observed. Test names and pass counts are from the runs listed in the audits.

## Summary

| Layer | Verified | Incomplete | Prototype | Planned | Missing | Unknown | Total |
|---|---|---|---|---|---|---|---|
| Host authority and security | 8 | 9 | 0 | 1 | 7 | 0 | 25 |
| Durable execution | 1 | 4 | 0 | 0 | 5 | 0 | 10 |
| Agent runtime and engines | 3 | 4 | 1 | 0 | 1 | 0 | 9 |
| Organization model | 1 | 4 | 0 | 0 | 4 | 0 | 9 |
| Communication and delegation | 2 | 5 | 0 | 0 | 6 | 0 | 13 |
| Autonomous work loop and triggers | 3 | 6 | 1 | 0 | 3 | 0 | 13 |
| Goals and budgets | 0 | 0 | 0 | 0 | 2 | 0 | 2 |
| Memory | 0 | 1 | 0 | 0 | 4 | 0 | 5 |
| Reporting and notifications | 3 | 1 | 1 | 0 | 4 | 0 | 9 |
| Organization UX | 3 | 3 | 0 | 0 | 3 | 1 | 10 |
| Agent experience and MCP tools | 3 | 2 | 0 | 0 | 3 | 0 | 8 |
| Capabilities and integrations | 3 | 0 | 0 | 2 | 3 | 0 | 8 |
| Interop | 2 | 0 | 0 | 0 | 3 | 0 | 5 |
| Observability, audit, evaluation | 3 | 5 | 0 | 0 | 5 | 1 | 14 |
| **All layers** | **35** | **44** | **3** | **3** | **53** | **2** | **140** |

`Verified` is Implemented and verified; `Incomplete` is Implemented, incomplete. Each row has an ID (layer code plus number) that the scenario section uses.

## Matrix

| Layer | Capability (RFC term) | Status | Evidence | Gap / smallest evolution |
|---|---|---|---|---|
| Host authority and security | `HA01` Isolation per Linux user (fail closed without the helper) | Implemented and verified | Root helpers `atlas-crew-spawn` and `atlas-crew-provision` (`crates/atlas-cli/src/commands/crew/isolation.rs:71-152`); `SO_PEERCRED` and request checks in `crates/atlas-host/src/crew/spawn/validate.rs`. Tests: `spawn` 107, `isolation` 30, `provision` 23 passed (audit-runtime-authority). Fail closed: isolation installed and socket missing makes every task fail (`runner/mod.rs` `run_one`, `isolation_fault`); test `a_server_with_isolation_installed_and_no_helper_refuses_tasks_and_starts_no_agent` passed. | Linux with helper installed only. On a Mac host crewmates share the crew user [INFERENCE]. Root tests are `#[ignore]` and only run in `crew-e2e` CI, which no audit ran. |
| Host authority and security | `HA02` Resource limits: memory and process count (cgroup) | Implemented, incomplete | Leaf `cm-<slug>` with `memory.max`, `memory.swap.max=0`, `pids.max=4096` (`crates/atlas-host/src/crew/spawn/cgroup.rs:306-314`); cap in `crates/atlas-host/src/crew/memory_cap.rs:48,54`. `cgroup` 13 passed (arithmetic and parsing only). | Kernel enforcement tests (`cgroup.rs:557,598,709`) are `#[ignore]`. Without a delegated cgroup v2 the helper spawns uncapped. Capture, locate and `ui_gate` run outside any cap. |
| Host authority and security | `HA03` Resource limits: CPU, IO, per-run wall time | Missing | Only `memory.max` and `pids.max` exist. The 2 h `TASK_TIMEOUT` is global (`crates/atlas-host/src/crew/runner/mod.rs:85`). | Add `cpu.max` to the existing leaf; make the timeout a per-crewmate or per-trigger setting. |
| Host authority and security | `HA04` Egress controls (network) | Implemented, incomplete | nftables table `inet atlas_crew`, one chain per crewmate user (`crates/atlas-host/src/crew/spawn/provision.rs:243-331`); replay at boot by `atlas-crew-egress.service`. Test `the_ruleset_replaces_its_own_table_and_gives_each_user_its_own_chain` passed; `provision` 23 passed. | Tests check ruleset text only, no packet drop was observed. Filtering is by IP and port: any host on 22, 80, 443 is reachable. |
| Host authority and security | `HA05` Per-crewmate or per-domain egress allowlist | Missing | `System::egress` always applies `Policy::default()` (`crates/atlas-host/src/crew/spawn/provision.rs:409-415`); `docs/crew.md:609` says allowlists have no setting. | `egress_ruleset` already takes `(user, Policy)`: read a root-owned per-crewmate policy file. Domain filtering needs an HTTP(S) proxy because nft is IP only. |
| Host authority and security | `HA06` Host authority decision path for tool calls | Implemented and verified | Every call of claude and omp (also terminal) ends in `approval::decide` (`crates/atlas-host/src/crew/approval.rs:665`): rails, mode, task grants, else a person. Tests: `approval` 36, `permissions` 27 passed. | Holds for claude and omp only. Network calls made by a shell inside an allowed command are not policy-evaluated beyond rails. |
| Host authority and security | `HA07` Engines that cannot ask for approval (codex, opencode) | Implemented, incomplete | `unsupported_engine` only refuses non-bypass modes (`crates/atlas-host/src/crew/approval.rs:921`); `crewmate_create` forces `bypass` (`crates/atlas-host/src/crew/handlers/crewmates.rs:47-48`). | These crewmates run unattended. Restrict autonomy-bearing crewmates to claude and omp, or add a hostd shell wrapper. |
| Host authority and security | `HA08` Permission modes and protected-path rails | Implemented and verified | `Mode {ask, edits, bypass}` (`crates/atlas-host/src/crew/store/mode.rs`); rails in `approval.rs:85-110,347-395`; `allow_bypass` and `revoke_bypass` (`approval.rs:992-1004`). Tests `the_rails_hold_in_every_mode`, `near_misses_are_not_protected` passed. | Three coarse labels. In `bypass` every non-rail call runs without a prompt. |
| Host authority and security | `HA09` Policy hierarchy, policy objects and autonomy profiles (RFC 15.1, 15.2) | Missing | Authority is mode, rails and task grants. No policy files. `classify` is the single hook (`crates/atlas-host/src/crew/approval.rs:69-83`). | Add `Policy::evaluate` ahead of the mode check, loaded from files hostd owns and the agent cannot write. The five RFC autonomy profiles (advisory to fully autonomous, RFC 15.2) are Missing: only `Mode` (`store/mode.rs`) exists and it stays the fallback. |
| Host authority and security | `HA10` Capability-based authority (RFC 15.4) | Missing | No per-crewmate capability list; `approval::decide` sees only the tool name and input (audit-runtime-authority, gap 4; audit-surfaces-ax, capability registry). | A `capabilities` list on `Crewmate` that `decide` and `tools_list()` both check. |
| Host authority and security | `HA11` Revocation (per crewmate, secret, grant) | Missing | Only `revoke_bypass` (bulk to Ask, `approval.rs:992-1004`) and the global kill switch. `Crewmate` has no `disabled` field (`crates/atlas-host/src/crew/store/mod.rs:263-296`). | Per-crewmate disable, per-secret delete, per-grant revoke; reuse the `set_paused` structure. |
| Host authority and security | `HA12` Destructive-action classification | Implemented and verified | `destructive()` and `destructive_command()` (`crates/atlas-host/src/crew/approval.rs:112-226`). Tests `destructive_calls_are_the_ones_that_destroy_data` and 12 more passed. Heuristic, word based. | No coverage for network-destructive calls (API DELETE, `gh repo delete`, payment APIs). |
| Host authority and security | `HA13` Destructive-action gate outside voice approvals | Missing | `destructive()` has one caller outside tests: `crates/atlas-host/src/crew/voice_card.rs:81`. `classify` never calls it (`approval.rs:69-83`), so `rm -rf` runs in `bypass` without a prompt. | Call `destructive()` from `classify` for `edits` and `bypass` modes; needed before autonomous crews touch real repos. |
| Host authority and security | `HA14` Approval persistence across restart | Implemented, incomplete | Pending approvals and grants are in memory by design (`crates/atlas-host/src/crew/approval.rs:476-518`). `waiting_action` cards persist (see DE row). | No stored approval request, only audit lines `approval_asked` and `approval_decision`. RFC 15.5 implies a durable record. |
| Host authority and security | `HA15` Approval binding to the exact action and material-change invalidation (RFC 15.5) | Implemented, incomplete | Held prompt is a one-shot `req` bound to `(pane, req)` (`crates/atlas-host/src/agents/mod.rs:432-470,511-522`). `scope: task` grants key by tool and exact command (`approval.rs:261-272`). `approval` suite passed. | No canonical hash, policy snapshot, agent version, or recipient and amount fields. The `edit` grant covers any in-tree edit for the whole task. Material-change invalidation and plan approval are Missing: the held call cannot change and there is no approve-now-execute-later model. |
| Host authority and security | `HA16` Approval expiry | Implemented, incomplete | Wait window `CREWMATE_DECISION_WAIT` 30 min then deny (`crates/atlas-host/src/agents/hook_server.rs:37`, `approval.rs:833-846`). Tests `an_approval_nobody_answers_fails_with_the_extensions_message_not_omps`, `an_approval_answered_after_20_minutes_still_lets_the_tool_run` passed. | One fixed window, no per-risk expiry, no TTL on task grants. `ask_user` and `waiting_action` cards wait up to 2 h. |
| Host authority and security | `HA17` Secrets storage, delivery and leasing | Implemented, incomplete | Per-crewmate `secrets.json`, mode 0600 (`crates/atlas-host/src/crew/store/mod.rs:964-1010`); env delivery to that crewmate's agent (`runner/conversation.rs:196-203`, `engine/launch.rs:168-177`). `secret` 30 passed. | Plaintext JSON, env delivery lets an injected agent print values. No lease, scope, TTL. Terminal agents get env from `agent-env` files, not `secret_env` [INFERENCE]. No broker or lease: only the GitHub App token path mints per operation (`credential_reply`, `crates/atlas-host/src/agents/hook_server.rs:355-358`); `secret_env` reads leave no audit line. |
| Host authority and security | `HA18` Browser sandbox (capture, locate, ui_gate) | Implemented, incomplete | Playwright runs as the crew user with `chromiumSandbox` off (`crates/atlas-host/src/crew/capture/runner.mjs:263-266`); forward proxy guard `capture/guard.rs`. `capture` 63 passed. Known gap N4 (`docs/crew.md:618`). | A browser exploit becomes the crew user and reaches every crewmate. No concurrency limit, no cgroup. |
| Host authority and security | `HA19` Capture browser runs as the crewmate or a dedicated user | Planned | `docs/crew.md:618` ('Plan: run the runner as the crewmate ... or as a dedicated capture user'). `Exec::isolated` exists in `crates/atlas-host/src/crew/engine/launch.rs`. | Needs an nft exception for the guard proxy port. Blocks autonomous browsing on a shared server. |
| Host authority and security | `HA20` Untrusted input labeling and prompt-injection handling | Implemented, incomplete | `no_auto` flag, `untrusted_event()` preamble, `author_allowed` (`crates/atlas-host/src/crew/triggers/mod.rs:135-170`); `runner/prompt.rs:101`; guards `runner/guard.rs`. Tests `untrusted` 10, `guard` 50 passed. | Textual labeling; the real control is the `no_auto` rail. No taint on derived data and `no_auto` is not carried from parent to child task (no parent exists). |
| Host authority and security | `HA21` Hook server and agent route authentication | Implemented and verified | Per-crewmate token, `authorize` (`crates/atlas-host/src/agents/hook_auth.rs:49-77`), `AGENT_ROUTES` (7), constant-time compare (`hook_server.rs:257-272`), 1 MiB body cap. `hook_auth` 7, `hook` 74 passed. | Port 47631 can be squatted by a detached crewmate process (`docs/crew.md` S-A, not fixed). Generic webhook token is not constant time (see Defects). |
| Host authority and security | `HA22` Kill switch (pause and resume) | Implemented and verified | `crew_pause` and `crew_resume` (`crates/atlas-host/src/crew/hub.rs:114-137`), persisted in `crew.json`, corrupt file reads as paused (`store/mod.rs:1071-1086`). `pause` 21 passed (incl. `paused_survives_a_restart_of_hostd`). | One global switch. Pause cancels running tasks (state `Cancelled`, error `paused`), it does not suspend. Held approvals stay open until their wait ends [INFERENCE]. |
| Host authority and security | `HA23` Admission control: max_agents and per-crewmate FIFO | Implemented and verified | `AgentSlots` (`crates/atlas-host/src/crew/runner/slots.rs`), default 4. Test `a_limit_of_two_runs_at_most_two_agents_and_the_rest_show_server_busy` passed. | No queue depth limit, no priority, no per-crewmate or per-trigger quota. |
| Host authority and security | `HA24` Rate limits on task creation and delegation | Missing | No limiter found in `crates/atlas-host/src/crew/` (audit-runtime-authority, concurrency row; audit-organization-work, bounded proactive work). | Add before any agent-originated task route exists. |
| Host authority and security | `HA25` Install, update and rollback of hostd and helpers | Implemented and verified | `atlas crew install\|update\|update --rollback` (`crates/atlas-cli/src/commands/crew/update.rs:97-196`, `binary.rs:289-330`). `atlas-cli test(crew)`: 177 run, 176 passed, 1 failed (environmental: `browsers.rs:1035`, local Playwright cache). | Rollback is the binary only, no data snapshot. `update` restarts the `atlas-crew` service; helper sockets need `install` again [INFERENCE]. |
| Durable execution | `DE01` Task and run record with states | Implemented, incomplete | `Task` and `TaskState {Queued, Running, Done, Failed, Cancelled, WaitingAction}` (`crates/atlas-host/src/crew/store/mod.rs:340-447`); append-only `tasks.jsonl` (`store/tasklog.rs`). | A task is the only run unit. Session is per crewmate, not per run. No objective, input refs, plan summary, org or parent. |
| Durable execution | `DE02` Explicit run states (waiting approval, recovering, outcome uncertain, paused) | Missing | Waiting approval is derived live (`handlers/crewmates.rs:82-94`); paused is `Cancelled` with error `paused` (`hub.rs:129`); recovering is `Queued` plus `resumed:true`. | Add an optional `run` object on `Task` rather than new `TaskState` values: clients decode `state` as a closed set. |
| Durable execution | `DE03` Parent and child runs, run lineage | Missing | `Task.chain` lists crewmate ids only (`store/mod.rs:441-446`); no parent task id. | `parent`, `delegated_by`, `depth` optional fields on `Task`. |
| Durable execution | `DE04` Durable runs and checkpoints | Implemented, incomplete | State, steps and usage persisted on every change (`crates/atlas-host/src/crew/runner/outcome.rs:263-298`); session saved per turn (`conversation.rs:250-258`). `restart` 26, `recover` 6, `queued_task_survives_restart` passed. | Only checkpoint is end of turn. Terminal agents rerun from the start (`runner/recover.rs:6-9`). |
| Durable execution | `DE05` Recovery after restart (resume cap, durable human wait) | Implemented and verified | `recover(hub)` (`crates/atlas-host/src/crew/runner/recover.rs:19,52-117`), `MAX_RESTARTS=2`. Tests `a_task_is_resumed_until_the_cap_then_fails`, `a_task_interrupted_three_times_fails_with_a_clear_error` and 5 more passed. Durable human wait: `waiting_action` cards survive (`runner/recover.rs:42`); test `a_waiting_action_task_can_be_answered_after_a_restart` passed. | `restarts` counts any interruption alike. No hostd-side check of what already happened; the resume prompt asks the agent to check. |
| Durable execution | `DE06` Idempotency of external actions | Implemented, incomplete | GitHub comment reuse (`runner/reporting.rs:41-70`), GitHub seen file written before firing (`triggers/github.rs:4-11`), webhook delivery ids (`github/app/webhook.rs`; test `a_delivery_id_is_taken_once_and_remembered_across_a_restart` passed). | No generic idempotency key or ledger for agent tool calls. `git push`, PRs and messages may repeat after a resume. |
| Durable execution | `DE07` Tool-call journal (intent, result, class) | Missing | `tool_call` audit lines record intent only; `Tape` keeps outputs in memory and drops them at task end (`crates/atlas-host/src/crew/runner/tape.rs`). | Per-task `tool_calls.jsonl` fed from the same hook events the tape reads (audit-runtime-authority, gap 2). |
| Durable execution | `DE08` Uncertain-outcome handling and reconciliation | Missing | No `OutcomeUncertain` state; grep for uncertain and reconcile in `crates/atlas-host/src/crew/` finds nothing (audit-runtime-authority). | Mark `started` and not `ended` external calls as uncertain on recovery; needs the journal first. |
| Durable execution | `DE09` Artifacts and attachments | Implemented, incomplete | `Task.attachments`, `ui` captures, `Task.origin` (`crates/atlas-host/src/crew/store/mod.rs:359-447`). `payloads` 6, `mobile_payloads` 3 passed. | No artifact registry or run-to-artifact index. |
| Durable execution | `DE10` Event bus (internal publish and subscribe) | Missing | hostd broadcasts push events to connected clients (`crates/atlas-host/src/crew/events.rs`), but no inbox, subscription or replay exists; events are not sequenced (`protocol.rs:15-16`). | Start with the `fire` gate as a typed event inbox (see AW rows). |
| Agent runtime and engines | `AR01` Agent identity and role | Implemented, incomplete | `Crewmate` (`crates/atlas-host/src/crew/store/mod.rs:263-296`): id, name, role (<=2000 chars), engine, mode, project; Linux user `atlas-cm-<12 hex>` (`spawn/validate.rs:98-106`); hook token (`store/hooktoken.rs:15`). `hook_auth` 7, `isolation` 30 passed. Merged from the runtime, organization and surfaces audits. | Role is free text. The agent can rewrite it with `set_profile` (`agents/hook_server.rs:570-596`). No definition version, no org binding. |
| Agent runtime and engines | `AR02` Agent definition version and per-run identity | Missing | A run carries only the crewmate id and pane key `crew:<crewmate>:<task>` (`crates/atlas-host/src/crew/pane.rs`). | Optional `agent_version` stamped on `Task`; no new identity system. |
| Agent runtime and engines | `AR03` Engine drivers and sign-in status (claude, codex, opencode, omp) | Implemented and verified | `crates/atlas-host/src/crew/engine/{claude,codex,opencode,omp}.rs`; batch of 246 tests incl. `engines`, `omp_approval`, `engine_login` passed (audit-surfaces-ax). Sign-in status: `crew_engine_status` (`crates/atlas-host/src/protocol.rs:913-922`, `engine_login.rs`), `engine_login` tests passed. | Engines run one-shot: no token deltas. omp is the only streaming protocol and is read until `prompt_result`. `crew_engine_auth` push has no consumer and is not phone-scoped (see Defects). |
| Agent runtime and engines | `AR04` Cost reporting by engine | Implemented, incomplete | claude and opencode set `cost_usd` (`crates/atlas-host/src/crew/engine/claude.rs:78`, `opencode.rs:85`); codex and omp set `None` (`codex.rs:129`, `omp.rs:254`). | Budgets cannot be enforced in USD for codex and omp; use tokens or wall time. |
| Agent runtime and engines | `AR05` Engine quota and rate-limit information | Prototype | `engine_limits` is a stub returning a fixed message (`crates/atlas-host/src/crew/handlers/settings.rs:295-300`). | No engine CLI exposes machine-readable quota. |
| Agent runtime and engines | `AR06` Terminal-agent crewmates (pty path) | Implemented, incomplete | `runner/pty.rs`; `crew::tests::tasks::crewmate_tasks_steps` passed. | No usage records, no checkpoint (reruns from start), no `secret_env` [INFERENCE]. |
| Agent runtime and engines | `AR07` Session continuity (engine session resume) | Implemented and verified | `CrewmateSession` (`crates/atlas-host/src/crew/store/mod.rs:161-170`); `claude --resume`, `omp --resume`. Test `a_running_conversational_task_resumes_its_session_after_a_restart` passed. | One session per crewmate: context can bleed between tasks. |
| Agent runtime and engines | `AR08` Guards on finished work | Implemented and verified | `crates/atlas-host/src/crew/runner/guard.rs`, `gate.rs`. `guard` 50 passed; `crew::tests::guards`, `prove_ui` passed. | Guards check weakened tests, strays and overclaims, not goals. |
| Agent runtime and engines | `AR09` Agent lifecycle (draft, configured, simulated, paused, degraded, archived) | Implemented, incomplete | Runtime state `idle`, `working`, `needs_you` computed on read (`crates/atlas-host/src/crew/handlers/crewmates.rs:82-93`); create, update, delete (`crewmates.rs:253,321,406`). `crewmate_state` 4 passed. | No per-crewmate pause, disable or archive; delete is destructive. Add `Crewmate.status` (`active\|paused\|archived`). |
| Organization model | `OM01` Organization as a first-class entity | Missing | No struct above the crew. `CrewSettings` has no name, mission, goals or members (`crates/atlas-host/src/crew/store/mod.rs:496-527`). | Optional `organization {name, mission, description}` in `crew.json`; one crew server is one organization. |
| Organization model | `OM02` Crew-level settings and policy set | Implemented, incomplete | `CrewSettings`: `paused`, `max_agents`, `agent_memory_max`, `default_approval`, `allow_bypass`, `voice`, `public_url` (`store/mod.rs:497-527`); `pause` 12 passed (organization audit run). | Machine and permission-mode policy only. No budgets, autonomy profile or quiet hours. |
| Organization model | `OM03` Departments | Missing | No `department` token in `crates/atlas-host/src/crew` (grep, audit-organization-work). | `Crewmate.department: Option<String>` first; a separate entity only when it needs its own policy. |
| Organization model | `OM04` Responsibilities and structured roles (inputs, outputs, authority, escalation) | Missing | Prompt carries `You are {name}. {role}` only (`crates/atlas-host/src/crew/runner/prompt.rs:204-249`). | Optional responsibilities fields rendered next to `role` in `task_prompt`. |
| Organization model | `OM05` Agent registry (queryable by role or capability) | Implemented, incomplete | `list_crewmates` (`crates/atlas-host/src/crew/store/mod.rs:741`), `crewmate_list` payload (`handlers/crewmates.rs:101-215`). | A listing: no capabilities, load history or role query. |
| Organization model | `OM06` Several crew servers under one owner | Implemented, incomplete | Desktop registry `crew-servers.json` and relay `crew_call` (`crates/atlas-host/src/crew/relay/mod.rs:70-90`). `relay` 11 passed. | hostd cannot talk to another hostd; cross-server organization has no home. |
| Organization model | `OM07` Export and import of a crew (archive) | Implemented and verified | `crates/atlas-host/src/crew/store/archive.rs` (`FORMAT_VERSION = 1`, line 31). 22 `archive_tests` passed. | New artifacts (goals, knowledge) need a version bump or an explicit exclusion. |
| Organization model | `OM08` Organization Architect: blueprint, review, simulation and authorized activation | Missing | grep for blueprint and organization architect finds nothing in `crates/atlas-host/src/crew`, desktop and mobile crew code, or `docs/crew.md`. | RFC section 7 is not started. Review, edit and activation with explicit authorization have no object or step either; they depend on the policy engine. |
| Organization model | `OM09` Guided setup of a single crewmate (setup chat) | Implemented, incomplete | `apps/desktop/src/lib/crew/chat/setup.svelte.ts`, `crewSetup.ts`, `Crewmate.setup_step`; `crewSetup.test.ts` is in the 833 desktop tests that passed in a prepared copy. | One agent at a time, no company discovery, no unknowns list. |
| Communication and delegation | `CD01` Agent-to-agent messages, requests, broadcast and envelope (RFC 9.1, 9.2) | Missing | MCP server has 7 tools, none addresses another crewmate (`crates/atlas-host/src/crew/mcp.rs:166-174,345`); `AGENT_ROUTES` (`agents/hook_auth.rs:16-25`). | A delegation is a task with a durable record, so a separate mailbox is not needed for the first cut. No envelope or durable mailbox either (`messages.jsonl` per crewmate only when non-task messages are needed). |
| Communication and delegation | `CD02` Delegation by an agent (parent run, recipient, scope, budget, deadline) | Missing | No hook route or MCP tool lets an agent create a task. Nearest: human `crewmate_task` (`handlers/tasks.rs:25-72`) and the `add_trigger` card (`runner/action.rs:204-228`). | New `AGENT_ROUTES` entry `delegate` that builds a normal `Task` through `fire`; add `parent`, `delegated_by`, `depth`. Accept, reject and clarify have no recipient-side state; reuse `TaskAction` cards or a `WaitingChild` state. |
| Communication and delegation | `CD03` Context transfer with permission check (RFC 9.3) | Missing | No transfer primitive; isolation blocks cross-reads at OS level (`crates/atlas-host/src/crew/spawn/layout.rs:17`). | hostd-mediated copy; output wrapped by `untrusted_event`. |
| Communication and delegation | `CD04` Handoff by chain trigger (after-crewmate) | Implemented and verified | `TriggerKind::Crewmate{from_crewmate,on}` (`crates/atlas-host/src/crew/store/mod.rs:104-112`), `on_task_finished` (`triggers/mod.rs:250-340`). Tests `crewmate_chain_trigger_fires_on_finish`, `hygiene` 5 passed. | Passes one text line, not the result or task id. `cancelled` does not fire. |
| Communication and delegation | `CD05` Handoff that carries the result and artifacts | Missing | Event text is `Crewmate {name} done\|failed` (`triggers/mod.rs:~270`); `render_prompt` knows only `{{event}}` (`:128`). | `{{result}}` and `{{task_id}}` placeholders, wrapped as untrusted. |
| Communication and delegation | `CD06` Cycle protection for chained work | Implemented and verified | `Task.chain` guard with `chain_refused` audit; `hygiene` tests show a mutual cycle stops after one round and a long A to B to C chain runs (`crates/atlas-host/src/crew/tests/hygiene.rs:53-141`). | No depth or fan-out limit across many distinct crewmates [INFERENCE]. |
| Communication and delegation | `CD07` Delegation depth, fan-out and spam bounds | Missing | No counter in `on_task_finished`; queue is an unbounded `VecDeque` (`crates/atlas-host/src/crew/runner/mod.rs:~97`) [INFERENCE]. | Max depth, per-crewmate max queue, audit `task_dropped`. |
| Communication and delegation | `CD08` Routing: explicit recipient | Implemented, incomplete | Sender names the crewmate id; Telegram `/task <name>` (`crates/atlas-host/src/crew/telegram.rs:~664-700`); GitHub events go to every matching crewmate (`triggers/github_hook.rs`). | Two crewmates matching one repo both start; no assignment arbitration. |
| Communication and delegation | `CD09` Routing by role, capability, department or load | Missing | Load data exists (`queue_len`, `running_task`, `AgentSlots`) and nothing consumes it (audit-organization-work). | hostd-side `resolve_recipient` over `list_crewmates` plus queue length; needs role and department fields. |
| Communication and delegation | `CD10` Work queues and priorities | Implemented, incomplete | Per-crewmate FIFO, one running; server cap `max_agents` (`crates/atlas-host/src/crew/runner/mod.rs:77-110,320`; `runner/slots.rs`). | No priority, deadline, dependency or join. |
| Communication and delegation | `CD11` Ownership, durable state and duplicate-assignment avoidance | Implemented, incomplete | Task fields `id`, `source`, `trigger_id`, `chain`, `origin`, `restarts` (`store/mod.rs:358-447`); `restart` tests passed. | Ownership is implicit in the crewmate's log. No cancellation propagation. `crewmate_task` has no idempotency key: two humans or webhooks can enqueue identical work. |
| Communication and delegation | `CD12` Escalation to a human | Implemented, incomplete | MCP `ask_user` (`crates/atlas-host/src/agents/hook_server.rs:513-567`), `TaskAction` cards, `crewmate_state` `needs_you` (`handlers/crewmates.rs:84-93`). | No category, SLA or option set beyond `choose`. Escalation to a higher-authority agent is Missing. |
| Communication and delegation | `CD13` Delegation graph visible to a human | Implemented, incomplete | Data exists (`Task.chain`, `Crewmate` trigger, `Task.trigger_id`) but no client reads it: grep of `apps/desktop/src/lib` and `apps/mobile/lib` finds nothing. | Needs `parent` on `Task` first, then a graph or tree view. |
| Autonomous work loop and triggers | `AW01` Trigger: schedule (cron, time zone) and interval | Implemented and verified | `TriggerKind::Schedule`, `tick_schedule` every 30 s (`crates/atlas-host/src/crew/triggers/mod.rs:176-246`), `schedule.rs:17-70`. Tests `time_contract` 10, `old_crons` 1, `automations` 7; `routine_every_minute_fires_within_90s` ran for real. Interval (60 s minimum): `triggers/mod.rs:~190-200`; tests `interval_rejects_under_60s`, `interval_fires_after_duration`. | Missed-run behavior is implicit (see below). `interval_fires_after_duration` re-implements the match inline. |
| Autonomous work loop and triggers | `AW02` Single scheduler (legacy routines retired) | Implemented, incomplete | Legacy `Routine` ticked by `schedule::tick` (`crates/atlas-host/src/crew/schedule.rs:90-148`) while triggers use `tick_schedule`: two schedulers run (see Defects). | Migrate routines to triggers or apply every new policy to both. |
| Autonomous work loop and triggers | `AW03` Trigger: file change | Implemented, incomplete | `triggers/file_watch.rs`: `notify` watcher, 5 s debounce, 30 s poll fallback. 4 unit tests passed. | `registered` set is never pruned: edit or disable needs a hostd restart (`file_watch.rs:34-46`, see Defects). |
| Autonomous work loop and triggers | `AW04` Trigger: service log | Prototype | `triggers/service.rs`: polls `.logs/<svc>.log` every 30 s, substring match (`:116-130`). 2 tests (tail reading only). | Heuristic, state in memory, no test of the fire edge. Helps only projects running `atlas dev`. |
| Autonomous work loop and triggers | `AW05` Trigger: generic webhook | Implemented, incomplete | `crates/atlas-host/src/crew/triggers/webhook.rs:26-68`; tests `webhook_wrong_token_rejected`, `webhook_correct_token_enqueues_task` passed. Merged: auth Implemented and verified, dedupe incomplete. | Token compared with `==` (lines 42-44). Payload cut to 200 chars, no delivery id, no rate limit, no HMAC. |
| Autonomous work loop and triggers | `AW06` Trigger: GitHub (8 event kinds) | Implemented and verified | `triggers/github_events.rs:~75-270`, poll 120 s (`github.rs:23`), signed webhook (`github_hook.rs:55-107`). `github_in` 20, `github_out` 12, `github_webhook` 10, `github_hook_events` 6, `github_e2e` 5, `untrusted` 9 passed. | Needs a public HTTPS URL for the webhook (`crew_settings_set.public_url`). |
| Autonomous work loop and triggers | `AW07` Agent-proposed automations (human-gated) | Implemented and verified | `ATLAS_ACTION add_trigger` card, Add or Skip (`crates/atlas-host/src/crew/runner/action.rs:204-228`, `handlers/tasks.rs:261-286`). `actions` 6, `automations` 7 passed. | Human gate by design; webhooks are not offered to agents. |
| Autonomous work loop and triggers | `AW08` Triggers: deadline, goal review, follow-up, condition, MCP event | Missing | Absent from `TriggerKind` (`crates/atlas-host/src/crew/store/mod.rs:76-128`). | Goal review can be emulated by a schedule trigger with a prompt. |
| Autonomous work loop and triggers | `AW09` Heartbeat, relevance check before a run, and an auditable skip | Missing | Every trigger starts a task unconditionally; grep for heartbeat and proactive finds nothing (audit-organization-work). | A cheap triage turn before enqueue, like `runner/summary.rs`, with an audit `trigger_skipped`. No `trigger_skipped` audit kind either, so an ignored event leaves no record (scenario C). |
| Autonomous work loop and triggers | `AW10` Typed event normalization (RFC 12.2) | Implemented, incomplete | GitHub only: `Fired{event,origin}` and `Event` enum (`crates/atlas-host/src/crew/triggers/mod.rs:441-455`). | Other sources are free text with one template variable `{{event}}`. |
| Autonomous work loop and triggers | `AW11` Event dedupe | Implemented, incomplete | GitHub seen file (256 KiB cap, `triggers/github.rs:25`) and delivery claim (`github/app/webhook.rs:150,312-323`). `hygiene` covers an unwritable seen file. | Nothing for webhook, file, service or schedule beyond a debounce. No cross-source dedupe. |
| Autonomous work loop and triggers | `AW12` Missed-run policy | Implemented, incomplete | `is_due` fires once after downtime; paused crew skips (`crates/atlas-host/src/crew/schedule.rs:68-70`, `triggers/mod.rs:176-181`). `time_contract`, `a_trigger_does_not_fire_while_paused` passed. | One behavior, not configurable. Add `Trigger.missed: skip\|run_once\|catch_up`. |
| Autonomous work loop and triggers | `AW13` Bounded proactive work (budget, cooldown, per-trigger cap) | Missing | No cooldown, budget or cap per trigger; only `max_agents`, the 2 h task cap, resume cap, kill switch, `no_auto`. | A 60 s interval can enqueue without bound [INFERENCE]. Needs fields on `Trigger` and a check in `fire`. |
| Goals and budgets | `GB01` Goals, hierarchy, metrics, progress and review (RFC 13) | Missing | No `goal` in `crates/atlas-host/src/crew` or `docs/crew.md`; the Grand Line vision is a GitHub discussion, not crew code. | `goals.json` in the crew dir plus a schedule trigger for review. Progress tracking and review cadence are Missing too; a review can only be a schedule trigger with a prompt. |
| Goals and budgets | `GB02` Budgets (cost, tokens, time per task, crewmate, org, day) | Missing | `UsageRecord` is captured (`crates/atlas-host/src/crew/store/mod.rs:171-187`, `usage.jsonl`) but never aggregated or compared with a limit. grep for `budget` in `crew/**` finds only a config-key list. | Check in `slots::take` and after each turn (`runner/conversation.rs:260-270`); token or wall-time budgets cover codex and omp. Attribution by trigger or delegation needs run lineage. |
| Memory | `ME01` Agent-specific memory (memory.md) | Implemented, incomplete | `read_memory`, `write_memory`, 64 KiB cut (`crates/atlas-host/src/crew/store/mod.rs:915-943`); prompt tells the agent to append (`runner/prompt.rs:230-232`). Merged: organization audit Implemented and verified, surfaces audit incomplete. | Flat file, no structure. The cut drops the newest lines and may split UTF-8 [INFERENCE]. No test of the cap found. |
| Memory | `ME02` Memory scopes (department, organization, customer, project, run) | Missing | One scope per crewmate; isolation blocks cross-reads (`docs/crew.md` 'One Linux user per crewmate'). | hostd-owned `knowledge/<scope>.jsonl` with hook routes scoped by the caller's token. |
| Memory | `ME03` Knowledge records, provenance, contradiction and expiry handling | Missing | Memory is a markdown blob; provenance exists only for tasks and audit. | Record id, source task, ts, confidence, expires, status. Contradiction, retraction and expiry handling is Missing too. |
| Memory | `ME04` Memory change audit and provenance | Missing | `set_memory` writes no audit line (`crates/atlas-host/src/crew/handlers/settings.rs:17-23`); the agent writes the file directly. | `memory_changed{task_id, bytes, sha}` after each task and on `set_memory`. |
| Memory | `ME05` Knowledge sharing and retrieval between agents | Missing | Only by a human copying text or a shared repo. No search or retrieval tool. | `knowledge_add` and `knowledge_search` routes. |
| Reporting and notifications | `RN01` Task result report (visible reply) | Implemented and verified | `finish_task` (`crates/atlas-host/src/crew/runner/outcome.rs:149-257`), `visible_reply`. `tasks` 12, `payloads` 6, `mobile_payloads` 3 passed. | Free text. No category, impact, decision-required or next step. |
| Reporting and notifications | `RN02` Structured Report object with evidence links (category, impact, decision required) | Missing | No `Report` type (audit-organization-work, 2.6). `Task.origin`, `attachments`, `trigger_id`, `chain` give partial evidence links. | Build a `Report` from existing `Task` fields and send it through one `notify` function. |
| Reporting and notifications | `RN03` Reply summary (short_replies) | Implemented, incomplete | `runner/summary.rs:maybe_summarize`, threshold 600 chars, cheap model, 25 s timeout. 4 tests passed with a fake summarizer. | Compresses one reply; extra engine spend; not an aggregate. |
| Reporting and notifications | `RN04` GitHub reply comment (post, edit, final link) | Implemented and verified | `runner/reporting.rs`, `github/reply.rs:Reporter`, 30 s throttle. `github_out` 12 passed. | Per task, best effort, not aggregated. |
| Reporting and notifications | `RN05` Notification decision and duplicate suppression (importance, urgency, quiet hours) | Missing | Hard-wired by task state: Telegram gets every done and failed, push only done (`crates/atlas-host/src/crew/runner/outcome.rs:211-262`); cards notify only on gate refusals (`runner/gate.rs:200-201`). Duplicate suppression exists inside one source only (GitHub seen file; plain-pane push `Tracker` cooldown 30 s, `crates/atlas-host/src/agents/push.rs`). | Replace the two sends with one `notify(report)` holding the policy. Crewmate pushes have no cooldown. |
| Reporting and notifications | `RN06` Crew UI channel (events to desktop and phone) | Implemented and verified | Push events over the hostd socket (`crates/atlas-host/src/crew/events.rs`), relayed with scopes (`crew/relay/mod.rs:~376`). `relay` 11 passed. | `crew_paused` and `crew_engine_auth` are dropped for phones (see Defects). |
| Reporting and notifications | `RN07` Mobile push through the relay | Prototype | `crates/atlas-host/src/crew/push.rs`, `services/push-relay` (Go). `push` 7 and `tests::push` 5 passed. `docs/crew.md:524`: 'Not live yet'. | Relay not deployed, app built without Firebase. Kinds are `task_done`, `needs_you`, `agent_*` only; a failed task does not push. |
| Reporting and notifications | `RN08` Other channels (email, WhatsApp, Slack, Discord) | Missing | Not in `crates/atlas-host/src/crew/`. | Sinks behind the single `notify`. |
| Reporting and notifications | `RN09` Digest (daily, weekly, department, goal, exception) | Missing | None. Agents cannot read other crewmates' tasks (no route). | A hostd job that reads tasks, audit and usage and emits one `Report`. |
| Organization UX | `UX01` Organization overview (company, team, mission, goal, budget) | Implemented, incomplete | Desktop `CrewWorkspace.svelte`, `CrewSidebar` (server, project, crewmate); mobile `list/crew_screen.dart`. 833 desktop tests passed (prepared copy). | Only two views, `chat` and `board` (`apps/desktop/src/lib/crew/crewView.svelte.ts:9`). No company, goal or budget view. |
| Organization UX | `UX02` Work view and board | Implemented and verified | `apps/desktop/src/lib/crew/board.ts`, `CrewBoard.svelte`; mobile `board.dart`. `board.test.ts` in the 833; Flutter `board_test.dart` read, not run. | Flat: no dependencies, assignment, priority, parent or goal link. |
| Organization UX | `UX03` Attention and needs-you | Implemented and verified | `crewmate_state` (`crates/atlas-host/src/crew/handlers/crewmates.rs:84-93`); cards `TaskAction` (8 kinds, `store/mod.rs:299-338`); mobile `needs_you_card.dart`. `approvals`, `actions`, `prove_ui`, `omp_approval` passed. | Per crewmate and per card; no cross-crew inbox. |
| Organization UX | `UX04` Cross-crew attention inbox (priority, SLA, escalation) | Missing | No such command; `crewmate_list` carries `waiting_task` and `pending_approval` per crewmate. | Derived read-only `crew_attention` command; no new storage. |
| Organization UX | `UX05` Organization graph view (reports to, team, hierarchy) | Missing | No `reports_to`, team or manager field (`crates/atlas-host/src/crew/store/mod.rs:263-295`; grep over `crew/`). | Optional `reports_to` and `team` on `Crewmate`. |
| Organization UX | `UX06` Agent status and health view | Implemented, incomplete | `crewmate_list` state, queued, `pending_approval`, engine status, `agent_slots` (`handlers/crewmates.rs:101-215`). `engine_login`, `limits`, `pause` passed. | No health score, heartbeat, error rate. Stuck tasks only caught by the 2 h timeout. |
| Organization UX | `UX07` Telegram bridge (notifications, approvals, task entry) | Implemented and verified | `crates/atlas-host/src/crew/telegram.rs:54,286-313,575-700`; `crew::tests::telegram` and units passed (13 + 2 + 2). Merged: organization and surfaces audits. | One bot per server, broadcast to all linked chats. Not reachable from a phone through the relay. |
| Organization UX | `UX08` Mobile crew screens other than the board (list, ops, profile) | Unknown | Flutter tests were not run (audit instruction); code read only under `apps/mobile/lib/features/crew/`. | Run `flutter test` for `features/crew` before relying on them. |
| Organization UX | `UX09` Phone receives pause and engine-auth pushes | Implemented, incomplete | `push_scope` (`crates/atlas-host/src/mobile/mod.rs:456-471`) lacks `crew_paused` and `crew_engine_auth`; `docs/crew.md:1003` says scope `act`. | Two match arms plus a relay test beside `crew/relay/tests.rs:233` (see Defects). |
| Organization UX | `UX10` Organization visibility: events, policies, goals, reports, departments views | Missing | Scenario H: 5 of 12 items exist (agents, current work, approvals, audit, health). The other 7 have no object, wire command or screen (audit-surfaces-ax). | Each needs data first; UI follows. |
| Agent experience and MCP tools | `AX01` Agent operating context: rules (permissions, rails, grammar) | Implemented and verified | `approval::fragment(mode, no_auto)` (`crates/atlas-host/src/crew/approval.rs:430-470`), sent with the first prompt and on mode change; `runner/prompt.rs:226-248`. `approvals`, `permissions`, `omp_approval` passed. | Prompt does not list the crewmate's tools, teammates, budget or goals. |
| Agent experience and MCP tools | `AX02` Agent operating context: responsibilities and charter | Missing | Prompt carries `role` and task text only (`crates/atlas-host/src/crew/runner/prompt.rs:204-249`). | Second optional prompt section built from new `Crewmate` fields. |
| Agent experience and MCP tools | `AX03` Agent self-observability (own audit, tasks, usage) | Missing | `crewmate_audit`, `crewmate_usage`, `crewmate_tasks` are socket commands for clients, not agent routes (`crates/atlas-host/src/crew/mcp.rs:345-470`). | MCP tools backed by new `/crew/<id>/...` routes limited to the caller. |
| Agent experience and MCP tools | `AX04` Feedback loops from human decisions | Implemented, incomplete | Allow once, allow for task, deny with a comment the agent reads (`crates/atlas-host/src/crew/approval.rs:1-17`); `refused` and `prove_ui` cards. `guards`, `prove_ui` passed. | Feedback stays inside one task. Nothing aggregates it into memory, rules or evals. |
| Agent experience and MCP tools | `AX05` MCP server for agents (7 tools) | Implemented and verified | `atlas mcp --crewmate <id>` (`crates/atlas-cli/src/commands/mcp.rs:24-25`) to `crew::mcp::serve`: `ask_user`, `set_profile`, `capture`, `locate`, `ui_gate`, `set_ui`, `approve`. `tools_list_names_every_tool_exactly_once` and `crew::mcp` units passed. | Wired for claude, codex and opencode only. |
| Agent experience and MCP tools | `AX06` omp crewmates get the atlas MCP tools | Missing | `engine/omp.rs` never uses `McpServer` (only claude, codex, opencode; `crates/atlas-host/src/crew/engine/launch.rs:481-508`). | Add an omp renderer in `launch.rs`. A UI task on omp cannot pass the two-capture gate [INFERENCE]. |
| Agent experience and MCP tools | `AX07` Agent edits its own profile (set_profile) | Implemented, incomplete | `handle_set_profile` (`crates/atlas-host/src/agents/hook_server.rs:570-596`), caps 40 and 2000 chars. No audit call in that function (grep, see Defects). | Identity is agent-mutable and unaudited. |
| Agent experience and MCP tools | `AX08` In-conversation action cards (ATLAS_ACTION, 8 kinds) | Implemented and verified | `runner/action.rs`, grammar `runner/prompt.rs:236-248`. `actions` 6, `answers` passed. | None noted. |
| Capabilities and integrations | `CI01` Capability registry (versioned manifest per crewmate or crew) | Missing | Static only: `crates/atlas-core/data/agents.json` and desktop catalog `apps/desktop/src/lib/crew/integrations.ts:13-190`. | Per-crewmate `capabilities` list that `tools_list()` filters. |
| Capabilities and integrations | `CI02` Native integration: GitHub (App, gh login, webhook, reply) | Implemented and verified | `crates/atlas-host/src/crew/github/app/mod.rs:1-40`, protocol `crew_github_app_*` (`protocol.rs:839-864`). `github_app`, `github_in`, `github_out`, `github_webhook`, `github_hook_events`, `github_e2e` passed. | Needs a public URL. Phones cannot touch the App (`mobile/mod.rs:573-576`). |
| Capabilities and integrations | `CI03` Browser capabilities: capture, locate, ui_gate | Implemented and verified | `crates/atlas-host/src/crew/capture/`, `locate/`, `uigate/`; `crew::tests::{capture,locate,uigate,prove_ui}` passed (246 batch). | Read-only observation of a URL, not a general browser agent. Opt-in install (about 500 MB). |
| Capabilities and integrations | `CI04` Computer use (desktop control) | Missing | Not part of the crew (audit-surfaces-ax). | Out of reach before the browser sandbox work. |
| Capabilities and integrations | `CI05` Voice (input, spoken replies, voice card answers) | Implemented and verified | `crates/atlas-host/src/crew/voice/**`, `voice_card.rs`, `voice_answer.rs`. `voice`, `voice_answers` passed. Spoken line: `runner/spoken.rs` (`agent_line`, 300 chars), 7 tests passed. | An I/O channel, not a capability agents call. |
| Capabilities and integrations | `CI06` MCP client: external servers per crewmate | Missing | Only `atlas` is given to an engine (`crates/atlas-host/src/crew/engine/launch.rs:182-196`); `McpServer` is a single value. | `Vec<McpServer>` from a per-crewmate file; protect with the `.mcp.json` rail. |
| Capabilities and integrations | `CI07` Third-party integrations (Linear, Notion, Slack, Figma, 1Password, Google Docs) | Planned | Catalog entries marked `coming-soon` (`apps/desktop/src/lib/crew/integrations.ts:105-155`). | Catalog only; no code behind them. |
| Capabilities and integrations | `CI08` Reusable skills as a capability | Planned | GitHub skills in the catalog are mostly `available: false` (`apps/desktop/src/lib/crew/integrations.ts:76-94`). | No loader on the host side. |
| Interop | `IO01` hostd wire protocol (PROTO=2, caps, golden fixtures) | Implemented and verified | `crates/atlas-host/src/protocol.rs:77`; 45 `crew*` commands pinned in `testdata/hostd/commands.json`; `protocol_golden` passed (31 in batch 1). | Additive rule only; a removal needs a PROTO bump and mobile `kMinHostProto` (`apps/mobile/lib/core/protocol/protocol_version.dart:10`). |
| Interop | `IO02` Mobile relay to a crew server (crew_call) | Implemented and verified | `crates/atlas-host/src/crew/relay/mod.rs`; `relay` 11 passed (one test needs 20 s). | Terminals, GitHub App, Telegram, voice setup and secrets are refused for phones. |
| Interop | `IO03` A2A (agent to agent protocol) | Missing | grep for `a2a` and `agent2agent` over `crates`, `apps`, `docs` finds nothing (audit-surfaces-ax). | `study-ag-ui.md` notes A2A as an adjacent protocol only. |
| Interop | `IO04` ACP (Agent Client Protocol) | Missing | No hits for `agent-client-protocol`; engines are driven as CLIs (`crates/atlas-host/src/crew/engine/*.rs`). | Evaluate separately as an engine driver. |
| Interop | `IO05` AG-UI and a run event stream for UIs (run id, per-call events, scope) | Missing | No AG-UI code. Mappable events exist: `crewmate_task`, `agent_step`, `agent_status`, `agent_approval` (`crates/atlas-host/src/agents/mod.rs:355-453`). `agent_step` is broadcast but no client reads it and phones never receive it (`crates/atlas-host/src/server.rs:700-712`; grep `agent_step` in `apps/` is empty). | Needs run id, deltas for headless engines, scoped events (see study-ag-ui.md). Add `agent_step` to `push_scope` after redaction; include `task_id`. |
| Observability, audit, evaluation | `OB01` Audit log (kinds, redaction, rotation) | Implemented, incomplete | `crates/atlas-host/src/crew/audit/mod.rs`; redaction `audit/redact.rs`; 1 MiB x 5 files. `audit` 39 passed. | Not fsynced, no hash chain, oldest file deleted, no `run_id`, `policy`, `result`, person identity. |
| Observability, audit, evaluation | `OB02` Audit views (desktop and mobile) | Implemented and verified | Desktop `settings/AuditSection.svelte`, mobile `ops/crew_audit_screen.dart`. `crew::tests::audit` 8 and `mobile_payloads::audit_fixtures_are_real_payloads` passed. | No cross-crewmate query, per-task view or export. `AuditKind` in `audit.ts` lists 9 kinds, host writes more. `docs/crew.md:817` is wrong (see Defects). |
| Observability, audit, evaluation | `OB03` Audit integrity and correlation (hash chain, fsync, run id, actor, policy) | Missing | `append_line(..., durable=true)` exists (`crates/atlas-host/src/crew/store/jsonl.rs:60`) but audit uses `durable=false`. Tool calls tie to a task by `task_id` only; `device` is a connection class, not a person. | `prev_hash` per line, fsync for decisions, gzip archive instead of delete. Add optional `run_id`, `actor`, `policy`, `result` fields. |
| Observability, audit, evaluation | `OB04` Activity feed per task (steps) | Implemented, incomplete | `Task.steps` up to 50 (`crates/atlas-host/src/crew/runner/outcome.rs:74-80`); `crew::tests::tasks::crewmate_tasks_steps` passed. | No cross-crew stream. Steps depend on agent hooks (see next row). |
| Observability, audit, evaluation | `OB05` Steps and audit tool_call for headless-engine tasks | Unknown | `runner/conversation.rs` never appends a `Step` (no `Step` or `append_step` hit); `atlas crew install` sets no hooks. `docs/crew.md:796` claims headless engines alike. Reason unknown: needs a live run with `claude` headless. | Emit steps from engine streams (`ToolSink`, `engine/mod.rs:174`). |
| Observability, audit, evaluation | `OB06` Traces per run (trace id, spans, model calls) | Missing | No run id, trace id or OpenTelemetry in `crates/atlas-host/src/crew/` (grep). | Use `task_id` as run key and add `model_call` audit entries from `UsageRecord`. |
| Observability, audit, evaluation | `OB07` Cost and usage accounting | Implemented, incomplete | `UsageRecord` and `usage.jsonl` (`crates/atlas-host/src/crew/store/usage.rs`, `store/mod.rs:1014-1060`); `crewmate_usage`; desktop `UsageSection`, mobile `crewmate_usage_screen.dart`. `usage` 13 passed. Merged from three audits. | Raw records only; no server-side aggregate; terminal-agent tasks produce no usage; codex and omp report no USD. |
| Observability, audit, evaluation | `OB08` Value measurement: completed, failed, duration, interventions | Implemented, incomplete | Derivable from `tasks.jsonl` and `approval_decision.wait_ms`; no endpoint (audit-organization-work, 2.7). | `crew_value{since}` aggregating existing data. |
| Observability, audit, evaluation | `OB09` Value measurement: time saved, outcomes, goal progress | Missing | No baseline or method model. | Report measured quantities only until a baseline field exists. |
| Observability, audit, evaluation | `OB10` Agent evaluations and regression suite (RFC 23) | Missing | No eval runner, golden transcripts or scoring. Rust tests use fake engines to test hostd, not agent quality. | `ui_gate` regressions are an eval-like replay for UI only. |
| Observability, audit, evaluation | `OB11` End-to-end test of the crew platform | Implemented and verified | `.github/workflows/crew-e2e.yml` runs `scripts/crew-e2e.sh` on privileged systemd (install twice, isolation A/B/C, restart mid-task, pause). 48 Rust test files, about 361 test attributes. | Fake `claude` script; no real model, goal or GitHub flow. Not run by the audits. |
| Observability, audit, evaluation | `OB12` Contract fixtures shared by Rust, desktop and mobile | Implemented and verified | Rust `*_fixture_is_real_payload` tests write `apps/mobile/test/fixtures/*.json`; desktop and Flutter read them (`apps/desktop/src/lib/crew/crewmateTaskResumed.contract.test.ts:8`). | Desktop crew tests fail from a clean checkout without `bun install` (see Defects). |
| Observability, audit, evaluation | `OB13` Desktop crew test suite from a clean checkout | Implemented, incomplete | Run in `apps/desktop`: all 67 files fail (`Cannot find module '@inlang/paraglide-js'`, `apps/desktop/scripts/paraglide.ts`, `test/setup.ts:10`). In a prepared copy: 833 pass, 0 fail. | Document `bun install` as a pre-step; the suite also reads `apps/mobile` and `crates`. |
| Observability, audit, evaluation | `OB14` Simulation and dry-run of tasks, automations or policies | Missing | Only `playwright install-deps --dry-run` (`crates/atlas-cli/src/commands/crew/apt.rs:5`). The test-only fake engine `run_fake` (`crates/atlas-host/src/crew/engine/mod.rs:241-264`) is `#[cfg(test)]`. | Promote `run_fake` to a `dry_run` engine kind. |

## Scenario coverage

RFC section 29 lists acceptance scenarios A to H. For each one: the capabilities it needs, their status today, and whether the scenario can run now. 'Can run today' means a person could follow the scenario on a current crew server and see every bullet hold. IDs refer to the matrix above.

### Scenario A: Configure an organization

**Can run today: No.** The crew has no organization, department or blueprint object. The only guided setup is one crewmate at a time. Export and import of a crew exists.

| RFC requirement | Capability | Status today |
|---|---|---|
| Generate a blueprint from a company description | `OM08` Organization Architect: blueprint, review, simulation and authorized activation | Missing |
| Departments, roles, agents, responsibilities | `OM01` Organization as a first-class entity | Missing |
|  | `OM03` Departments | Missing |
|  | `OM04` Responsibilities and structured roles (inputs, outputs, authority, escalation) | Missing |
|  | `AR01` Agent identity and role | Implemented, incomplete |
| Capabilities, policies, goals, event sources in the blueprint | `CI01` Capability registry (versioned manifest per crewmate or crew) | Missing |
|  | `HA09` Policy hierarchy, policy objects and autonomy profiles (RFC 15.1, 15.2) | Missing |
|  | `GB01` Goals, hierarchy, metrics, progress and review (RFC 13) | Missing |
|  | `AW10` Typed event normalization (RFC 12.2) | Implemented, incomplete |
| Unknowns identified; user can review and edit | `OM08` Organization Architect: blueprint, review, simulation and authorized activation | Missing |
|  | `OM09` Guided setup of a single crewmate (setup chat) | Implemented, incomplete |
| Simulation available | `OB14` Simulation and dry-run of tasks, automations or policies | Missing |
| Activation requires explicit authorization | `HA08` Permission modes and protected-path rails | Implemented and verified |
|  | `OM07` Export and import of a crew (archive) | Implemented and verified |

### Scenario B: Cross-agent delegation

**Can run today: No.** An agent has no route or tool to create work for another crewmate. The nearest mechanism is the after-crewmate chain trigger, which passes one text line and no result.

| RFC requirement | Capability | Status today |
|---|---|---|
| Identify an appropriate recipient | `CD09` Routing by role, capability, department or load | Missing |
|  | `CD08` Routing: explicit recipient | Implemented, incomplete |
|  | `OM05` Agent registry (queryable by role or capability) | Implemented, incomplete |
| Create a structured delegation | `CD02` Delegation by an agent (parent run, recipient, scope, budget, deadline) | Missing |
|  | `CD01` Agent-to-agent messages, requests, broadcast and envelope (RFC 9.1, 9.2) | Missing |
| Context is permission-checked | `CD03` Context transfer with permission check (RFC 9.3) | Missing |
|  | `HA20` Untrusted input labeling and prompt-injection handling | Implemented, incomplete |
| Child task has an owner and durable state | `CD11` Ownership, durable state and duplicate-assignment avoidance | Implemented, incomplete |
|  | `DE03` Parent and child runs, run lineage | Missing |
|  | `DE01` Task and run record with states | Implemented, incomplete |
| Parent receives a result | `CD05` Handoff that carries the result and artifacts | Missing |
|  | `CD04` Handoff by chain trigger (after-crewmate) | Implemented and verified |
| Failure or rejection is handled | `CD02` Delegation by an agent (parent run, recipient, scope, budget, deadline) | Missing |
|  | `CD12` Escalation to a human | Implemented, incomplete |
| No infinite delegation | `CD06` Cycle protection for chained work | Implemented and verified |
|  | `CD07` Delegation depth, fan-out and spam bounds | Missing |
|  | `HA24` Rate limits on task creation and delegation | Missing |

### Scenario C: Proactive work

**Can run today: Partly (GitHub events only).** A GitHub event becomes a normalized, deduplicated, untrusted-wrapped task. There is no policy check, no relevance step before a paid run, no record of an ignored event and no budget.

| RFC requirement | Capability | Status today |
|---|---|---|
| Event is normalized | `AW10` Typed event normalization (RFC 12.2) | Implemented, incomplete |
|  | `AW06` Trigger: GitHub (8 event kinds) | Implemented and verified |
| Relevant policy is evaluated | `HA09` Policy hierarchy, policy objects and autonomy profiles (RFC 15.1, 15.2) | Missing |
| Duplicate work is prevented | `AW11` Event dedupe | Implemented, incomplete |
| Agent decides whether work is necessary | `AW09` Heartbeat, relevance check before a run, and an auditable skip | Missing |
| Work created or ignored with an auditable outcome | `AW09` Heartbeat, relevance check before a run, and an auditable skip | Missing |
|  | `OB01` Audit log (kinds, redaction, rotation) | Implemented, incomplete |
| Work is bounded by budget and policy | `AW13` Bounded proactive work (budget, cooldown, per-trigger cap) | Missing |
|  | `GB02` Budgets (cost, tokens, time per task, crewmate, org, day) | Missing |
|  | `HA23` Admission control: max_agents and per-crewmate FIFO | Implemented and verified |
| Schedule and interval sources | `AW01` Trigger: schedule (cron, time zone) and interval | Implemented and verified |
|  | `AW02` Single scheduler (legacy routines retired) | Implemented, incomplete |

### Scenario D: Human approval

**Can run today: Partly.** Claude and omp tool calls are held, shown with their input, answered by an owner, audited and denied after a timeout. The approval is bound to a pane and request id, not to a canonical hash, and nothing invalidates it on a material change.

| RFC requirement | Capability | Status today |
|---|---|---|
| Exact action is shown | `HA06` Host authority decision path for tool calls | Implemented and verified |
|  | `UX03` Attention and needs-you | Implemented and verified |
| Context and impact are shown | `AX01` Agent operating context: rules (permissions, rails, grammar) | Implemented and verified |
|  | `UX03` Attention and needs-you | Implemented and verified |
| Approval is bound to the action; material changes invalidate it | `HA15` Approval binding to the exact action and material-change invalidation (RFC 15.5) | Implemented, incomplete |
| Approval expires | `HA16` Approval expiry | Implemented, incomplete |
| Agent cannot bypass approval | `HA06` Host authority decision path for tool calls | Implemented and verified |
|  | `HA08` Permission modes and protected-path rails | Implemented and verified |
|  | `HA07` Engines that cannot ask for approval (codex, opencode) | Implemented, incomplete |
| Destructive actions are held in every mode | `HA12` Destructive-action classification | Implemented and verified |
|  | `HA13` Destructive-action gate outside voice approvals | Missing |
| Approval is auditable | `OB01` Audit log (kinds, redaction, rotation) | Implemented, incomplete |
|  | `OB02` Audit views (desktop and mobile) | Implemented and verified |
| Approval survives a restart | `HA14` Approval persistence across restart | Implemented, incomplete |

### Scenario E: Failure recovery

**Can run today: Partly.** An interrupted task is requeued and its engine session resumed, with a cap of two resumes. Hostd does not know which external actions already happened, so completed work can repeat and uncertain outcomes are not reconciled.

| RFC requirement | Capability | Status today |
|---|---|---|
| State is recovered | `DE05` Recovery after restart (resume cap, durable human wait) | Implemented and verified |
|  | `DE04` Durable runs and checkpoints | Implemented, incomplete |
|  | `AR07` Session continuity (engine session resume) | Implemented and verified |
| Completed work is not duplicated | `DE06` Idempotency of external actions | Implemented, incomplete |
|  | `DE07` Tool-call journal (intent, result, class) | Missing |
| Uncertain outcomes are reconciled | `DE08` Uncertain-outcome handling and reconciliation | Missing |
| Run status is truthful | `DE01` Task and run record with states | Implemented, incomplete |
|  | `DE02` Explicit run states (waiting approval, recovering, outcome uncertain, paused) | Missing |
| Human intervention is requested if needed | `CD12` Escalation to a human | Implemented, incomplete |
|  | `UX03` Attention and needs-you | Implemented and verified |
| Terminal agents | `AR06` Terminal-agent crewmates (pty path) | Implemented, incomplete |

### Scenario F: Meaningful reporting

**Can run today: No.** Each task produces its own reply, Telegram message and push. Nothing aggregates runs, ranks them or suppresses duplicates across sources.

| RFC requirement | Capability | Status today |
|---|---|---|
| Important outcomes aggregated | `RN02` Structured Report object with evidence links (category, impact, decision required) | Missing |
|  | `RN09` Digest (daily, weekly, department, goal, exception) | Missing |
| Duplicates suppressed; priority applied | `RN05` Notification decision and duplicate suppression (importance, urgency, quiet hours) | Missing |
| User receives an actionable report | `RN01` Task result report (visible reply) | Implemented and verified |
|  | `RN06` Crew UI channel (events to desktop and phone) | Implemented and verified |
|  | `UX04` Cross-crew attention inbox (priority, SLA, escalation) | Missing |
| Mobile push | `RN07` Mobile push through the relay | Prototype |
| Report links to evidence and related work | `RN02` Structured Report object with evidence links (category, impact, decision required) | Missing |
|  | `DE09` Artifacts and attachments | Implemented, incomplete |
|  | `CD13` Delegation graph visible to a human | Implemented, incomplete |

### Scenario G: Organizational memory

**Can run today: No.** Memory is one markdown file per crewmate with no provenance, scope or expiry. OS-level isolation keeps crewmates from reading each other, but no shared store exists to read from.

| RFC requirement | Capability | Status today |
|---|---|---|
| Fact has source and provenance | `ME03` Knowledge records, provenance, contradiction and expiry handling | Missing |
|  | `ME04` Memory change audit and provenance | Missing |
| Stored in the appropriate scope | `ME02` Memory scopes (department, organization, customer, project, run) | Missing |
| Authorized agents can retrieve it | `ME05` Knowledge sharing and retrieval between agents | Missing |
| Unauthorized agents cannot | `HA01` Isolation per Linux user (fail closed without the helper) | Implemented and verified |
|  | `ME02` Memory scopes (department, organization, customer, project, run) | Missing |
| Contradictions and stale knowledge handled | `ME03` Knowledge records, provenance, contradiction and expiry handling | Missing |
| Per-agent memory today | `ME01` Agent-specific memory (memory.md) | Implemented, incomplete |

### Scenario H: Organization visibility

**Can run today: Partly.** Five of the twelve items the RFC lists exist (agents, current work, approvals, audit, health); the other seven have no object, wire command or screen (audit-surfaces-ax). Counts below were not re-derived by a new run.

| Item the human must inspect | Today | Capability |
|---|---|---|
| Organization | Missing | `OM01` (Missing) |
| Departments | Missing | `OM03` (Missing) |
| Agents | Exists (list, sidebar) | `OM05` (Implemented, incomplete), `UX01` (Implemented, incomplete) |
| Current work | Exists (board) | `UX02` (Implemented and verified) |
| Delegations | Data in `Task.chain` and the Crewmate trigger, no client reads it | `CD13` (Implemented, incomplete) |
| Approvals | Exists (needs-you cards, audit) | `UX03` (Implemented and verified) |
| Goals | Missing | `GB01` (Missing) |
| Events | Only the trigger list per crewmate and `Task.source` | `UX10` (Missing), `AW10` (Implemented, incomplete) |
| Reports | Missing (per-task reply only) | `RN02` (Missing) |
| Policies | Mode, rails, `default_approval`, `allow_bypass` in settings; no policy objects | `OM02` (Implemented, incomplete), `HA09` (Missing) |
| Health | Partial (state, engine sign-in, memory cap; no score) | `UX06` (Implemented, incomplete) |
| Audit | Exists (desktop and mobile) | `OB02` (Implemented and verified) |

