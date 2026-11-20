# Dependency graph

Which capability must exist before which. IDs are the recommendations in [recommended-architecture.md](recommended-architecture.md). An arrow `X --> Y` means "Y needs X". Solid nodes are in the `1.5.0-alpha.29` milestone; the R nodes are on the roadmap.

```mermaid
flowchart LR
  F1[F1 clients tolerate unknown values]
  A1[A1 policy as data]
  A2[A2 project profiles]
  A3[A3 autonomy only on askable engines]
  A4[A4 egress per crewmate + browser sandbox]
  A5[A5 approval records + hash]
  A6[A6 per-crewmate status]
  D1[D1 run record]
  D2[D2 tool-call journal + recovery turn]
  D3[D3 command ids]
  W1[W1 event gate]
  W2[W2 delegation]
  W3[W3 budgets]
  W4[W4 dev environment + evidence]
  N1[N1 notification policy]
  N2[N2 run events + attention + review card]
  O1[O1 audit fields]

  F1 --> D1
  F1 --> A6
  F1 --> N2
  A1 --> A2
  A1 --> A3
  A1 --> A4
  A1 --> A5
  A1 --> D2
  A1 --> O1
  D1 --> A5
  D1 --> D2
  D1 --> W1
  D1 --> O1
  W1 --> W2
  W1 --> W3
  W1 --> N1
  A1 --> W2
  D1 --> W2
  W3 --> W2
  D1 --> W3
  N1 --> W3
  A2 --> W4
  D2 --> W4
  W4 --> N2
  D1 --> N2

  subgraph Roadmap
    R1[R1 organization + teams]
    R2[R2 goals]
    R3[R3 org memory]
    R4[R4 digests + value]
    R5[R5 routing / coordinator]
    R6[R6 organization architect]
    R7[R7 capability registry + external MCP]
    R8[R8 secrets broker]
    R9[R9 AG-UI inbound engine]
    R10[R10 AG-UI outbound SSE]
    R11[R11 dry run + evals]
    R12[R12 streaming + revisioned sync]
    R15[R15 durable approvals]
    R16[R16 wrapper for non-asking engines]
  end
  A1 --> R1
  W2 --> R1
  W1 --> R2
  W3 --> R2
  R3 --> R2
  D1 --> R3
  O1 --> R3
  N1 --> R4
  W3 --> R4
  R1 --> R5
  W2 --> R5
  A2 --> R6
  R1 --> R6
  A1 --> R7
  A1 --> R8
  N2 --> R9
  N2 --> R10
  D2 --> R11
  W1 --> R11
  N2 --> R12
  A5 --> R15
  A3 --> R16
```

## Why each edge exists

| Edge | Reason |
|---|---|
| F1 before D1, A6, N2 | They add fields and enum values that old desktop and mobile builds read. A strict decoder would crash or hide the board. |
| A1 before A2 | A project profile is a policy file plus a descriptive file; without a policy engine the profile is only text in the prompt. |
| A1 before A3 | "Automatic origin" is a policy input; the refusal of unattended engines is a policy decision. |
| A1 before A4 | Egress rules per crewmate are read from the same policy files. |
| A1 and D1 before A5 | An approval record names the task (D1) and the rule that held it (A1). |
| A1 and D1 before D2 | The journal is per task (D1); `external` classification uses policy labels (A1). |
| D1 before W1 | The gate writes taint and lineage into the task it creates. |
| W1 before W2, W3, N1 | Delegation, budgets and the quiet outcome are all decisions made at the gate. |
| W3 before W2 | A delegated task spends from a carve of its parent's budget; without budgets, delegation has no spend bound. |
| N1 before W3 | A budget stop must reach a person; the notification policy is where that is guaranteed. |
| A2 and D2 before W4 | The services a crewmate may start come from the profile; the review evidence (external calls, PR) comes from the journal. |
| W4 before N2 | The review card shows the review object W4 records. |
| W2 before R1, R5 | Teams and routing are only useful once one crewmate can hand work to another. |
| N2 before R9, R10, R12 | AG-UI and streaming are projections of typed run events; without them there is nothing to project. |

## Critical path

`F1 -> D1 -> W1 -> N1 -> W3 -> W2` is the longest chain inside the milestone. `A1 -> A2 -> W4 -> N2` runs in parallel with it after A1. A1 and F1 have no dependencies and start first; D3, A6 and O1 are short and can run beside anything once their single parent lands.
