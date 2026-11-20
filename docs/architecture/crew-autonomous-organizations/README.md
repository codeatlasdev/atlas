# Atlas Crew: autonomous organizations, architecture baseline

Verified architecture baseline for the RFC [Atlas Crew: Autonomous Organizations and Agent Operating System](rfc.md), taken on `main` at `72b47fc3` (2026-11-20). Research only: no production code was changed to write it. Every capability is classified from source code and, where possible, from tests that were run.

## The short version

- Atlas Crew already has the core of an agent operating system: hostd owns every agent process, decides every tool call of engines that can ask, isolates each crewmate as its own Linux user with its own cgroup and firewall chain, persists and recovers tasks, audits decisions, and reaches people on desktop, phone and Telegram.
- It lacks five things around its task funnel: **policy as data**, a **run record** that knows its parent and what it did outside, a **gate** in front of every automatic start, **delegation** through hostd, and **one notification policy**. Budgets, project profiles, an egress rule per crewmate and a sandboxed browser complete what a crew needs to work alone on a repository.
- None of it needs a new database, queue, workflow engine or protocol bump.
- `1.5.0-alpha.29` delivers that for four real repositories (atlas, ac, zelvo, wi), with merge, release and deploy kept as human steps enforced by policy. Organization, goals, organization memory, interop and evaluation follow on the roadmap.

## Documents

| Document | What it answers |
|---|---|
| [current-state.md](current-state.md) | What exists today, layer by layer, with file and test evidence, defects found, and the compatibility constraints any change must respect |
| [capability-matrix.md](capability-matrix.md) | Every RFC capability with its status (Implemented and verified, Implemented incomplete, Prototype, Planned, Missing, Unknown) and the RFC acceptance scenarios A to H |
| [architecture-gap-analysis.md](architecture-gap-analysis.md) | The gaps ranked by what they block (S1 to S3) and the recommendation that closes each |
| [dependency-graph.md](dependency-graph.md) | What must exist before what, and the critical path |
| [recommended-architecture.md](recommended-architecture.md) | The smallest evolution, as recommendations with what exists, what is missing, why, dependencies, alternatives, risks and acceptance criteria |
| [implementation-sequence.md](implementation-sequence.md) | The phases of `1.5.0-alpha.29`, the end-to-end acceptance, and what follows |
| [open-questions.md](open-questions.md) | Decisions for the maintainer, with a default recommendation each |
| [adr/](adr/README.md) | Proposed architecture decisions |
| [rfc.md](rfc.md) | The RFC this baseline answers |

## Raw evidence

| Report | Scope |
|---|---|
| [research/audit-runtime-authority.md](research/audit-runtime-authority.md) | Host authority, security, durable execution, agent runtime (about 24 filtered test runs) |
| [research/audit-organization-work.md](research/audit-organization-work.md) | Organization model, delegation, triggers, goals, memory, reporting (about 330 tests run) |
| [research/audit-surfaces-ax.md](research/audit-surfaces-ax.md) | Desktop, mobile, agent experience, MCP, wire protocol, observability (413 Rust and 833 desktop tests run) |
| [research/dogfood-targets.md](research/dogfood-targets.md) | atlas, ac, zelvo and wi: stack, gate, deploy, secrets, risks and readiness for a crew |
| [research/study-ag-ui.md](research/study-ag-ui.md) | The AG-UI 1.0 protocol and how Atlas Crew maps onto it |
| [research/study-monocode.md](research/study-monocode.md) | Monocode's integrations, UI and UX, and the lessons for Crew |

Related: the earlier product study of OpenBot, n8n Nod and agentes.chat (PR #669, `docs/studies/crew-agent-platforms/`).

## How to use this folder

Each work item in the `1.5.0-alpha.29` milestone links to its recommendation here. When an item changes the design, update the recommendation in the same pull request. When a roadmap item starts, write its design note in this folder first, with the same seven fields.
