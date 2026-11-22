# ADR 0004: No new task states or enum values before clients tolerate unknown ones

**Status:** Proposed (2026-11-20)

## Context

`TaskState` has six values (`crates/atlas-host/src/crew/store/mod.rs:340-347`). The RFC's run model needs more (waiting for approval, recovering, outcome uncertain, paused). Desktop and mobile decode task state, card kinds, trigger kinds and crewmate state; whether they survive unknown values was not verified. An older hostd that reads a task line with an unknown state parks it as unknown and the task disappears from its view.

## Problem

How to grow the run model without breaking old clients and rolled-back servers?

## Decision drivers

- Old phones and desktops talk to new servers for weeks.
- Rollback must keep working (`atlas crew update --rollback`).

## Considered options

1. Add states now and bump `PROTO`.
2. Never add states; put everything in side objects forever.
3. First make clients tolerant (F1), carry new information in an optional `Task.run` object meanwhile, and add enum values only in a later release once tolerant clients are the minimum.

## Decision

Option 3.

## Consequences

The first phase uses `run.waiting_on` instead of a `waiting_approval` state. A later release may promote it to a state once the minimum supported client is tolerant.

## Security implications

None directly; it avoids a class of crashes that would hide pending approvals from the owner.

## Migration

F1 ships contract tests in both clients using real fixtures with an unknown value.

## Open questions

When the minimum supported client version moves past F1 (release policy decision).
