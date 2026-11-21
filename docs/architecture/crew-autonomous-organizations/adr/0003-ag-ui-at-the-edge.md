# ADR 0003: AG-UI at the edge, not as hostd's own protocol

**Status:** Proposed (2026-11-20)

## Context

AG-UI 1.0 (frozen 2026-09-17) is an event protocol between agent backends and user interfaces (research/study-ag-ui.md). hostd speaks its own NDJSON protocol (`PROTO = 2`) to the desktop and, through the relay, to phones.

## Problem

Should Atlas adopt AG-UI for its own clients, at the edge for third parties, or not at all?

## Decision drivers

- Atlas approvals and `ask_user` hold a live agent process; AG-UI has no mid-run input from the client and treats an interrupt as the end of a run.
- No 1.0 Dart SDK; Rust SDKs lag 1.0.
- `PROTO` must not be bumped for this.
- The ecosystem value (CopilotKit, agent frameworks, AgentCore) is real.

## Considered options

A. Edge adoption: an inbound `agui` engine kind, later an outbound SSE projection, with hostd-owned serde types checked against a hash-pinned schema.
B. Replace the native pushes with AG-UI.
C. An additive `agui` push on the NDJSON socket only.
D. No AG-UI; MCP and A2A only.

## Decision

A, after typed run events (N2) exist, so the projection is a pure function of them. B is rejected for now. C is allowed as a spike inside A.

## Consequences

Two vocabularies (native and AG-UI) maintained from one internal event module, with golden fixtures. Atlas-only concepts go in `metadata` or `CUSTOM` events with a `dev.atlas.` prefix.

## Security implications

Every SSE request is authenticated; read-scoped clients never receive approval content; `provide_secret` never travels as an interrupt payload; runs created from AG-UI input are marked untrusted until the owner trusts the client.

## Migration

None for existing clients.

## Open questions

Q19, Q20.
