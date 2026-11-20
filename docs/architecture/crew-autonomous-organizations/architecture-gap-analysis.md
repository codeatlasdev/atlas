# Architecture gap analysis

What stands between Atlas Crew today ([current-state.md](current-state.md)) and the RFC's autonomous organization, ranked by what each gap blocks. Baseline `main` at `72b47fc3`. Each gap links to the recommendation that closes it in [recommended-architecture.md](recommended-architecture.md).

## Severity

| Level | Meaning |
|---|---|
| **S1** | Blocks a crew from working alone on atlas, ac, zelvo or wi without a person watching, or is a security hole once work is automatic. |
| **S2** | Blocks an RFC acceptance scenario (A to H) but not unattended repo work. |
| **S3** | Quality, scale or experience; the product works without it. |

## 1. Host authority and security

| Gap | Evidence | Severity | Closed by |
|---|---|---|---|
| Policy is three labels; no rule can say "never deploy", "never touch this host", "never merge" | `crew/approval.rs:69-83`; `store/mode.rs` | S1 | A1 |
| `destructive()` only blocks voice approvals; `rm -rf` runs unasked in `bypass` | `approval.rs:112-226`, `voice_card.rs` | S1 | A1 defaults |
| No machine-readable project rules (gate, done, deploys, production hosts, destructive scripts) | research/dogfood-targets.md §6 | S1 | A2 |
| Automatic tasks run on engines whose tool calls hostd never sees (codex, opencode) | `crew/handlers/crewmates.rs:47-48`, `approval.rs:921` | S1 | A3 |
| omp crewmates do not get the atlas MCP tools | `crew/engine/launch.rs:182-196` | S2 | A3 |
| Egress is the same for every crewmate (TCP 22, 80, 443 to any public host) | `crew/spawn/provision.rs:409-415` | S1 | A4 |
| Capture's Chromium runs without its sandbox as the crew user (N4) | `crew/capture/runner.mjs:263-266`, `docs/crew.md` N4 | S1 | A4 |
| Approvals not persisted, no hash of the action, task grants without expiry, device class instead of the person | `agents/mod.rs:432-522`, `approval.rs:261-272, 476-518` | S2 | A5 |
| Secrets delivered as environment variables, plaintext JSON at rest, reads not audited | `crew/store/mod.rs:964-1010`, `conversation.rs:196-203` | S2 | R8 (audit of reads in O1) |
| No revocation finer than "pause everything" and "revoke bypass" | `hub.rs:114-137`, `approval.rs:992-1004` | S1 (stopping one misbehaving crewmate at night) | A6, A1 |
| No CPU or IO limits; one global 2-hour timeout | `crew/spawn/cgroup.rs:306-314`, `engine/mod.rs:25` | S3 | W3 (wall time), R13 |
| Hook port can be squatted by a detached crewmate process (S-A) | `docs/crew.md` S-A | S3 | Tracked separately |

## 2. Durable execution

| Gap | Evidence | Severity | Closed by |
|---|---|---|---|
| No record of external actions already taken; a crash after `gh pr create` can repeat it | `crew/runner/recover.rs:6-9`, `runner/tape.rs` | S1 | D2 |
| No "outcome uncertain" state or recovery prompt | research/audit-runtime-authority.md §2 | S1 | D2 |
| No parent, child, depth or "waiting on" in the task record | `crew/store/mod.rs:359-447` | S2 | D1 |
| Mutating commands have no idempotency key; a phone retry can duplicate a task | research/audit-organization-work.md §2.2 | S2 | D3 |
| Terminal agents rerun from scratch after a restart | `recover.rs:6-9` | S3 | D2 journal in the resume prompt |
| Rollback is binary-only; newer files are rewritten by older structs | research/audit-runtime-authority.md §5 | S3 | Principle 5 (new files) |

## 3. Autonomous work loop

