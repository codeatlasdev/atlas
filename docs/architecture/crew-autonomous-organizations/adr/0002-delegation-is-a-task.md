# ADR 0002: A delegation is a task, not a message

**Status:** Proposed (2026-11-20)

## Context

The RFC describes an agent communication protocol with message types, envelopes and routing (§9) and delegation with handoffs (§10). Atlas Crew has one way for agents to cause work for other agents: chain triggers, which pass a one-line event and no result (`crates/atlas-host/src/crew/triggers/mod.rs:250-340`).

## Problem

What is the smallest primitive that lets a crewmate hand work to another, wait for the result, and stay inside authority, budget and audit?

## Decision drivers

- Every unit of work already has a durable record, a queue, slots, recovery, audit and a board card: the `Task`.
- No new queue or broker (RFC §27.3).
- Authority must not widen through delegation (RFC §15.6).

## Considered options

1. A message bus with mailboxes and addressing.
2. Orchestration in the client, as Monocode does (runs stop when the window closes, research/study-monocode.md §11.3).
3. A delegation creates a normal task for the recipient, with `run.parent`, depth, inherited taint and a carved budget; the parent may park until the child finishes and then resumes with the child's reply.

## Decision

Option 3, through MCP tools on the existing atlas server (`delegate`, `task_status`, `my_tasks`) backed by per-crewmate hook routes.

## Consequences

- Delegations show up on the board, in the audit and in budgets with no new surface.
- Messages that are not work (broadcasts, escalations) are not covered; a mailbox can be added later as a second file per crewmate if needed.
- The parent's second turn reuses the parked-action flow (`crates/atlas-host/src/crew/runner/conversation.rs:145-157`).

## Security implications

The child gets the stricter of its own and its parent's policy. The objective is wrapped as untrusted text. Depth (max 3), the chain guard, the queue cap and a per-crewmate delegation rate bound loops and spam. The default policy holds every delegation for the owner (Q4).

## Migration

Additive: new optional `Task.run` fields and new MCP tools. Old clients see delegated tasks as normal tasks.

## Open questions

Q4 (default hold or allow).
