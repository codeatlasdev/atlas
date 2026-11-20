# Implementation sequence

How the recommendations in [recommended-architecture.md](recommended-architecture.md) are delivered: what goes in `1.5.0-alpha.29`, in which order, and what the milestone must prove at the end. The order follows [dependency-graph.md](dependency-graph.md). Tracking issues: #672 for `1.5.0-alpha.29` and #673 for the roadmap. The table below maps each work item to its issue.

## Goal of 1.5.0-alpha.29

> A crew on a Linux crew server takes an issue in atlas, ac, zelvo or wi, works on it in its own worktree under rules hostd enforces, hands the change to a reviewer crewmate, opens a pull request with the evidence the repository asks for, stays inside a budget, and reaches the owner only when a decision is needed. Merge, release and deploy stay with a person.

That is the RFC's "Agent Platform" step with the first piece of "Multi-Agent System" (§35), scoped to the four repositories the owner works on.

## Rules for every item

- Tests first: each issue names the regression or contract tests that must fail before the change (AGENTS.md "Tests").
- Wire changes are additive, behind a cap, with a phone scope and a fixture from the real serializer.
- New configuration goes in new files (principle 5).
- Each user-visible change has a `CHANGELOG.md` line and a `docs/crew.md` section (and the pt-BR mirror).
- No item adds a runtime dependency without saying why in its PR.
- Heavy commands run through `atlas run --heavy`.

## Phase 0: defects found by this audit (any time, in parallel)

Small, independent fixes. Each has its own issue.

| Item | What |
|---|---|
| B1 | `crew_paused` and `crew_engine_auth` reach phones (`push_scope`), docs aligned |
| B2 | `docs/crew.md` matches the code: mobile audit screen exists, scopes of pushes |
| B3 | Webhook trigger token compared in constant time, delivery id and full payload size documented |
| B4 | Headless-engine tasks record steps (claude, codex, opencode, omp) |
| B5 | File triggers follow edits: re-register on change, stop when disabled or removed |
| B6 | Desktop crew tests run from a clean checkout (documented `bun install`, explicit fixture paths) |

## Phase 1: foundations (start together)

| Item | Depends on | Size [INFERENCE] |
|---|---|---|
| F1 Clients tolerate unknown values (desktop and mobile) | none | M |
| A1 Policy as data in front of the mode | none | L |
| D3 Command ids on mutating commands | none | S |
| O1 Audit fields, durability, memory and profile audit | A1 for `rule` (field can land empty first) | M |

## Phase 2: authority and durable runs

| Item | Depends on | Size |
|---|---|---|
| D1 Run record on every task | F1 | M |
| A6 Per-crewmate status (pause, archive) | F1 | M |
| A2 Project profiles and the four starting profiles | A1 | L |
| A3 Autonomy only on askable engines, MCP for omp | A1 | M |
| A5 Approval records bound to the action | A1, D1 | M |
| D2 Tool-call journal, uncertain outcomes, recovery turn | A1, D1 | L |

## Phase 3: the autonomous loop

| Item | Depends on | Size |
|---|---|---|
| W1 Event gate, routines migration | D1 | L |
| N1 Notification policy | W1 | M |
| W3 Budgets, usage for codex, omp and terminal agents | W1, D1, N1 | L |
| A4 Egress per crewmate and browser sandbox (N4) | A1 | L |

## Phase 4: working as a crew

| Item | Depends on | Size |
|---|---|---|
| W2 Delegation through hostd | W1, D1, A1, W3 | L |
| W4 Crewmate dev environment and review evidence | A2, D2 | L |
| N2 Run events, attention inbox, review card | F1, D1, W4 | L |

## Phase 5: acceptance (closes the milestone)

One issue, **E1**, owns the end-to-end proof. It is done when all of these pass on a real Linux crew server with the isolation helper installed, recorded in the issue with logs and captures:

1. **atlas.** A builder crewmate is assigned an issue labelled for the crew. It works in its worktree, runs `bun run check:changed` and the gate through the heavy queue, delegates review to a reviewer crewmate (W2), addresses the review, opens a PR with a CHANGELOG line and the gate result in the review object (W4). Attempts to `git push origin main`, `gh pr merge` and `bun run release` are denied by policy and audited (A1, A2).
2. **wi.** A crewmate fixes an issue with its acceptance list, runs `test`, `test:integration` with `docker-compose.test.yml`, and opens a PR. `bun run deploy` and any command that reaches the production host are denied (A2, A4).
3. **zelvo.** A crewmate starts `redis`, `server` and `web` for its own worktree (W4), runs the e2e it needs against its own database (open question Q5 decides which), and the services stop at task end. `db:reset` and `bun run deploy` are denied.
4. **ac.** The ac profile forbids starting the dev stack, so the crewmate does code and unit-test work only and reports the stack as a blocker when a task needs it (ac's own rule, research/dogfood-targets.md §2). `git push origin main` is denied.
5. **Recovery.** hostd is restarted while a crewmate runs `gh pr create`; after the restart no second PR exists and the task shows the uncertain call as checked (D2).
6. **Budget.** A crewmate with a daily limit stops at it, parks with a budget card, and the owner gets one notification (W3, N1).
7. **Quiet.** A scheduled triage crewmate with nothing to do ends `quiet` and nobody is notified (W1, N1).
8. **Old clients.** The previous alpha's desktop and mobile builds still list crewmates and tasks against the new server (F1).
9. **Kill.** Pausing one crewmate from the phone stops its task within seconds and leaves the others working (A6).

## After 1.5.0-alpha.29

The roadmap milestone holds the R items of [recommended-architecture.md](recommended-architecture.md) §4 in dependency order: organization and teams (R1), organization memory (R3), goals (R2), digests and value (R4), routing and a coordinator (R5), the organization architect (R6), the capability registry and external MCP servers (R7), the secrets broker (R8), AG-UI at the edge (R9, R10), dry runs and evals (R11), streaming and revisioned sync (R12), CPU and IO limits (R13), a tamper-evident audit (R14), durable approvals (R15) and a wrapper for non-asking engines (R16). Each R issue is refined into its own design note in this folder before work starts, using the same seven fields.

## Issue map

| Item | Issue | Item | Issue |
|---|---|---|---|
| A1 | #681 | A2 | #686 |
| A3 | #687 | A4 | #693 |
| A5 | #688 | A6 | #685 |
| B1 | #674 | B2 | #675 |
| B3 | #676 | B4 | #677 |
| B5 | #678 | B6 | #679 |
| D1 | #684 | D2 | #689 |
| D3 | #682 | E1 | #697 |
| F1 | #680 | N1 | #691 |
| N2 | #696 | O1 | #683 |
| W1 | #690 | W2 | #694 |
| W3 | #692 | W4 | #695 |
| R1 | #698 | R2 | #700 |
| R3 | #699 | R4 | #701 |
| R5 | #702 | R6 | #703 |
| R7 | #704 | R8 | #705 |
| R9 | #706 | R10 | #707 |
| R11 | #708 | R12 | #709 |
| R13 | #710 | R14 | #711 |
| R15 | #712 | R16 | #713 |