| Gap | Evidence | Severity | Closed by |
|---|---|---|---|
| No cooldown, rate or queue cap; a 60-second trigger enqueues without bound | `crew/triggers/mod.rs:176-246`, `runner/mod.rs` `VecDeque` | S1 | W1 |
| No dedupe outside GitHub; webhook body cut to 200 characters, no delivery id | `crew/triggers/webhook.rs:38-70` | S2 | W1 |
| Missed-run behaviour is implicit (fire once on boot) and not configurable | `crew/schedule.rs:68-70` | S2 | W1 |
| No "nothing to report" outcome; every run notifies | `outcome.rs:211-262` | S2 | W1, N1 |
| Two schedulers (`routines` and `triggers`) | `schedule.rs:90-148`, `triggers/mod.rs:370-385` | S3 | W1 migration |
| File triggers are never re-registered or removed after an edit until restart | `crew/triggers/file_watch.rs` `register_new` | S3 | Bug issue |
| No budgets of any kind; codex and omp record no cost, terminal agents no usage | `engine/codex.rs:129`, `engine/omp.rs:254` | S1 | W3 |
| No heartbeat, deadline, goal-review or condition triggers | `store/mod.rs:76-128` | S3 | R2 |

## 4. Coordination

| Gap | Evidence | Severity | Closed by |
|---|---|---|---|
| A crewmate cannot hand work to another or read the result | `crew/mcp.rs:166-174`, `agents/hook_auth.rs:16-25` | S1 for builder plus reviewer crews; S2 otherwise | W2 |
| Chain triggers pass a one-line event, no result or task id | `triggers/mod.rs:~270` | S2 | W2 placeholders |
| No routing by role, capability or load | `telegram.rs:~664-700`, `github_hook.rs` | S3 | R5 |
| No organization entity, teams, charters or goals | research/audit-organization-work.md §2.1, §2.4 | S2 | R1, R2 |
| One flat 64 KiB memory file per crewmate, unaudited, no provenance | `store/mod.rs:915-943`, `handlers/settings.rs:17-23` | S2 | O1 (audit now), R3 |

## 5. Reporting and experience

| Gap | Evidence | Severity | Closed by |
|---|---|---|---|
| Parked cards and `ask_user` notify nobody; failures do not push | research/audit-organization-work.md §2.6 | S1 | N1 |
| No cross-crew attention list with age | research/audit-surfaces-ax.md §2 | S2 | N2 |
| Steps are 80-character text, 50 per task, none for headless engines; `agent_step` read by no client | `outcome.rs:71-95`, `agents/mod.rs:355-372` | S2 | N2, bug issue |
| No result card with files, diff and gate result after a task | research/study-monocode.md §11.1 | S2 | W4, N2 |
| `crew_paused` and `crew_engine_auth` never reach phones | `mobile/mod.rs:456-471` | S2 | Bug issue |
| Docs contradict the code (mobile audit screen, `crew_paused` scope) | `docs/crew.md:817, 1003` | S3 | Bug issue |
| Mobile push relay not deployed | `docs/crew.md:520-524` | S3 for this plan (Telegram and the apps carry attention) | Outside this plan |

## 6. Observability and evaluation

| Gap | Evidence | Severity | Closed by |
|---|---|---|---|
| Audit lines lack `task_id`, person, rule and result; no fsync; the oldest file is deleted | `crew/audit/mod.rs:42-48, 150-170` | S2 | O1 |
| Agents cannot read their own history, usage or tasks | `crew/mcp.rs:345-470` | S2 | W2 (`my_tasks`, `task_status`) |
| No dry run, simulation or eval suite for agent behaviour | research/audit-surfaces-ax.md §2 | S2 | R11; the .29 plan adds a scripted end-to-end acceptance |
| Desktop crew tests are not self-contained (need `bun install`, mobile fixtures, Rust sources) | research/audit-surfaces-ax.md §1 | S3 | Bug issue |

## 7. Interop

| Gap | Evidence | Severity | Closed by |
|---|---|---|---|
| No external MCP servers per crewmate, no capability registry | `launch.rs:182-196`, `apps/desktop/src/lib/crew/integrations.ts` | S2 | R7 |
| No AG-UI, A2A or ACP | grep, research/study-ag-ui.md §5 | S3 | R9, R10 (AG-UI at the edge); A2A and ACP stay open questions |

## 8. The S1 list is the .29 critical path

Every S1 gap above is closed by a work item in the `1.5.0-alpha.29` milestone ([implementation-sequence.md](implementation-sequence.md)). The S2 items that the S1 work depends on (D1 run record, F1 client tolerance, O1 audit fields, N2 attention) come with it. Everything else is on the roadmap milestone.
