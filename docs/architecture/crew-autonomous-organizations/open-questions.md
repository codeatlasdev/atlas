# Open questions

Decisions this baseline cannot take from the code. Each has the recommendation this study would follow if nobody decides, and the work item that waits on it. Questions marked **blocks .29** must be answered before the named item is merged.

## Product and scope

| # | Question | Recommendation if undecided | Waits |
|---|---|---|---|
| Q1 | Who is Crew for next: one developer, a small team, or people who do not code? (also asked in PR #669) | One developer running crews on their own repositories. Every item in .29 serves that; teams come with R1. | Roadmap order |
| Q2 | Is one crew server one organization, or can an organization span several servers (the desktop's `crew-servers.json`)? | One server is one organization until a real need for more appears. hostd cannot talk to another hostd today. | R1 |
| Q3 | Should "quiet" runs show anywhere (a counter on the crewmate), or vanish? | Count them on the crewmate and in the audit; never notify. | W1, N1 |

## Authority

| # | Question | Recommendation if undecided | Waits |
|---|---|---|---|
| Q4 | **blocks .29** May a crewmate delegate to another without the owner approving each delegation? | Default `hold` (a card per delegation); the owner adds `allow` rules for pairs they trust, such as builder to reviewer. | W2 |
| Q5 | **blocks .29** Which database does a zelvo crewmate use: a Neon branch per crewmate, or a local Postgres like CI's? | Local Postgres 18 in the crewmate's compose, seeded with `db:seed:qa`; never the shared Neon dev database. | W4, E1 |
| Q6 | **blocks .29** May a crewmate start `atlas dev` services for ac? ac's `AGENTS.md` says agents never start the stack. | No, until ac's maintainers change that rule. The ac profile lists no services. | A2, E1 |
| Q7 | Should the destructive defaults (`hold` on `rm -r`, force push, `DROP`) also apply to tasks the owner started in `bypass`? | Only to automatic, delegated and stranger origins. An owner who chose `bypass` for their own task keeps today's behaviour. | A1 |
| Q8 | Do codex and opencode crewmates keep running unattended on schedules, with an explicit opt-in, or are automatic tasks refused for them until a wrapper exists (R16)? | Refused unless the crew policy opts in, with a visible warning. | A3 |
| Q9 | Should approvals become durable interrupts (end the run, resume the engine session after the answer) instead of a live 30-minute hold? | Keep the live hold in .29 and record approvals durably (A5). Revisit with R15 once D2 exists. | R15 |
| Q10 | Is SSH denied by default for crewmates with automatic triggers (git over HTTPS through the GitHub App helper)? | Yes. | A4 |
| Q11 | Is per-crewmate secret delivery by environment variable acceptable for autonomous mode, given egress allows any public host on 443? | Acceptable for .29 with A4 host rules and O1 audit; the broker (R8) replaces it. | R8 |

## Durable execution

| # | Question | Recommendation if undecided | Waits |
|---|---|---|---|
| Q12 | Which actions count as external and non-idempotent for a software crew? | `git push`, `gh pr create`, `gh pr merge`, `gh pr comment`, `gh issue comment`, `gh api` with a non-GET method, `curl` or `wget` with a body or a non-GET method, plus anything a profile labels `deploy` or `external`. The list lives in hostd defaults plus profiles. | D2 |
| Q13 | Should a paused crew keep held approvals open, or cancel them as today? | Cancel, as today (pause means stop). | A6 |
| Q14 | Default missed-run behaviour for schedules after downtime: fire once (today) or skip? | Keep fire once as the default; offer `skip` per trigger. | W1 |

## Budgets and reporting

| # | Question | Recommendation if undecided | Waits |
|---|---|---|---|
| Q15 | Budgets in USD, tokens or wall time? | All three as optional limits; the UI states which engines report USD. | W3 |
| Q16 | Should crew show dollar estimates computed from token counts and model prices? | No. Only what the engine reports. | W3 |
| Q17 | Should failures and parked cards push to the phone? | Yes for both, with the existing privacy invariant. | N1 |
| Q18 | When does the push relay go live? Until then attention rests on the apps and Telegram. | Not a .29 dependency. | Outside |

## Interop and experience

| # | Question | Recommendation if undecided | Waits |
|---|---|---|---|
| Q19 | Who would consume an AG-UI stream of crew runs: dashboards in ac, zelvo or wi, CopilotKit embeds, Slack or Teams? | Build the inbound engine (R9) first; the outbound stream (R10) only with a named consumer. | R9, R10 |
| Q20 | Should a crewmate thread accept client-sent history (AG-UI's model), or only the newest message? | Only the newest message; hostd owns the session. | R10 |
| Q21 | Should the desktop crew tests keep reading `apps/mobile/test/fixtures`, or should fixtures move to a shared `testdata/` folder? | Move to `testdata/crew/` so every client reads one place. | B6 |

## Questions the audits could not answer from code

These need a run on a real crew server and are part of E1's checklist:

- Do headless-engine crewmates in production have hooks that report tool calls? Code says it depends on each engine's config; nothing in `atlas crew install` sets it (research/audit-surfaces-ax.md §7).
- Does a terminal claude crewmate stay in its own ask mode when hostd is unreachable (`atlas hook` never fails, research/audit-runtime-authority.md §7)?
- Are ports isolated between two worktrees of the same repository on one crew server when `atlas dev` runs for both?
