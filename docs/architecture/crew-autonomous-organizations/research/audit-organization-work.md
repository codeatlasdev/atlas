Raw evidence for docs/architecture/crew-autonomous-organizations/ (2026-11-20, main 72b47fc3).

The `/tmp/...` logs and clones cited below were local to the audit run and were not kept.

# Audit: Organization control plane, orchestrator, autonomous work, memory, goals, reporting

Baseline of `main` at `72b47fc3` (the main checkout, 2026-11-20). Nothing in any repo was modified. `atlas dev` was not run. Paths are repo-relative; line numbers are as read.

## 1. Scope

RFC §6 (organization model, departments, roles, lifecycle), §9 (agent communication), §10 (delegation, handoff, routing), §12 (autonomous loop, triggers, proactivity), §13 (goals), §14 (memory), §16 (reporting, notification), §22 (value); scenarios B, C, F, G. Code read: `crates/atlas-host/src/crew/{store,triggers,schedule.rs,runner,push.rs,telegram.rs,relay,mcp.rs,handlers,hub.rs,events.rs,audit}`, `crates/atlas-host/src/agents/{hook_server,hook_auth}.rs`, `docs/crew.md` (Crewmates, Automations, GitHub, Telegram, Push, MCP, Audit, Reference). The earlier study (PR #669) was used only for orientation; every row below was re-read in source.

### Tests run (all through the heavy queue, from the main checkout)

Command pattern: `atlas run --heavy --label <l> -- env CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 nice -n 15 cargo nextest run -p atlas-host --locked --no-fail-fast -E '<filter>'`

| Run | Filter (abridged) | Result |
|---|---|---|
| 1 | `crew::{triggers,schedule,push,telegram,runner::summary,runner::spoken,runner::reporting,store::tests,store::log_tests}` + `crew::tests::{triggers,automations,github_in,github_out,github_webhook,github_hook_events,github_e2e,untrusted,push,telegram,crewmates,crewmate_state,restart,pause,old_crons,time_contract,audit,tasks,limits,payloads,mobile_payloads}` | **259 run, 259 passed**, 1081 skipped, 43 s (log `/tmp/auditorg/test1.log`) |
| 2 | `crew::tests::{hygiene,actions,github_fake,github_app,answers}` + `crew::store::{archive,archive_tests}` + `crew::relay` + `crew::runner::recover` + `crew::triggers::{github_events,github_hook,file_watch,service,webhook}` | 58 run, 57 passed; the 58th (`relay::tests::the_relay_dials_a_crew_server_on_its_registered_port`) was killed by the tool's 300 s background deadline, not a test failure (`/tmp/auditorg/test2.log`). |
| 3 | `crew::relay` rerun alone | **11 run, 11 passed** (that test took 20 s). |

Not run: the 8-event GitHub `decide` matrix is covered inside run 1 (`github_in` 20, `github_hook_events` 6). No test exists for goals, delegation by an agent, digests, value reports or scoped memory (nothing to run).

## 2. Capability table

Classification key: IV = Implemented and verified; II = Implemented, incomplete; P = Prototype; PL = Planned; M = Missing; U = Unknown.

### 2.1 Organization model (§6)

| Capability | Class | Evidence | Notes |
|---|---|---|---|
| Organization as a first-class entity | **M** | No struct above the crew. The nearest things: the crew server (one hostd dir, `crew.json` = `CrewSettings`, `store/mod.rs:496-527`) and the desktop registry of servers (`~/.atlas/desktop/crew-servers.json`, read by `relay/mod.rs:70-90`). `CrewSettings` has no name, mission, goals, members, policies or knowledge fields. `docs/crew.md:5-15` calls "crew" the set of everything run with Crew. | One crew server = one implicit organization with no identity of its own. A server is a flat list of crewmates. |
| Crew-level settings / policy set | II | `CrewSettings`: `short_replies`, `paused`, `max_agents`, `agent_memory_max`, `default_approval`, `allow_bypass`, `voice`, `public_url` (`store/mod.rs:497-527`); `crew_settings_set` (`crew/mod.rs:217-239`). Tests: `crew::tests::pause` (12 pass), `limits`. | Only machine-level and permission-mode policy. No per-department or per-capability policy, no budgets, no autonomy profile, no quiet hours. |
| Departments | **M** | No `department` token anywhere in `crates/atlas-host/src/crew` (grep). Crewmates are a flat list (`list_crewmates`, `store/mod.rs:741`). | Grouping would be new data. |
| Roles (role distinct from agent instance) | II (free text only) | `Crewmate.role`: ≤2000 chars (`store/mod.rs:267-268`, `handlers/crewmates.rs:check_role`); put in the prompt as `You are {name}. {role}` (`runner/prompt.rs:226`); changeable by the agent itself via MCP `set_profile` (`hook_server.rs:570-595`, route in `AGENT_ROUTES` `hook_auth.rs:16-25`). | A role is a prose string on the instance. No machine-readable responsibilities, inputs/outputs, authority, success criteria, escalation conditions. Roles are not reusable or shared across instances. An agent can rewrite its own role (no audit of old text, only `role_chars`, `handlers/crewmates.rs:audit_changes`; and `handle_set_profile` records nothing at all [INFERENCE from reading: no `audit.record` in `hook_server.rs:570-595`]). |
| Agent lifecycle states | II | Runtime states `idle` / `working` / `needs_you` computed on read by `crewmate_state` (`handlers/crewmates.rs:82-93`), test `crew::tests::crewmate_state` (4 pass). Create/update/delete (`crewmates.rs:253,321,406`); kill switch for the whole crew `paused` (`hub.rs:103-132`, test `pause` 12 pass). Delete is destructive: cancels tasks, removes the Linux user and directory. | RFC states Draft, Configured, Simulated, Paused (per agent), Degraded, Disabled, Archived are absent. There is no per-crewmate pause/disable and no archive (delete only). Per-trigger `enabled` exists (`Trigger.enabled`, `store/mod.rs:134`). |
| Agent registry | II | `list_crewmates` (`store/mod.rs:741`), `crewmate_list` returns id, name, role, agent, mode, state, queued, running_task, last, triggers, look, project (`crewmates.rs:crewmate_json`). Telegram `/crew` (`telegram.rs:~653`). | A listing, not a registry: no capabilities, no availability metadata beyond `state`/`queued`, no load or performance history, not queryable by role/capability. |
| Memory-based "organization" persistence / import-export | IV (for what it is) | `crew::store::archive` format v1 (`archive.rs:1-80`): manifest + crew.json + per-crewmate definition, memory, tasks, usage. 22 tests pass (`archive_tests`). | Good portability primitive; format has a version gate (`FORMAT_VERSION`, `archive.rs:30`). |

### 2.2 Communication, delegation, handoff, routing (§9, §10)

| Capability | Class | Evidence | Notes |
|---|---|---|---|
| Agent-to-agent communication (message/request/task/response…) | **M** | MCP server exposes 7 tools: `ask_user`, `set_profile`, `capture`, `locate`, `ui_gate`, `set_ui`, `approve` (`mcp.rs:166-174` dispatch, `tools_list` at `mcp.rs:345`); hook routes an agent may call: `AGENT_ROUTES` (`hook_auth.rs:16-25`). None sends anything to another crewmate. | `ask_user` goes to the human only (`hook_server.rs:513-567`). The only inter-crewmate edge is the chain trigger (below). No envelope, no addressing, no durable mailbox, no accept/reject/clarify. |
| Delegation (parent run, recipient, objective, scope, budget, deadline…) | **M** | Same as above. An agent cannot create a task for another crewmate or itself (no hook route, no MCP tool). The nearest: a human `crewmate_task` (`handlers/tasks.rs:25-72`) and the `ATLAS_ACTION add_trigger` card that makes a human click Add (`runner/action.rs:204-228`, `handlers/tasks.rs:261-286`). | Scenario B cannot run today. The smallest extension is an `AGENT_ROUTES` entry (see §4). |
| Handoff by chaining (after-crewmate) | **IV** (the mechanism it offers) | `TriggerKind::Crewmate{from_crewmate,on}` (`store/mod.rs:104-112`); `triggers::on_task_finished` (`triggers/mod.rs:250-340`) is called from every finish path (`runner/{pty,conversation,prompt,mod}.rs`); `Task.chain` lineage (`store/mod.rs:441-446`). Tests: `crew::tests::triggers::crewmate_chain_trigger_fires_on_finish`; `hygiene` (5 pass) covers mutual cycle stops after one round with `chain_refused` audit, and a long A→B→C chain still runs (`tests/hygiene.rs:53-141`). | Passes only a one-line text event: `"Crewmate {name} done|failed"` (`triggers/mod.rs:~270`), NOT the result, task id or artifacts; the downstream prompt cannot reference the finished task. Match key is the producer's id, not a role. `cancelled` does not fire (`on_str` only done/failed). `chain` is the only loop bound: it blocks a repeat, but a fan-out chain across many distinct crewmates has no depth/count limit [INFERENCE: no counter in `on_task_finished`]. |
| Cycle / loop bound | IV (cycles), M (depth, rate) | Chain guard as above; `chain_refused` audit documented `docs/crew.md:790-815`. | No delegation depth, no per-crewmate rate limit, no spam limit; the runner queue is an unbounded `VecDeque` (`runner/mod.rs:~97`) [INFERENCE]. |
| Routing (who handles a task) | II | Explicit only: the sender names the crewmate id (`crewmate_task`), Telegram `/task <name>` or the sole crewmate (`telegram.rs:~664-700`, `create_task` `:883`), triggers bound to one crewmate each. GitHub: every crewmate's triggers evaluated against each event (`github_hook.rs:process`, `triggers/github.rs:poll_all`), so two crewmates both matching `owner/repo` both start (no assignment arbitration). | No routing by role, capability, department, load, or coordinator agent. Telegram with >1 crewmates and no `/task` replies "Which crewmate?". |
| Dynamic delegation (load, availability, permissions, cost) | **M** | Load data exists but is unused for routing: `runners.queue_len`, `running_task` (`runner/mod.rs:~120`), `AgentSlots` / `max_agents` (`runner/slots.rs`, default 4). | Primitives for a scheduler exist; nothing consumes them. |
| Work queues & priorities | II | Per-crewmate FIFO `VecDeque`, one running (`runner/mod.rs:77-110,320`); `enqueue_front` used only for restart recovery (`:162`); server-wide cap `max_agents` with `waiting_for: "server busy"`. | No priority field on `Task`, no deadlines, no reservations, no leases, no cross-crewmate dependency graph, no join. |
| Ownership / durable state of work | II | A task has `id`, `source`, `trigger_id`, `chain`, `origin`, state, `restarts`, `resumed` (`store/mod.rs:358-447`); persisted as `tasks.jsonl`+`tasks.idx`; restart recovery `runner/recover.rs` (`tests/restart` 11 pass). | Ownership = the crewmate whose log holds it. No parent/child ids; `chain` lists crewmate ids, not task ids. No cancellation propagation. |
| Duplicate-assignment avoidance | II | Per source: GitHub seen-file + delivery claim (below); `crewmate_task` has no idempotency key. | Two humans or two webhooks can both enqueue identical work. |
| Human-visible communication history / graph | II | Audit lines `task_queued` carry `source` and `prompt`; `chain_refused` carries the chain; `Task.chain`/`trigger_id` are in `crewmate_tasks`. Desktop Crew board lists tasks across crewmates (docs/crew.md Desktop). | No delegation graph (no parent/child), no message log (no messages). |

### 2.3 Autonomous work: triggers, events, proactivity (§12)

| Capability | Class | Evidence | Notes |
|---|---|---|---|
| Trigger: schedule (cron + tz) | **IV** | `TriggerKind::Schedule` (`store/mod.rs:78-80`); `tick_schedule` every 30 s (`triggers/mod.rs:176-246`, loop `:370-385`); `schedule::{parse_cron,is_due,next_fire_after}` (`schedule.rs:17-70`). Tests: `time_contract` (10), `old_crons` (1), `triggers::tests` (9), `schedule::tests` (3), `automations` (7); `routine_every_minute_fires_within_90s` ran for real (30 s). | New schedule baselines `last_fired = now` at set time (`handlers/settings.rs:set_triggers`). Legacy `routines` (`Routine`, `store/mod.rs:50-56`) still ticked separately by `schedule::tick` (`schedule.rs:90-148`): two overlapping schedulers in the product. |
| Trigger: interval (≥60 s) | IV | `triggers/mod.rs:~190-200`; validate `:42`; test `interval_rejects_under_60s`, `interval_fires_after_duration` (note: the second test re-implements the match inline rather than calling `tick_schedule`; the real tick is exercised by `routine_every_minute_fires_within_90s`/`crewmate_list_includes_triggers`). | |
| Trigger: file | II | `file_watch.rs`: `notify` recursive watcher per trigger, 5 s debounce, falls back to 30 s mtime poll; glob filter and own-worktree exclusion (`Filter`, `file_watch.rs:62-110`). 4 unit tests pass. Rescan for new triggers every 120 s (`:26`). | Registered once per trigger id (`registered` set) and never unregistered or reconfigured when a trigger is edited or disabled until hostd restarts [INFERENCE from `register_new`, `:34-55`]. Event is only the changed path; no content hash, no dedupe beyond the 5 s debounce. |
| Trigger: service | Prototype | `service.rs`: polls `.logs/<svc>.log` tail every 30 s, substring match on error/failed/crash vs started/ready/listening (`:116-130`); in-memory state; Unknown→Failed deliberately ignored (`:62-70`). 2 tests pass (tail reading only). | Heuristic log scraping; state lost on restart; no test of the fire edge. Only helps projects that run `atlas dev`. |
| Trigger: webhook (generic) | IV (auth), II (dedupe) | `webhook.rs:handle`: token compared by equality to the trigger's token inside path `POST /crew/<id>/hook/<token>` (`webhook.rs:26-68`); body trimmed to 200 chars and wrapped untrusted (`:52-66`); `no_auto` unless `allow_auto` (`triggers/mod.rs:fire`, `:460-540`). Hook server body cap 1 MiB (`hook_server.rs:27`). Tests: `webhook_wrong_token_rejected`, `webhook_correct_token_enqueues_task`, `a_trigger_does_not_fire_while_paused`, `untrusted` (9). | Webhook payload is cut to 200 chars (RFC event schema would want the whole normalized payload). No delivery-id/idempotency, no rate limit, no HMAC: the token is the only secret. Token compare is `==`, not constant-time (the helper `constant_time_eq` exists in `store/hooktoken.rs` and is used elsewhere) [INFERENCE: `webhook.rs:handle` uses `tk == token`]. |
| Trigger: GitHub (8 event kinds) | **IV** | `validate_trigger` lists `pr_opened, pr_review_requested, issue_opened, ci_failed, release, mention, issue_assigned, pr_review_comment` (`triggers/mod.rs:58-67`); `decide` (`github_events.rs:~75-270`); poll every 120 s (`github.rs:23`) and signed webhook `/github/webhook` (`github_hook.rs:55-107`). Tests: `github_in` 20, `github_out` 12, `github_webhook` 10, `github_hook_events` 6, `github_e2e` 5, `untrusted` 9: all pass. | Strongest area. `Fired.origin` gives a typed reply target. |
| Trigger: crewmate (chain) | IV | See 2.2. | |
| Trigger: heartbeat / proactive evaluation | **M** | None. Every trigger starts a task unconditionally; "decide whether work is necessary" happens only inside the agent run, after a full engine turn is paid. `grep -i heartbeat|proactiv` in `crew/` finds nothing relevant. | The cheapest evolution is a pre-run "triage" step (see §4). |
| Trigger: deadline, goal review, follow-up, condition, MCP event | **M** | Absent from `TriggerKind` (`store/mod.rs:76-128`). | |
| Event normalization | II | GitHub only: webhook delivery is rewritten to the feed shape and passed to the single `decide` (`github_hook.rs:decide_delivery`, `FromPayload`); output `Fired{event,origin}`. `Event` enum (`triggers/mod.rs:441-455`) = `Local | Stranger | Github`. | Every non-GitHub source is free text; no schema, no event type/id/correlation key, no subscription/filter language. `{{event}}` is the only template variable (`render_prompt`, `:128`). |
| Dedupe | II | GitHub poll: `<crewmate dir>/<trigger_id>_seen.jsonl` written *before* firing, capped at 256 KiB, new trigger baselines at the feed head (`github.rs:82-250`, `SEEN_MAX_BYTES`); 3 tests pass + `hygiene` covers an unwritable seen file. GitHub webhook: delivery ids claimed durably, replay returns 200 duplicate + `github_webhook_duplicate` audit (`app/webhook.rs:150,312-323`); `a_signed_delivery_starts_the_task_and_a_replayed_id_does_nothing` passes. Poll to webhook hand-over cutoff (`github.rs:122-160`). | Webhook, file, service, schedule: none beyond a debounce or in-memory state. No cross-source dedupe (same incident via CI-failed and a mention = two tasks). No aggregation/debounce/backpressure policy. |
| Missed-run policy | II (implicit) | `is_due` fires when the next fire after `last_fired` ≤ now (`schedule.rs:68-70`; `should_fire_cron`): after downtime a schedule fires once (one catch-up, not N); a **paused** crew skips and does not queue (`triggers/mod.rs:176-181`, doc `docs/crew.md` Pause); the 30 s ticker uses `MissedTickBehavior::Skip` (`:376`). Tests: `time_contract`, `pause::a_trigger_does_not_fire_while_paused`. | One behavior for all triggers; not configurable (no `skip|catch_up|run_once`), not documented as a contract except for pause. After a long outage a stale daily report fires immediately on boot [INFERENCE from `is_due` with an old `last_fired`]. |
| Bounded proactive work (budget, cooldown, relevance) | **M** | No budget, cooldown or concurrency cap per trigger. Only: `max_agents`, 2 h task cap (`runner/mod.rs:TASK_TIMEOUT`), 2 restarts cap (`recover.rs`), kill switch, `no_auto`. | An enabled interval trigger of 60 s with a long prompt can enqueue unboundedly (tick makes one task per due trigger; the queue is not capped) [INFERENCE]. |
| Agent-created work (agent proposes automations) | IV | `ATLAS_ACTION add_trigger` → validated → human card Add/Skip (`runner/action.rs:204-228`, `handlers/tasks.rs:261-286`; tests `actions` 6, `automations` 7). Webhooks are not offered to agents. | The only agent-initiated work primitive, and it is human-gated by design. |
| Untrusted-content wrapping / author filter | **IV** | `untrusted_event` (`triggers/mod.rs:141-155`), `author_allowed` (`:158`), `yolo_for` (`:170`), `rail` (approval.rs) ; `untrusted` 9 tests pass, `github_in` filter tests. | The RFC's context-poisoning concern is addressed for external text only. Chain events are marked `Local`; the finished task's *result* is not passed, so no poisoning path today, but also no way to hand results over. |

### 2.4 Goals (§13)

| Capability | Class | Evidence | Notes |
|---|---|---|---|
| Goals / hierarchy / metrics / review cadence | **M** | No `goal` anywhere in `crates/atlas-host/src/crew` or `docs/crew.md` for crewmates (grep). Product-level "Grand Line" vision is Discussion #343 and issue #348 coordination text, not in crew code. | Scheduled review can be emulated by a schedule trigger whose prompt says "review goal X": that is the whole existing primitive. |
| Goal progress tracking | **M** | Same. | |
| Budgets | **M** | `UsageRecord{model, tokens, cost_usd, duration_ms}` (`store/mod.rs:171-187`) is stored per task and appended to `usage.jsonl` (`append_usage`, `:1014-1035`, rotated at 1 MiB, 400 files); read by `crewmate_usage` (`handlers/tasks.rs:312-315`, raw records only). | Cost data is captured but never aggregated server-side, never compared with a limit, and never stops a run. `cost_usd` is `Option` (depends on the engine reporting it). `docs/crew.md` says no budgets. |

### 2.5 Memory (§14)

| Capability | Class | Evidence | Notes |
|---|---|---|---|
| Agent-specific memory (human preference/operational) | **IV** (as a file) | `memory.md` per crewmate: `read_memory`/`write_memory` (`store/mod.rs:915-943`, truncates at 64 KiB: `MAX`, `:927`); the agent is told its path and to append durable learnings (`runner/prompt.rs:230-231`); humans read/replace through `crewmate_memory`/`crewmate_memory_set` (`handlers/settings.rs:11-23`; `crew/mod.rs:208-209`); isolated crewmate files read/written with no-follow helpers (`store/mod.rs:918-941`); moves with the crewmate in the archive; test `isolation::a_crewmate_without_a_user_is_provisioned...keeps_its_memory`, `an_export_of_an_isolated_crewmate_carries_its_current_memory` (read, not run: `tests/isolation.rs` is cfg(unix) heavy; 25 isolation tests were not in my filter). | Truncation silently cuts the *tail* at a byte boundary (`&bytes[..MAX]`, may split a UTF-8 char), so a full memory drops the newest lessons (the most recently appended) rather than the oldest [INFERENCE from `write_memory`; only the human/API path truncates, the agent appends directly to the file]. No test of the 64 KiB cap found (`grep` over tests). |
| Memory scopes (department, organization, customer, project, run) | **M** | One scope: per crewmate. No shared file, no read path from one crewmate to another (the agent's working dir is its own; one Linux user per crewmate in isolated mode blocks cross-reads: `crew/spawn/layout.rs:17`, `docs/crew.md` "One Linux user per crewmate"). | Isolation is a feature here and is exactly why sharing needs an explicit hostd-mediated primitive. |
| Knowledge records (id, type, provenance, confidence, validity, expiry, review) | **M** | Memory is an unstructured markdown blob. Provenance exists only for tasks/audit, not for memory lines. | |
| Who writes memory / provenance / human correction | II | Who: the agent (appends directly), the human (replace via UI/API). Hostd records nothing for memory changes: `set_memory` has no `audit.record` (`handlers/settings.rs:17-23`) [verified by reading]. | Not audited, not diffed, no versioning; a poisoned memory (e.g. an agent that read a hostile issue and "learned" from it) cannot be traced to a task. |
| Contradiction / retraction / expiry | **M** | | |
| Knowledge sharing between crewmates | **M** | Possible only by a human copying text, or by both reading the same repo/work tree. | |
| Session continuity | IV | `CrewmateSession{session_id, session_file}` per crewmate (`store/mod.rs:160-169`), resume after restart (`recover.rs`). | Engine-level conversation memory, orthogonal to RFC "organization memory". |

### 2.6 Reporting and notification (§16)

| Capability | Class | Evidence | Notes |
|---|---|---|---|
| Task result report (visible reply, `result`, `reply_full`) | IV | `finish_task` (`runner/outcome.rs:149-257`) stores `result` (ATLAS_ACTION line stripped by `visible_reply`), broadcasts `crewmate_task` event (`events.rs:9-15`). Tests: `tasks` 12, `payloads` 6, `mobile_payloads` 3 pass. | Free text. No category, impact, evidence, decision-required, related runs/agents, suggested next step. |
| Short summary of replies | II | `summary.rs:maybe_summarize` (threshold 600 chars, cheap model, 25 s timeout, falls back to full text); 4 tests pass (fake summarizer binary). | Summary = compression of one reply, not an aggregate across runs. Extra engine spend per long task. |
| Spoken line | IV | `runner/spoken.rs` (`agent_line`, `first_sentences`, cut at 300 chars); 7 tests pass; stored as `Task.spoken` (`store/mod.rs:~385`). | |
| GitHub reply comment (post, edit, final link) | **IV** | `runner/reporting.rs:start/progress/finish`, `github/reply.rs:Reporter` (marker `<!-- atlas-crew -->`, 30 s throttle `PROGRESS_INTERVAL`, ≤15 steps, ≤20k chars); `github_out` 12 tests pass. | Per-task, not per-event aggregated; best effort. |
| Notification decision (importance, urgency, duplicates, load, quiet hours, escalation) | **M** | Decision is hard-wired by task state: Telegram gets every `done` and `failed` (`outcome.rs:211-234`, 4000 chars); push gets only `done` (`outcome.rs:240-262`) plus `needs_you` for approvals (`hub.rs:forward_approvals` ~`:326`); action cards go to Telegram only for gate refusals (`runner/gate.rs:200-201` `send_card`). A task that fails sends Telegram but no push; a parked card (connect_github, choose, ask_user) sends neither push nor Telegram [INFERENCE: `park_task`, `action.rs:114-120`, and `handle_ask`, `hook_server.rs:513-567`, call neither]. | Every automated run notifies; a 60 s interval trigger would send one Telegram message per run. No per-user, per-crewmate or per-source preference; no quiet hours; no dedupe/aggregation. |
| Channel: Crew UI (desktop/mobile) | IV | Push events over the hostd socket (`events.rs`), relayed to the phone with scopes `relay/mod.rs:~376` (`crew_event`); `relay` 11 tests pass. | |
| Channel: Mobile push via relay | **P (not live)** | Code: `push.rs` (`notify`, `send_all`, grants, `403/410` drop), tests `push` 7 + `tests::push` 5 pass; relay service `services/push-relay` (Go). `docs/crew.md:521`: "Not live yet: the push relay is not deployed and the app is built without Firebase". Kinds are only `task_done`, `needs_you` (and `agent_done`/`agent_needs_you` for plain panes, `agents/push.rs`). | The `kind`-only payload is a privacy decision (no content). Anything richer (`title`, report category) would need a relay change. |
| Channel: Telegram | **IV** | `telegram.rs`: one bot per server, linked chats, `send_message`, approval buttons, `/crew /mode /task /ui`, photos, voice notes; 13 + 2 + 2 tests pass (link-code lockout, API path, media group). | Outbound is broadcast to *all* linked chats, no per-chat routing or filter. |
| Channels: email, WhatsApp, Slack, Discord | **M** | Not in `crew/`. | |
| Duplicate-report suppression | II | Only inside one source: GitHub seen-file/delivery id and the push `Tracker` cooldown for plain agent panes (`agents/push.rs`, `COOLDOWN_MS` 30 s). Crewmate pushes have no cooldown. | |
| Digest (daily/weekly/department/goal/exception) | **M** | None. A digest can be emulated: a schedule trigger asks a crewmate to read `crewmate_tasks`… but agents have no tool to read other crewmates' tasks (no MCP/hook route). | |
| Report that links evidence and related work | II | `Task.origin` (GitHub URL parts), `attachments` (agent captures), `trigger_id`, `chain`, audit. | No `Report` object; no artifact registry. |

### 2.7 Value measurement (§22)

| Capability | Class | Evidence | Notes |
|---|---|---|---|
| Tasks completed / failed / duration | II | Derivable from `tasks.jsonl` (`state`, `started_at`, `finished_at`, `source`) via `crewmate_tasks`; per-task `usage`. | No aggregation endpoint; clients compute. |
| Cost (tokens, USD) | II | `UsageRecord` + `usage.jsonl`, `crewmate_usage` (raw records). Desktop `UsageSection`, mobile `crewmate_usage_screen` render them (docs; UI code not read). | Cost for headless engines depends on what the engine prints; `cost_usd` optional. |
| Human interventions, approval latency | II | `approval_decision` audit has `wait_ms`, `source`, `device` (docs/crew.md audit table). | Raw audit only. |
| Time saved, outcomes, goal progress, delegations, escalations | **M** | No baseline/method model. RFC says not to claim monetary value without a method; nothing claims it. | |

## 3. Existing primitives worth reusing

**Data on disk** (all per hostd dir, `crew/<id>/…` under `CrewStore`, `store/mod.rs:22-28`)
- `crew.json` → `CrewSettings` (`store/mod.rs:496-527`): every new field is `#[serde(default)]`, so adding fields is non-breaking; load tolerates damage (`recover_crew_settings`, `hub.rs:set_paused`).
- `crewmate.json` → `Crewmate` (`store/mod.rs:262-295`): `#[serde(default)]` on all later fields; legacy `bot.json` / `approval` alias handled (`:31-32`, `:271-273`); `triggers: Vec<Trigger>` with a tagged enum (`#[serde(tag="type")]`, `:76-128`).
- `tasks.jsonl` + `tasks.idx` (last state per id wins; compaction, `store/tasklog.rs`), `usage.jsonl` (+ gz rotation), `audit.jsonl` (+ `.1…`), `crew-audit.jsonl` (crew-wide, outlives deletes), `memory.md`, `secrets.json` (0600), `<trigger>_seen.jsonl`, `github-deliveries.jsonl`, `telegram.json`, `push/tokens.json`.
- `Task` has spare structure for orchestration: `source`, `trigger_id`, `chain`, `origin`, `no_auto`, `restarts`, `resumed`, `waiting_for`, `action` (`store/mod.rs:358-447`).

**Wire / APIs**
- hostd NDJSON commands (`crew/mod.rs:101-251`): `crewmate_{list,create,update,delete,task,tasks,cancel,answer,action_answer,usage,audit,memory,memory_set,routine_set,triggers_set,secret_set}`, `crew_{audit,settings_get/set,pause,resume,telegram_*,github_*,engine_status}`; versioned by `PROTO=2` and `caps` (`protocol.rs:76-91`), additive-change rule documented (`protocol.rs:70-75`), golden fixtures guard shape.
- Push events (`events.rs`): `crewmate_task`, `crewmate_updated`, `crew_paused`, `crewmate_approval_unanswered`. A new event type is additive.
- Agent surface: loopback hook server (`agents/hook_server.rs`), per-crewmate token → `AGENT_ROUTES` allow-list; authorization checks the path id equals the token's crewmate (`hook_auth.rs:44-60`); MCP stdio shim `atlas-crew-mcp` (`mcp.rs`).
- Mobile relay: phone → Mac hostd → `crew_call{server,cmd,args}` on a crew server, scopes enforced (`relay/mod.rs`, 11 tests).

**Runtime behaviour**
- The single task funnel: `Task::new` → `append_task` → `runner::enqueue` → `run_worker` → `finish_task` → `triggers::on_task_finished`. Every trigger/source uses `triggers::fire` or a copy of it; any new orchestration (delegation, triage) can reuse `fire`'s order: pause check → store → `last_fired` → enqueue → broadcast (`triggers/mod.rs:460-540`).
- Durable-before-fire dedupe (GitHub seen file) and delivery-claim pattern (`app/webhook.rs:claim`): the template for an event inbox with idempotency.
- Kill switch (`paused`): checked by every path that could start work (`triggers/mod.rs:176,255`, `github.rs:67`, `github_hook.rs`, `runner/mod.rs:320`).
- Restart recovery (`recover.rs`: resume with cap 2).
- Typed reply target (`GithubOrigin`) + `Reporter`: the model for "report back to the originating channel".
- Untrusted-text discipline (`untrusted_event`, `author_allowed`, `no_auto`, `rail`) and the approval engine (`approval.rs`): the authority primitives for RFC §15; any agent-to-agent text must pass `untrusted_event`.
- Audit writer with redaction and rotation (`audit/mod.rs`), recorders keyed by `kind` string: free to add kinds (`chain_refused` pattern).
- Isolation: one Linux user per crewmate, so "agents communicate without sharing unrestricted memory" is already enforced at the OS level; hostd is the only broker.

**Security boundaries**
- A crewmate's token opens only its own routes (`hook_auth.rs`), body ≤1 MiB, GitHub route ≤2 MiB with signature before parse (`github_hook.rs`), webhook token in path, Telegram owner-chat-only approvals, voice answers refused for untrusted chats.

## 4. Gaps and smallest evolution of an existing primitive

| RFC need | Smallest evolution |
|---|---|
| Organization entity (§6.1) | Extend `CrewSettings` (crew.json) with optional `organization: {name, mission, description}`; keep one crew server = one organization. Cross-server org (several servers) can stay in the desktop's `crew-servers.json`. No new file. |
| Departments / role definitions | Add optional `department: Option<String>` and `responsibilities: Option<…>` to `Crewmate` (serde default), and render them into `task_prompt` next to `role` (`runner/prompt.rs:226`). A department is a tag first; a separate entity only when it needs its own policy. |
| Per-crewmate pause / disable / archive | New `Crewmate.status` (`active|paused|archived`, default `active`) checked in `enqueue`/`tick_schedule`/`fire`, mirroring the global `is_paused` call sites. |
| Agent delegation (§10) | A new `AGENT_ROUTES` entry `delegate` (and MCP tool) that hostd handles by building a normal `Task::new("delegation", …)` for the recipient through `fire`-style enqueue. Add to `Task`: `parent: Option<{crewmate, task}>`, `delegated_by`, `depth`. Guardrails reuse `chain` (cycle) plus a max depth and per-crewmate max-queue. Result returns by extending `on_task_finished` to fill the parent's waiting action (`TaskAction`-style card or a new `WaitingChild`). Output is wrapped via `untrusted_event` (agent text is not trusted by the recipient). |
| Message envelope / durable mailbox (§9) | Not needed for first cut: a delegation *is* a task with a durable record. A separate `messages.jsonl` per crewmate (copy of the `tasks.jsonl` store) only when non-task messages (broadcast, escalate) are required. |
| Routing by role/capability | A hostd-side `resolve_recipient({role?, department?, capability?})` that reads `list_crewmates` + `queue_len`; the data exists (`crewmate_json`). Needs the role/department fields above. |
| Chain handoff with data | Add `{{result}}`/`{{task_id}}` placeholders to `render_prompt` for `Crewmate` triggers (`on_task_finished` already holds the finished task's id), wrapped as untrusted. |
| Event normalization / triage (§12.2, 12.3) | Give `Event` a typed `{type, source, key, text}` and pass it through one `fire` gate that (a) dedupes by `(trigger, key)`, (b) applies cooldown / max-per-hour from new optional `Trigger` fields, (c) can call a cheap "relevance" turn like `summary.rs` before enqueuing a full run, writing an audit `trigger_skipped{reason}` (scenario C's "ignored with an auditable outcome"). Today only GitHub has keys. |
| Missed-run policy | Add `Trigger.missed: skip|run_once|catch_up` (default = today's behavior, `run_once`); implement in `is_due` callers (`triggers/mod.rs:176-246`). |
| Priorities / queue bound | `Task.priority: Option<u8>` and an insertion rule in `push()` (`runner/mod.rs:166`); queue cap with an audit `task_dropped`. |
| Goals (§13) | Smallest: a `goals.json` in the crew dir (versioned list: id, owner crewmate, metric, target, cadence, status, history) read into the prompt for the owner, plus a `schedule` trigger that spawns the review task; progress evidence posted by the agent through a new hook route `goal_update`. Governed changes = audited `config_changed{field:"goal"}`. |
| Budgets | Aggregate `usage.jsonl` per crewmate/day in hostd (the data exists) and check it in `fire`/`push` before enqueueing (refuse + audit + notify); add `CrewSettings.budget_usd_day` and `Crewmate.budget_*` optional. Depends on engines reporting `cost_usd`. |
| Memory scopes / knowledge records (§14) | Keep `memory.md` as the private scope. Add hostd-owned `knowledge/<scope>.jsonl` (records: id, text, source{crewmate, task}, ts, confidence, expires, status) with hook routes `knowledge_add` / `knowledge_search` scoped by the caller's token; reuse the `jsonl` store helpers. Prompt injects only top-N for the crewmate's scopes. Record provenance (task id) automatically from the pane key. |
| Memory audit | `set_memory` and a diff of `memory.md` after each task, written to the audit as `memory_changed{task_id, bytes, sha}`. |
| Notification decision (§16.2) | Replace the two hard-coded sends in `finish_task` (`outcome.rs:211-262`) with one `notify(report)` function holding the policy (category, quiet hours from `CrewSettings`, dedupe by key, rate limit); keep Telegram/push as sinks. A `Report{category,title,summary,task_id,decision_required}` struct built from `Task` fields that exist. |
| Digest | A schedule-triggered hostd job (no agent turn) that reads tasks/audit/usage and sends one `Report` through the same `notify`; or an agent turn with a new read-only hook route `crew_activity` (scoped, redacted). |
| Value report | Server-side `crew_value{since}` aggregating tasks by state/source/crewmate, usage, approvals (`approval_decision.wait_ms`), delegations (once `parent` exists). Time saved needs an explicit per-task baseline field, else report only measured quantities. |

## 5. Compatibility constraints and migration risks

- **On-disk**: `crewmate.json` and `crew.json` are read with `serde(default)` tolerance, so added optional fields are safe forward; an *older hostd reading a newer file* ignores unknown fields (no `deny_unknown_fields` on these types; it is used only in capture/locate/uigate specs, grep). However an old hostd that **saves** a crewmate (e.g. `set_profile`, `crewmate_update`) rewrites the file and **drops fields it does not know** [INFERENCE: serde struct round-trip]. A downgrade after adding fields silently loses them.
- `tasks.jsonl` records are full `Task` snapshots, last line per id wins; compaction past 4 MiB rewrites with the current struct, so unknown fields (`parent`, `priority`) are dropped by an old reader that compacts. Keep new fields `skip_serializing_if` default so old logs are byte-stable.
- **Archive**: `FORMAT_VERSION = 1` (`archive.rs:30`); import refuses unknown formats and never exports `<trigger>_seen.jsonl` or sessions. New artifacts (knowledge, goals, messages) need either a new entry type with a version bump (older import errors `UnsupportedFormat`) or must be left out of the archive explicitly. Currently "there is no other kind of entry" (`archive.rs:5`).
- **Wire**: `PROTO=2`, additive changes need no bump (`protocol.rs:70-75`), `crewmate_list` shape is golden-tested (`protocol_golden`, mobile fixtures `crewmate_task_event.json`); the mobile app (`kMinHostProto`) and desktop read `state`, `waiting_task`, `triggers[]`. New `TriggerKind` variants are the risky one: a client that deserializes `triggers[].kind.type` strictly (the Dart/TS models were not read by me) may fail on an unknown type [U: client parsing not verified here]. New `TaskAction` kinds have the same issue for cards (`action.kind`).
- **Old crew servers**: servers are updated independently (`atlas crew update`); clients check `info.caps` for `"crew"` only. Add a new cap (e.g. `crew_org`) before relying on organization commands, and keep `crewmate_*` commands intact. Servers older than alpha.26 already lack `waiting_task`/`paused` and clients tolerate it.
- **Trigger formats**: legacy `bot` trigger alias and `routines` still deserialize (`store/mod.rs:104-108`, `Routine`); two schedulers run today (`schedule::tick` for routines, `tick_schedule` for triggers); a new missed-run policy must be applied to both or routines should be migrated.
- **Webhook URLs**: `/crew/<id>/hook/<token>` and `/bots/…` are in external systems and must keep working (`webhook.rs:14-15`).
- **Pane key** `crew:<crewmate id>:<task id>` is parsed in several places (`pane.rs`); delegated tasks reuse it, so no format change is needed.
- **Security regressions to avoid**: agent-originated text must never bypass `untrusted_event`/`no_auto` (a delegated task from an agent that read a hostile issue inherits its taint: `no_auto` must propagate to the child, which no current field carries across tasks); a delegation route must be rate limited and depth limited before it exists, because a crewmate's token is the only credential and its process controls everything it sends (`hook_auth.rs:scrub` comment).
- **Isolation**: any shared knowledge store must live under hostd ownership; crewmate users can read only their own dirs. Memory files for isolated crewmates are written in place with no-follow helpers; don't replace that.
- **Mobile**: push `kind` set is closed (`task_done`, `needs_you`, `agent_*`); new kinds must be added to the relay and the app. Push is not live, so a design must not depend on it.
- **Concurrency pitfall**: handlers do load-modify-save of `crewmate.json` without a lock (see the stale-snapshot comments `triggers/mod.rs:218-222`, `schedule.rs:130-134`); adding more writers (goal updates, status flips) raises lost-update risk. Prefer separate files per concern.

## 6. Dependencies between capabilities

1. Foundation (no deps): crewmate `department`/`responsibilities` + per-crewmate `status` + audit of `set_profile`/memory changes.
2. Event gate (`fire` with typed event, dedupe key, cooldown, skip audit, per-trigger budgets/missed policy) → needed before more autonomous triggers (heartbeat, deadline, goal review) are safe.
3. Aggregated usage/budgets → needed before unattended proactive work at scale (budget check in the gate) and before value reporting.
4. Delegation (parent/depth/owner on `Task`, agent route, result return, `no_auto` propagation) → needs (1) for routing by role and (2) for rate/depth bounds; handoff-with-data depends on it or on `{{result}}` placeholders.
5. Notification policy + `Report` (single `notify` sink) → needs (4) for "parent receives result" and aggregation; digest needs it plus read access to activity. Push relay deployment is an external dependency for the mobile channel only.
6. Knowledge store (scoped, provenance) → needs task/pane identity (exists) and (1) for department scope; memory governance (expiry, retraction) depends on it.
7. Goals → need (2) for review triggers, (3) for budgets, (6) for evidence, (5) for reports; goal hierarchy depends on departments (1).
8. Value report → needs (3), (4), (5) and a baseline convention.

## 7. Open questions

1. Does the organization span several crew servers (the desktop's `crew-servers.json`) or is one server the organization? This decides where `organization` metadata and cross-agent messaging live (hostd cannot talk to another hostd today).
2. May an agent create tasks for another crewmate without human approval? Today every agent-initiated change is a human card (`add_trigger`). The RFC's scenario B implies autonomy; policy default and limits need an owner decision.
3. Which unit is "a role"? A reusable definition shared by several instances, or a label on each crewmate?
4. Should webhooks get HMAC/idempotency keys, and a full-payload event instead of 200 chars? (GitHub already has both.)
5. Missed-run: is "fire once after downtime" the intended contract for daily jobs, or should the default be skip?
6. Is the dual scheduler (`routines` + `triggers`) to be retired?
7. Should failures push to the phone? Today only `done` pushes (`outcome.rs:240`).
8. Do mobile and desktop clients tolerate unknown `triggers[].kind.type` and `action.kind` values? (not verified: client code outside my scope).
9. Memory: should the 64 KiB cut keep the head or the tail, and who curates it once more than one writer exists?
10. Cost: which engines actually report `cost_usd` and tokens in headless mode (affects budget enforceability)?
