Raw evidence for docs/architecture/crew-autonomous-organizations/ (2026-11-20, main 72b47fc3).

The `/tmp/...` logs and clones cited below were local to the audit run and were not kept.

# AG-UI (Agent-User Interaction Protocol) and Atlas Crew

Study of 2026-11-20 for the RFC "Atlas Crew: Autonomous Organizations & Agent Operating System" (§20 interop, §24.6 agent protocols and UI, with the event needs of §11 runs, §15.5 approval binding, §16 notifications, §17 UX and §21 audit). Research only: no file in any repository was changed.

**How to read the citations**
- `AGUI:path:line` is the AG-UI repo at commit `1de7b128` (2026-11-20 17:12 +0100), cloned to `/tmp` and deleted after the study: `https://github.com/ag-ui-protocol/ag-ui/blob/1de7b12898f2aadd17fb0f03cbbe96dd2c07ddc9/<path>`.
- `atlas:path:line` is `codeatlasdev/atlas` `main` at `72b47fc3` (2026-11-20).
- Spec pages are cited by their `docs.ag-ui.com/spec/1.0/...` URL. They are the same files as `AGUI:docs/spec/1.0/...`; the live `/spec/1.0/changelog` was fetched and matches the repo.
- `[INFERENCE]` marks a claim I did not verify.

**What I ran**
- I validated 13 sample Atlas→AG-UI events against the pinned `spec/1.0/schema.json` with Python `jsonschema` (Draft 2020-12). 11 passed. The two meant to fail were rejected: an interrupt outcome with `interrupts: []`, and `subagentRunId: null`. So the mapping in §6 uses valid shapes, and the schema's "absent means absent" and "at least one interrupt" rules are enforced as the docs say.
- I ran no Atlas tests. Every Atlas claim comes from reading the source at the lines cited.

---

## 1. What it is (in one paragraph)

AG-UI is an open protocol (MIT) for the link between an agent backend and a user-facing app. The app sends one `RunAgentInput` (thread, run, messages, frontend tools, context, state, forwarded props, resume answers). The agent sends back one ordered stream of typed JSON events: lifecycle, text, tool calls, reasoning, state snapshots and JSON-Patch deltas, activity, subagents, and raw/custom ([architecture](https://docs.ag-ui.com/spec/1.0/architecture), `AGUI:docs/spec/1.0/architecture.mdx`). CopilotKit created it and it is "developed in the open with the community" (`AGUI:README.md:248`). It describes itself as the third protocol next to MCP (agent to tools/context) and A2A (agent to agent) (`AGUI:docs/agentic-protocols.mdx`). It is not a generative-UI spec. It carries A2UI, MCP Apps/MCP-UI and Open-JSON-UI payloads (`AGUI:docs/concepts/generative-ui-specs.mdx`).

Status on 2026-11-20:
- **Protocol 1.0** was frozen on 2026-09-17 (commit `fdbca490` "feat(spec): freeze the draft as 1.0"). `spec/1.0/schema.json` has not changed since: one commit in its history (GitHub API).
- A **1.1 draft** is open. So far it only adds optional fields: `encryptedValueType` (#2888) and `TOOL_CALL_RESULT.error` (#3232) (`AGUI:docs/spec/draft/changelog.mdx`).

---

## 2. Spec details

### 2.1 Structure of the spec
- The JSON Schema (`spec/1.0/schema.json`, 98 `$defs`, Draft 2020-12, every object closed with `unevaluatedProperties: false`) is authoritative for **structure**. The prose spec, in BCP 14 language, is authoritative for **behaviour**: ordering, lifecycle, attribution, errors, compatibility ([spec index](https://docs.ag-ui.com/spec/1.0); `AGUI:spec/README.md`).
- Roles: **producer** (emits events) and **consumer** (reads them). Conformance is judged per stream ([spec index](https://docs.ag-ui.com/spec/1.0)).
- **Tolerant reader, strict writer.** A producer emits only what the schema defines. A consumer drops unknown event types, strips unknown properties and union members with a warning, and treats a malformed known value as fatal ([processing](https://docs.ag-ui.com/spec/1.0/basic/processing), `AGUI:docs/spec/1.0/basic/processing.mdx:12,63-71`).
- **Absent means absent.** An optional field is omitted, never `null`. A `null` inside open-by-key objects (metadata, state) is data and is kept ([event model](https://docs.ag-ui.com/spec/1.0/basic)).
- The **conformance corpus** has 69 stream fixtures, replayed into the TS and .NET clients (`AGUI:spec/1.0/conformance/README.md`), plus 43 per-definition schema fixture folders (`AGUI:spec/1.0/fixtures`).

### 2.2 Envelope and identifiers
| Field | Req. | Meaning |
|---|---|---|
| `type` | yes | one of 31 `EventType` values |
| `timestamp` | no | informational (ms epoch by convention). Arrival order is the protocol's order |
| `rawEvent` | no | the provider-native event, verbatim. Consumers MUST NOT derive behaviour from it |
| `metadata` | no | open by key, last write wins per key, no recursion. The `ag-ui` key is reserved ([metadata](https://docs.ag-ui.com/spec/1.0/basic/metadata)) |
| `subagentRunId` | no | on "attributable" events only, never on `RUN_*` or `MESSAGES_SNAPSHOT` |

Identifiers ([event model](https://docs.ag-ui.com/spec/1.0/basic)):
- `threadId`: minted by the app, stable across runs.
- `runId`: never reused on a thread.
- `messageId`: unique within the thread; crosses runs.
- `toolCallId`: ties together the call, its result and any interrupt about it.
- `subagentRunId`: one invocation, not one agent.
- All IDs are opaque strings.

### 2.3 All 31 events (field lists extracted from `spec/1.0/schema.json`; `?` = optional; envelope fields omitted)

| Family (pattern) | Event | Fields |
|---|---|---|
| Lifecycle | `RUN_STARTED` | `threadId`, `runId`, `protocolVersion?`, `parentRunId?`, `input?: RunAgentInput` |
| | `RUN_FINISHED` | `threadId`, `runId`, `result?`, `outcome?: success{pendingToolCallIds?} \| interrupt{interrupts[≥1]} \| cancelled`, `usage?: TokenUsage[]` |
| | `RUN_ERROR` | `message`, `code?`, `usage?` |
| | `STEP_STARTED` / `STEP_FINISHED` | `stepName` (matched by name, per owner; steps may overlap) |
| Text (streaming) | `TEXT_MESSAGE_START` | `messageId`, `role?` (developer/system/assistant/user; absent = assistant), `name?` |
| | `TEXT_MESSAGE_CONTENT` | `messageId`, `delta` |
| | `TEXT_MESSAGE_END` | `messageId` |
| | `TEXT_MESSAGE_CHUNK` | `messageId?`, `role?`, `delta?`, `name?` (compact form, expanded by the client) |
| Tool calls (streaming) | `TOOL_CALL_START` | `toolCallId`, `toolCallName`, `parentMessageId?` |
| | `TOOL_CALL_ARGS` | `toolCallId`, `delta` (argument text, not validated) |
| | `TOOL_CALL_END` | `toolCallId` |
| | `TOOL_CALL_CHUNK` | `toolCallId?`, `toolCallName?`, `parentMessageId?`, `delta?` |
| | `TOOL_CALL_RESULT` | `messageId`, `toolCallId`, `content: string \| ContentPart[]`, `role?: "tool"` (+ `error?` in draft 1.1) |
| Reasoning (streaming) | `REASONING_START` / `REASONING_END` | `messageId` (a span) |
| | `REASONING_MESSAGE_START` | `messageId`, `role: "reasoning"` |
| | `REASONING_MESSAGE_CONTENT` | `messageId`, `delta` |
| | `REASONING_MESSAGE_END` | `messageId` |
| | `REASONING_MESSAGE_CHUNK` | `messageId?`, `delta?` |
| | `REASONING_ENCRYPTED_VALUE` | `subtype: message \| tool-call`, `entityId`, `encryptedValue` (opaque, returned unchanged) |
| State (snapshot-delta) | `STATE_SNAPSHOT` | `snapshot` (any JSON) |
| | `STATE_DELTA` | `delta: JsonPatch` (RFC 6902, applied atomically; a well-formed patch that fails is skipped with a warning) |
| | `MESSAGES_SNAPSHOT` | `messages: Message[]` (reconciled by id; activity and reasoning messages are kept unless the snapshot restates that role) |
| Activity (snapshot-delta) | `ACTIVITY_SNAPSHOT` | `messageId`, `activityType` (open string), `content: object`, `replace?` |
| | `ACTIVITY_DELTA` | `messageId`, `activityType`, `patch: JsonPatch` |
| Subagents | `SUBAGENT_STARTED` | `subagentRunId`, `name`, `description?`, `parentSubagentRunId?`, `parentToolCallId?`, `parentMessageId?` |
| | `SUBAGENT_FINISHED` | `subagentRunId`, `result?`, `outcome?: success \| suspended{interruptIds?}` |
| | `SUBAGENT_ERROR` | `subagentRunId`, `message`, `code?` |
| Passthrough | `RAW` | `event` (any), `source?` |
| | `CUSTOM` | `name`, `value` (a consumer ignores names it does not know; producers SHOULD prefix names with a vendor) |

The family table and who consumes each family are in [events](https://docs.ag-ui.com/spec/1.0/events). Only the run lifecycle is mandatory for a producer. A consumer must accept every family.

### 2.4 Lifecycle rules that matter for Atlas ([runs and steps](https://docs.ag-ui.com/spec/1.0/events/lifecycle))
- A stream starts with `RUN_STARTED` or `RUN_ERROR`. Everything a run opened (messages, tool calls, steps, subagents) must be closed before `RUN_FINISHED`. After the close, only a new `RUN_STARTED` or a late `RUN_ERROR` may follow.
- **Cancelled is its own outcome.** A stopped run MUST NOT be reported as success, and a consumer MUST NOT show it as either success or failure.
- **Truncation is not cancellation.** A dropped connection has no outcome. The consumer MUST NOT invent a `RUN_FINISHED` for it.
- **Several runs on one stream.** Replayed history uses snapshots (`MESSAGES_SNAPSHOT`, `STATE_SNAPSHOT`), so replays are idempotent.
- **Token usage.** One `TokenUsage` entry per provider and model (`provider?`, `model?`, `inputTokens?`, `outputTokens?`, `totalTokens?`, `reasoningTokens?`, `cachedInputTokens?`, `cacheWriteInputTokens?`):
  - `inputTokens` *includes* cached reads and cache writes. The producer must add them in when its provider reports them separately.
  - An absent count is not zero.
  - Subagent usage is counted in the parent run. A separate run (`parentRunId`) and a resumed run report their own usage.

### 2.5 Run input (`RunAgentInput`) ([run input](https://docs.ag-ui.com/spec/1.0/basic/run-input))
| Field | Req. | Rule |
|---|---|---|
| `threadId`, `runId` | yes | identity |
| `protocolVersion` | no | the consumer's version (`MAJOR.MINOR`). The producer answers with its own on `RUN_STARTED` |
| `parentRunId` | no | the run that spawned this one |
| `messages` | yes | **the complete history**: "the protocol has no side channel through which earlier turns arrive". Activity messages are stripped before sending |
| `tools` | no | **frontend tools** (name, description, parameters JSON Schema). Absent = empty |
| `context` | no | `{description, value}` pairs to inject into the model's grounding |
| `state` | no | the starting state; absent = `{}` |
| `forwardedProps` | no | app-specific, passed through; intermediaries MUST NOT change it |
| `resume` | no | `ResumeEntry{interruptId, status: resolved \| cancelled, payload?, metadata?}` |

Content parts (`text`, `image`, `audio`, `video`, `document`, each with a `data`/`url`/`file` source) are allowed on user and tool messages (`AGUI:docs/spec/1.0/changelog.mdx`, minor 6).

### 2.6 Frontend tools vs backend tools ([tool calls](https://docs.ag-ui.com/spec/1.0/events/tool-calls))
- **Backend tools** are the agent's own. It runs them and reports `TOOL_CALL_RESULT` in the same stream. They are optionally declared in `AgentCapabilities.tools.items`.
- **Frontend tools** are the app's, listed in `RunAgentInput.tools`. The app runs them.
  - The producer MUST NOT answer them.
  - It finishes the run as *success*, optionally naming `pendingToolCallIds`.
  - The next run's `messages` carry one tool message per call (a decline is still an answer).
  - "The protocol has no mid-run channel from the consumer, so the answer can only cross a run boundary" (`AGUI:docs/spec/1.0/events/tool-calls.mdx:159`).
- Security rules: tool arguments are untrusted input; get consent before side effects; never present a call as user-approved when it was not.

### 2.7 Interrupts and approvals (human in the loop) ([interrupt-resume](https://docs.ag-ui.com/spec/1.0/basic/patterns/interrupt-resume))
- A run that needs outside input **ends** with `RUN_FINISHED{outcome:{type:"interrupt", interrupts:[Interrupt…]}}`.
- `Interrupt` = `id`, `reason` (open string), `message?`, `toolCallId?`, `responseSchema?` (opaque JSON Schema for building a form), `expiresAt?`, `subagentRunId?`, `metadata?`.
- The next run carries `resume[]`. It MUST cover every interrupt, either answered or explicitly `cancelled`. The **consumer** enforces coverage. A producer may be stateless.
- A producer that knows an interrupt is still open MUST NOT perform the action because an entry is missing.
- "An interrupted run is a closed run". The model is terminal and stateless by design (`AGUI:docs/spec/1.0/basic/patterns/interrupt-resume.mdx:7`).

### 2.8 Shared state and generative UI
- **Shared state.** `STATE_SNAPSHOT` replaces the state and `STATE_DELTA` patches it (JSON Patch). The next run sends the state back as `RunAgentInput.state`. State events are remote writes and must be validated before acting on them ([state](https://docs.ag-ui.com/spec/1.0/events/state)).
- **Activity.** Typed progress widgets (`activityType` + object content, patched in place). They keep their place in the message sequence but never go back to the agent ([activity](https://docs.ag-ui.com/spec/1.0/events/activity)).
- **Generative UI** reaches the app through frontend tools, activity, and the A2UI or MCP Apps middlewares (`AGUI:middlewares/a2ui-middleware`, `AGUI:middlewares/mcp-apps-middleware`). A "Generative User Interfaces" draft (two-step generation) is still `Draft` (`AGUI:docs/drafts/generative-ui.mdx`).

### 2.9 Transports ([transports](https://docs.ag-ui.com/spec/1.0/basic/transports))
| Binding | Status | Framing |
|---|---|---|
| HTTP POST + SSE | MUST for any HTTP implementation | JSON body. Each SSE `data:` holds exactly one event. LF line endings. Comments are allowed as keep-alives |
| HTTP POST + Protobuf | optional | `Accept: application/vnd.ag-ui.event+proto`, 4-byte big-endian length-prefixed frames, a byte-parity corpus across encoders ([http-protobuf](https://docs.ag-ui.com/spec/1.0/basic/transports/http-protobuf)) |
| WebSocket, message bus, in-process | "custom transport", MAY | must keep ordered, complete delivery, input before events, a termination signal distinct from truncation, and an error path for rejected input. Should frame "exactly as the SSE binding does (one event object per frame) rather than inventing a new envelope" |

Further transport facts:
- **No stream resumption.** `Last-Event-ID` is not used, and a broken stream cannot be re-entered (`AGUI:docs/spec/1.0/basic/transports/http-sse.mdx:51-53`).
- `transport.resumable` and `transport.pushNotifications` are reserved capability flags with no mechanism behind them (`AGUI:docs/spec/1.0/basic/capabilities.mdx:94`; open design issues [#2105](https://github.com/ag-ui-protocol/ag-ui/issues/2105) and [#2106](https://github.com/ag-ui-protocol/ag-ui/issues/2106)).
- **Auth is out of scope.** "AG-UI defines no credential" (`AGUI:docs/spec/1.0/basic/transports/index.mdx:33`).
- **WebSocket** has no standard binding. Proposals are open ([#2084](https://github.com/ag-ui-protocol/ag-ui/issues/2084), [#1568](https://github.com/ag-ui-protocol/ag-ui/issues/1568)).

### 2.10 Capabilities and versioning
- **`AgentCapabilities`** has these groups: `identity`, `transport`, `tools`, `output`, `state`, `multiAgent`, `reasoning`, `multimodal`, `execution`, `humanInTheLoop` (`approvals`, `interventions`, `feedback`, `interrupts`, `approveWithEdits`) and `custom`.
  - Declarations are informative; the stream is authoritative.
  - **How a client retrieves them is deliberately unspecified** ([capabilities](https://docs.ag-ui.com/spec/1.0/basic/capabilities)).
- **Versioning** ([versioning](https://docs.ag-ui.com/spec/1.0/basic/versioning)):
  - The version is negotiated in-band: `RunAgentInput.protocolVersion` and `RUN_STARTED.protocolVersion`.
  - Minors only add things. A producer MUST serve a newer minor of its own major.
  - Lossy downgrades must warn.
  - Retired shapes are translated by middleware and listed in a registry with expiry dates. The 0.x shims (`THINKING_*` → `REASONING_*`, `null` fields, the `binary` part) expire on **2027-09-17** (`AGUI:DEPRECATIONS.md`).
  - Version-gated shims exist for the 0.0.39, 0.0.45 and 0.0.57 eras (`AGUI:DEPRECATIONS.md`). That is evidence of breaking churn before 1.0.

### 2.11 Middleware and the client SDK
- **Middleware.** The TS `AbstractAgent.use(...)` chain is made of RxJS operators over the event stream: transform, filter, log, auth (`AGUI:docs/concepts/middleware.mdx`).
  - It runs **before** enforcement, so shims can translate retired shapes (architecture principle 4).
  - `connectAgent()` (attach to an existing run or thread) calls the abstract `connect()` and **does not run middleware** (`AGUI:docs/concepts/middleware.mdx:38`, `AGUI:sdks/typescript/packages/client/src/agent/agent.ts:444-481`).
  - What `connect()` means is up to each agent implementation. The base class throws `AGUIConnectNotImplementedError`.
- **Shipped middlewares:** `mcp-middleware` (lists an MCP server's tools, injects them into the run, executes the calls server-side and loops), `a2a-middleware`, `a2ui-middleware`, `mcp-apps-middleware`, `event-throttle-middleware` (`AGUI:middlewares/`).
- **Client pipeline:** compatibility boundary → middleware → enforcement → chunk expansion → verification (`AGUI:sdks/typescript/packages/client/src/agent/agent.ts:479-490`).

---

## 3. Ecosystem

### 3.1 SDKs (as of `1de7b128`)
| Language | Maintainer | Version | 1.0 coverage | Evidence |
|---|---|---|---|---|
| TypeScript `@ag-ui/core`, `client`, `encoder`, `proto` | first party | 1.0.5 | full: generated from the schema, conformance lane | `AGUI:sdks/typescript/packages/*/package.json`. `core` peer-depends on `zod`; `client` depends on `rxjs`, `fast-json-patch`, `uuid`, `compare-versions`, `untruncate-json` |
| Python `ag-ui-protocol` | first party | 1.0.0 | full | `AGUI:sdks/python/pyproject.toml` |
| .NET | first party | 1.0.1 | full, conformance lane | `AGUI:sdks/dotnet/Directory.Build.props`; spec index "three first-party SDKs" (`AGUI:docs/spec/1.0/index.mdx:126`) |
| Kotlin (multiplatform, incl. iOS) | community | 0.4.1 | interrupts, reasoning | `AGUI:sdks/community/kotlin/CHANGELOG.md` |
| Go | community | n/a | subagents, outcomes | `AGUI:sdks/community/go/pkg/core/events/subagent_events.go`, `run_outcome_test.go` |
| **Dart/Flutter** `ag_ui` | community | 0.3.0 (pub.dev) | **pre-1.0**: no `SUBAGENT_*`, no `RUN_FINISHED.outcome` or interrupts, still has `THINKING_*` | `AGUI:sdks/community/dart/lib/src/events/event_type.dart:30-66`; `grep interrupt\|outcome` in `dart/lib` finds comments only |
| **Rust** (in-repo) `ag-ui-core` / `ag-ui-client` | community | 0.1.0 (crates.io, last updated 2025-08-12) | **pre-1.0**: `Thinking*` events, its TODO lists JSON Patch types, SSE review, error handling | `AGUI:sdks/community/rust/crates/ag-ui-core/src/event.rs:20-38`, `AGUI:sdks/community/rust/TODO` |
| Rust (external) `ag-ui` | third party (KimSoungRyoul) | 1.0.0, published 2026-11-20, 641 downloads | claims 1.0 | crates.io API; repo `KimSoungRyoul/ag-ui-rust` |
| Java, Ruby, C++, Swift | community | n/a | not checked | `AGUI:sdks/community/` |

Community SDKs "will need to be maintained by the community member who made the contribution" (`AGUI:CONTRIBUTING.md:54`).

### 3.2 Integrations (agent side)
- **In-repo integrations** (`AGUI:integrations/`): LangGraph, LangChain, CrewAI, Mastra, Pydantic AI, Agno, AG2, LlamaIndex, Google ADK (Python middleware and JS), AWS Strands, Microsoft Agent Framework, OpenAI Agents SDK, Vercel AI SDK, watsonx, Langroid, Oracle Agent Spec, Google Antigravity, **Claude Agent SDK** (TS and Python), **Claude Managed Agents**, A2A, and community Cloudflare Agents, Genkit, Omnara and Spring AI.
- **Built-in support** is claimed for Microsoft Agent Framework, Google ADK, Google Antigravity, AWS Strands, Mastra, Pydantic AI, Agno, LlamaIndex and AG2. LangChain and CrewAI are listed as "Partners" (`AGUI:README.md:176-177`).
- **Amazon Bedrock AgentCore Runtime** has native AG-UI hosting ([AWS announcement 2026-03](https://aws.amazon.com/about-aws/whats-new/2026/03/amazon-bedrock-agentcore-runtime-ag-ui-protocol/), listed in `AGUI:docs/integrations.mdx`).
- **n8n: none.** [#369 "Add n8n Support"](https://github.com/ag-ui-protocol/ag-ui/issues/369) has been open since 2025-09-10. Flowise ([#367](https://github.com/ag-ui-protocol/ag-ui/issues/367)) and Langflow ([#366](https://github.com/ag-ui-protocol/ag-ui/issues/366)) are also still open.
- **No integration for Codex CLI, OpenCode or omp.** Claude is covered only through the Agent SDK and Managed Agents, not through the `claude` CLI that Atlas runs.
- **The Claude Agent SDK adapter matters most for Atlas.** It maps native permission deferrals (`deferTools`) to `RUN_FINISHED.outcome.interrupt`. It resumes the same SDK session with `resume[{interruptId, status, payload:{behavior: allow|deny, updatedInput?}}]`. It needs a durable thread→session binding store. It supports **one deferred tool call at a time**, and its README calls its own validation "synthetic" (`AGUI:integrations/claude-agent-sdk/typescript/README.md`, "Session recovery (PNI-659, unreleased)").

### 3.3 Clients (UI side)
CopilotKit (React, Angular, Vue, React Native), CopilotKit channels-sdk (Slack, Teams), a terminal client, OpenBot builders, and the Dojo demo app (`AGUI:README.md:166`, `AGUI:docs/integrations.mdx`, `AGUI:apps/dojo`). OpenBot (studied in PR #669) is listed as an AG-UI client.

### 3.4 Relation to MCP, A2A and ACP
| Protocol | Link | How AG-UI relates | Evidence |
|---|---|---|---|
| MCP | agent to tools/context | complementary. `mcp-middleware` runs MCP tools server-side inside an AG-UI run; MCP Apps UI rides through `mcp-apps-middleware` | `AGUI:docs/agentic-protocols.mdx`, `AGUI:middlewares/mcp-middleware/README.md` |
| A2A | agent to agent | complementary. `@ag-ui/a2a` turns an A2A agent into an AG-UI agent and is marked "Experimental"; `pushNotifications` might be aligned with A2A's `PushNotificationConfig` ([#2106](https://github.com/ag-ui-protocol/ag-ui/issues/2106)) | `AGUI:integrations/a2a/typescript/README.md` |
| ACP (Agent Client Protocol, Zed) | editor to coding-agent CLI (JSON-RPC over stdio; remote "work in progress") | **not mentioned by AG-UI**. It sits on the hostd to CLI side, where Atlas's engines live, not the hostd to UI side | [agentclientprotocol.com](https://agentclientprotocol.com/get-started/introduction); PR #669 README C5 proposes ACP as the generic engine driver |

---

## 4. Maturity and adoption (measured 2026-11-20)

| Metric | Value | Source |
|---|---|---|
| GitHub stars / forks / open issues | 20,778 / 2,129 / 599 | `gh api repos/ag-ui-protocol/ag-ui` |
| Repo created | 2025-05-07 | same |
| Contributors | ~176 | contributors API pagination (`per_page=1`, last page 176) |
| Commits on `main` since 1.0 (2026-09-17) | ~1,275 | commits API pagination |
| Release cadence | date-tagged releases (`release/2026-11-09`, `-10`, `-12`, `-13`, `-17`) | releases API |
| npm weekly downloads (2026-11-13..19) | `@ag-ui/core` 7.57 M, `@ag-ui/client` 2.58 M, `@ag-ui/encoder` 2.68 M | api.npmjs.org |
| PyPI `ag-ui-protocol` | 2.26 M/week, 9.2 M/month | pypistats.org |
| crates.io | `ag-ui-core` 24 k total (stale 0.1.0); `ag-ui` 1.0.0 641 | crates.io API |
| Committer email domains since 2026-08-20 | gmail 732, copilotkit.ai 662, noreply 336, microsoft.com 17, anthropic.com 11 | `git log --format=%ae` |
| Governance | CopilotKit-created, no foundation found; community SDKs owned by their authors | `AGUI:README.md:248`, `AGUI:CONTRIBUTING.md:54` |

[INFERENCE] The npm numbers are inflated by CopilotKit's own dependency tree and CI. They show reach in the JS/Python agent-framework world, not independent frontend adoption.

**Maturity verdict.** The spec is unusually rigorous for its age: normative rules, a closed schema, a conformance corpus, a deprecation registry with expiry dates. But 1.0 is **two months old**, the 0.x line broke shapes several times, and the Rust and Dart SDKs (Atlas's two languages besides TS) lag behind 1.0.

### Open proposals relevant to Atlas
| Issue | Topic | Why it matters here |
|---|---|---|
| [#2105](https://github.com/ag-ui-protocol/ag-ui/issues/2105) | stream resumption by sequence number | phones reconnect all the time; Atlas re-syncs by refetching (`atlas:crates/atlas-host/src/protocol.rs:12-16`) |
| [#2106](https://github.com/ag-ui-protocol/ag-ui/issues/2106) | push notifications after a run ends | crew runs nobody watches (triggers, schedules) |
| [#2186](https://github.com/ag-ui-protocol/ag-ui/issues/2186) | client resends the full history on every run | crewmate threads are long-lived and hostd owns the session |
| [#2157](https://github.com/ag-ui-protocol/ag-ui/issues/2157) | client → server custom action channel | steering, mid-run approvals, `ask_user` answers |
| [#2154](https://github.com/ag-ui-protocol/ag-ui/issues/2154) | rewind / edit / regenerate | n/a |
| [#2192](https://github.com/ag-ui-protocol/ag-ui/issues/2192) | connection-time capability handshake | capabilities retrieval is unspecified |
| [#1454](https://github.com/ag-ui-protocol/ag-ui/issues/1454) | server-minted `threadId`/`runId` | triggers create tasks server-side |
| [#1593](https://github.com/ag-ui-protocol/ag-ui/issues/1593) | MCP `structuredContent` in tool results | Atlas MCP tools (`capture`, `locate`, `ui_gate`) |
| Draft "Meta Events" | run-independent annotations (feedback, tags) | RFC §18.4 feedback (`AGUI:docs/drafts/meta-events.mdx`) |

---

## 5. Atlas Crew today: what an event stream would project from

Classifications use the RFC terms. They describe Atlas's *existing primitive* for each concept, not AG-UI support. Atlas has no AG-UI code: `grep -ri 'ag-ui\|agui\|ag_ui'` over `*.rs/*.ts/*.dart/*.md` returns nothing.

| Capability (RFC term) | Classification | Evidence | Notes |
|---|---|---|---|
| Client wire protocol | Implemented (not re-tested here) | NDJSON over a unix socket, one multiplexed connection per client; request `{id,cmd,args}`, push `{"ev":…}` (`atlas:crates/atlas-host/src/protocol.rs:1-9`); `PROTO = 2`, bumped only on non-additive changes, checked by mobile `kMinHostProto` (`protocol.rs:69-77`, `apps/mobile/lib/core/protocol/protocol_version.dart:6,10`); `caps` gate features (`protocol.rs:84-92`) | additive pushes need no bump, so an AG-UI projection can be added behind a cap |
| Run = task | Implemented | `Task` with `state` Queued/Running/Done/Failed/Cancelled/WaitingAction (`crew/store/mod.rs:340-347, 359-447`) | `Queued` and `preparing`/`waiting_for` happen before any AG-UI run exists |
| Run event stream to clients | Implemented, incomplete | one coarse push, `crewmate_task {crewmate_id, task?}` carrying the **whole task** (`crew/events.rs:11-17`), plus `agent_status` / `agent_step` / `agent_tool_output` / `agent_approval(_done)` per pane (`agents/mod.rs:353-373, 402-407, 452-466, 492-495`) | state replacement, not deltas; no message or tool-call ids |
| Live reply streaming | Missing | engines collect all stdout before parsing: `launch::output` (`crew/engine/launch.rs:210-230`); `claude -p --output-format json` keeps only the final `result` object (`crew/engine/claude.rs:16-24, 42-48`); omp reads until `prompt_result` (`crew/engine/omp.rs:1-12`) | AG-UI `TEXT_MESSAGE_CONTENT` would carry the whole reply as one delta |
| Tool-call steps | Implemented, incomplete | `agent_step {pane_key, tool_name, tool_input, tool_output}` from hooks (`agents/mod.rs:351-362`); stored as `Step{ts,text,group}`, capped at 50, **only for terminal (pty) tasks** (`crew/runner/outcome.rs:71-95`); conversational tasks finish with `vec![]` steps (`crew/runner/conversation.rs:335`) | no `toolCallId` on `agent_step`; the text is a lossy summary |
| Approvals (§15.5) | Implemented | rails → mode → task grants → person; nobody answering in `CREWMATE_DECISION_WAIT` (30 min) is a deny (`crew/approval.rs:1-23`, `agents/hook_server.rs:37`); `agent_approve {pane, req, allow, scope, comment}` (`protocol.rs:388-399`); voice answers (`protocol.rs:400-409`) | **synchronous hold of a live process**: the run does not end |
| ask_user card | Implemented | MCP tool blocks the agent until answered (`crew/mcp.rs:6, 159, 172`); `AskRequest{id, question, options, allow_text, answered}` (`crew/store/mod.rs:189-212`); answer `crewmate_answer` (`protocol.rs:766-773`) | also a mid-run hold |
| Parked actions (`ATLAS_ACTION`) | Implemented | reply line → `TaskAction` (connect GitHub, secret, input, choose, sign-in, add trigger; Atlas-only `Refused`/`ProveUi`) (`crew/runner/action.rs:1-41`, `crew/store/mod.rs:300-337`); "parks the task until the user answers, then a second turn carries the answer" (`crew/runner/conversation.rs:1-3, 145-157`); answer `crewmate_action_answer` (`protocol.rs:814-826`) | **the same shape as AG-UI interrupt → resume** |
| Spoken replies | Implemented | `Task.spoken` (`crew/store/mod.rs:387-390`), `runner/spoken.rs` | no AG-UI field; would go in metadata |
| Usage / cost (§22, §31) | Implemented, incomplete | `UsageRecord{model, input_tokens, output_tokens, cache_read, cache_write, cost_usd, duration_ms}` (`crew/store/mod.rs:172-187`) | AG-UI has no cost or duration field; its `inputTokens` includes cache |
| Subagents / delegation (§10) | Implemented, incomplete | `agent_tree` flat nodes with parent (`protocol.rs:419-424`); crewmate chain triggers (`Task.chain`, `crew/store/mod.rs:442-446`) | the tree maps to `SUBAGENT_*`, the chain to `parentRunId` |
| Cancel / interrupt | Implemented | `crewmate_cancel` (`protocol.rs:673-676`), `agent_interrupt` (Escape) (`protocol.rs:436-439`) | AG-UI has no cancel request in-band, only the `cancelled` outcome |
| Steering mid-run | Implemented (omp only) | `agent_send {steer:true}` (`protocol.rs:410-418`) | no AG-UI equivalent ([#2157](https://github.com/ag-ui-protocol/ag-ui/issues/2157)) |
| Restart recovery (§11) | Implemented | `Task.restarts`, `Task.resumed` (`crew/store/mod.rs:422-428`), `runner/recover.rs` | AG-UI: a truncated run; the next run is a new `runId` |
| Mobile relay and scopes (§30) | Implemented | relay wraps pushes as `{"ev":"crew_event","server",…}` (`crew/relay/mod.rs:1-5, 376`); per-push phone scopes, `agent_approval` needs `Approve` (`mobile/mod.rs:456-468`) | any new push needs a scope entry or it never reaches phones |

---

## 6. Mapping Atlas Crew onto AG-UI 1.0

### 6.1 Identity
| Atlas | AG-UI | Note |
|---|---|---|
| crewmate | agent (`AgentCapabilities.identity.name`, `framework: "atlas-crew"`) | capability retrieval is ours to define |
| crewmate conversation (`CrewmateSession`, `crew/store/mod.rs:161-169`) | `threadId` (e.g. `crewmate:<id>`) | hostd owns the thread; client `messages` are advisory (see risk R4) |
| task | `runId` = task id | a parked-then-answered task becomes **two runs** on the thread |
| task chained by a crewmate trigger | `RUN_STARTED.parentRunId` = the previous task's id | |
| `agent_tree` subagent node | `subagentRunId` + `SUBAGENT_STARTED{name, parentSubagentRunId}` | |

### 6.2 Events
| Atlas concept | AG-UI event(s) | Fit |
|---|---|---|
| task `running` | `RUN_STARTED{protocolVersion:"1.0", input?}` | exact |
| task `done` | `RUN_FINISHED{outcome: success, result?: reply, usage}` | exact |
| task `failed` (error text) | `RUN_ERROR{message, code?}` (e.g. `code: "oom"`, `"timeout"`, `"guard_refused"`) | exact |
| task `cancelled` | `RUN_FINISHED{outcome:{type:"cancelled"}}` | exact; AG-UI forbids showing it as success or failure, which is how Atlas already treats it |
| task `waiting_action` + `TaskAction` | `RUN_FINISHED{outcome:{type:"interrupt", interrupts:[{id, reason:"connect_github"\|"provide_secret"\|"input"\|"choose"\|"sign_in_engine"\|"add_trigger"\|"refused"\|"prove_ui", message, responseSchema}]}}` | **good**. `crewmate_action_answer` becomes `resume[{interruptId, status, payload}]`. **`provide_secret` must never be answered through AG-UI** (the payload would round-trip in the run input) |
| `queued` / `preparing` / `waiting_for` ("server busy") | `ACTIVITY_SNAPSHOT{activityType:"dev.atlas.task", content:{state, preparing, waiting_for}}` once a run is open; before that, nothing (no run exists yet) | gap: AG-UI has no pre-run "queued" state |
| `task.title` (what the agent says it works on) | `STEP_STARTED/FINISHED{stepName}` or an activity patch | fine |
| `agent_step` (tool call) | `TOOL_CALL_START{toolCallId, toolCallName}` → `TOOL_CALL_ARGS{delta: JSON(tool_input)}` → `TOOL_CALL_END` → `TOOL_CALL_RESULT{content: tool_output}` | needs a stable `toolCallId` (claude's `tool_use_id`, omp's call id); `agent_step` carries none today |
| `agent_tool_output` with `is_error` (omp) | `TOOL_CALL_RESULT` + draft-1.1 `error` | 1.1 only |
| final reply (`reply_full`) | `TEXT_MESSAGE_START/CONTENT/END` (one delta today) | streaming would need stream-json engines |
| short reply (`reply`) and `spoken` | `metadata: {"dev.atlas.short": …, "dev.atlas.spoken": …}` on the text message | metadata is open by key; the `ag-ui` key is reserved |
| `usage` | `RUN_FINISHED.usage[{provider, model, inputTokens = input + cache_read + cache_write, cachedInputTokens: cache_read, cacheWriteInputTokens: cache_write, outputTokens}]`; `cost_usd`, `duration_ms` → `metadata` | the accounting rule must be implemented; Claude reports cache separately [INFERENCE from the field names] |
| attachments (images) | user message `ContentPart` `image` with a `url` source to a hostd fetch endpoint or inline `data` | |
| reasoning | `REASONING_*` | Atlas does not capture reasoning today |
| **approval prompt** (`agent_approval`, live hold) | inside the open run: `TOOL_CALL_START/ARGS/END` for the held call, plus `ACTIVITY_SNAPSHOT{activityType:"dev.atlas.approval", content:{req, rail?, scopes}}`; answered via the native `agent_approve`; resolved with `ACTIVITY_DELTA` | **gap**. The AG-UI interrupt is terminal ("an interrupted run is a closed run") while Atlas keeps the agent process waiting up to 30 min. Ending the run while the process lives breaks AG-UI's lifecycle; keeping it open needs a non-AG-UI answer channel |
| **`ask_user`** (live hold) | same pattern: `TOOL_CALL_*` named `ask_user` + `ACTIVITY_SNAPSHOT{activityType:"dev.atlas.ask", content: AskRequest}`; answered via `crewmate_answer` | gap, as above |
| steering (`agent_send steer`) | none | gap ([#2157](https://github.com/ag-ui-protocol/ag-ui/issues/2157)) |
| `crewmate_updated`, `crew_paused`, `pty_*`, `lsp_*`, `fs_event`, `atlas_update` | none: not run events | stay native |
| memory, audit, routines, triggers, settings | none | out of AG-UI's scope |

### 6.3 Transport options
1. **Inside hostd's NDJSON socket (desktop, phone relay).**
   - What: a new additive push, e.g. `{"ev":"agui","crewmate_id":…,"event":{…AG-UI event…}}`, gated by a new cap `agui` and a phone `Scope::Read` (approval-bearing events need `Scope::Approve`, as at `mobile/mod.rs:462`).
   - Spec fit: this wraps the event in an envelope, which the custom-transport SHOULD advises against. The reason is legitimate: one connection multiplexes many runs and ptys.
   - Gaps: `RunAgentInput` delivery and run termination must be documented. The input already exists as `crewmate_task` (`protocol.rs:631-646`).
2. **HTTP + SSE endpoint on the crew server**, for third-party frontends (CopilotKit, channels-sdk, the AG-UI terminal client).
   - What: `POST /agui/crewmates/<id>` takes a `RunAgentInput` and creates a task. It sits behind the operator-supplied HTTPS that webhooks already use (PR #669 README A4 describes that HTTPS).
   - Gaps: a run started elsewhere (Telegram, GitHub, schedule) cannot be watched through standard AG-UI. A `connect` endpoint is implementation-defined, and in TS it skips middleware.
3. **Inbound (hostd as an AG-UI consumer).**
   - What: a new engine kind `agui` whose `run_turn` POSTs a `RunAgentInput` to a remote agent URL (LangGraph, Mastra, ADK, Strands, AgentCore, Claude Managed Agents…) and folds the stream into `TurnResult`, `steps` and `TaskAction`.
   - Fit: interrupt outcomes map to `waiting_action` exactly, and the Atlas MCP tools (`ask_user`, `capture`…) could be offered as frontend tools.
   - Why it matters: this covers RFC §20 "external agent runtimes" without rebuilding them.

### 6.4 New dependency, or only a wire format?
**Only a wire format, if Atlas wants it to be.**
- **Rust (hostd).** No 1.0 Rust crate is first-party or mature. In-repo `ag-ui-core` 0.1.0 is pre-1.0; the external `ag-ui` 1.0.0 was published today. The schema is self-contained (JSON Patch inlined, `AGUI:spec/README.md` "Addressing a definition"). 31 serde structs with `skip_serializing_if = "Option::is_none"` (Atlas already uses that convention, `crew/store/mod.rs:163-186`), checked in tests against a vendored, pinned `schema.json`, need no runtime dependency.
- **Desktop (Svelte/TS).** `@ag-ui/core` is types plus zod, so it is optional. `@ag-ui/client` brings `rxjs` and `fast-json-patch`, and only pays off if the desktop renders through it.
- **Mobile (Flutter).** The community `ag_ui` 0.3.0 is pre-1.0 and lacks subagents and interrupts (`AGUI:sdks/community/dart/lib/src/events/event_type.dart`). The phone would either use its own types or wait.
- **Native push wins.** As long as the native `crewmate_task` push is the source of truth for Atlas's own clients, no client needs an AG-UI dependency at all.

---

## 7. Recommendation and alternatives

**Recommendation: do not make AG-UI the internal hostd to desktop/mobile protocol. Adopt AG-UI 1.0 at the edge, as a wire format with no SDK dependency in hostd, when RFC §20 interop work starts. Order: (1) an inbound `agui` engine kind, (2) an outbound read-mostly SSE projection of crewmate runs.**

Why:
1. **The semantic core does not fit Atlas's live holds.**
   - AG-UI has "no mid-run channel from the consumer" (`tool-calls.mdx:159`, `interrupt-resume.mdx:7`).
   - Atlas approvals and `ask_user` hold a live agent process for up to 30 min (`approval.rs:12-13`, `hook_server.rs:37`).
   - Mapping them needs Atlas-specific activities and native answer commands, so the part that matters most for safety (§15.5 approval binding) would not be portable anyway.
2. **The parked-action path fits AG-UI interrupts almost one-to-one** (`conversation.rs:1-3`). So does the Claude Agent SDK adapter's durable deferral. The inbound direction therefore gets the most value at the lowest risk: any AG-UI agent becomes a crewmate, and Atlas's rails, modes, audit and cards wrap it.
3. **Atlas's own clients would gain nothing today.**
   - Engines do not stream tokens (`launch.rs:210-230`).
   - The native push already carries everything, plus Atlas-only concepts (spoken, prove_ui, guards, chain).
   - Migrating would cost a `PROTO` bump and a Flutter SDK that does not exist at 1.0.
4. **The ecosystem is the real asset:** CopilotKit UIs, Slack and Teams channels, AgentCore, Microsoft Agent Framework, ADK. It is reachable at the edge without a rewrite.

| Option | What | Pros | Cons | Verdict |
|---|---|---|---|---|
| A. Edge adoption (recommended): inbound `agui` engine, then an outbound SSE projection | hostd-owned serde types from the pinned schema; native protocol unchanged | interop both ways; no client migration; no dependency; additive under `PROTO=2` | two vocabularies to keep in step; live holds need `dev.atlas.*` activities | **do, after §20 is scheduled** |
| B. AG-UI replaces `crewmate_task` / `agent_*` pushes for desktop and phone | one event model everywhere | ecosystem renderers on desktop | `PROTO` bump; no 1.0 Dart SDK; no live holds or steering; history-resend model (#2186) clashes with hostd-owned sessions; the protocol is 2 months old | **no, for now** |
| C. Additive `{"ev":"agui"}` push on the NDJSON socket only | lets the desktop prototype AG-UI rendering | cheap; behind a cap | no third-party value by itself | optional spike inside A |
| D. No AG-UI; expose crew via MCP (PR #669 A4) and A2A | crew as a tool or peer for other agents | already-planned direction | no streaming UI interop; A2A is agent-to-agent, not UI | complementary, not a substitute |
| E. ACP for hostd to CLI engines (PR #669 C5) | generic engine driver | the engine side is where Atlas spends the most per-engine code | ACP is editor to agent, not UI; orthogonal to AG-UI | evaluate separately; an ACP engine could feed the same AG-UI projection |

---

## 8. Risks

| # | Risk | Likelihood / impact | Mitigation |
|---|---|---|---|
| R1 | **Protocol churn.** 0.x broke shapes (0.0.39/45/57 shims, THINKING→REASONING); 1.0 is 2 months old; shims expire 2027-09-17 (`AGUI:DEPRECATIONS.md`) | medium / medium | pin `spec/1.0/schema.json` by hash in-repo; emit only 1.0 shapes; additive-only minors are a spec MUST (`versioning`); re-validate when 1.1 is frozen |
| R2 | **Governance concentration.** CopilotKit created it; copilotkit.ai is the largest named committer domain; no foundation | medium / medium | no SDK dependency in hostd; the schema is MIT and self-contained, so a fork-free freeze is possible |
| R3 | **Semantic mismatch** (no mid-run input, no queued state, no subscription to runs started elsewhere, no stream resume #2105) | high / high for approvals | keep approvals and `ask_user` native; `dev.atlas.*` activities are display-only; never derive authority from an AG-UI event |
| R4 | **History and state round-trip.** AG-UI treats client `messages` and `state` as authoritative input; hostd owns the session | high / medium | ignore client history for crewmate threads (document it), or accept only the last user message; never put secrets, tokens or rails in `state` (the spec says the same: "Producers SHOULD NOT put secrets in them") |
| R5 | **New attack surface** (HTTP endpoint, model output as UI, `forwardedProps` passthrough) | medium / high | same auth as webhooks plus per-client scopes (read vs approve, as in `mobile/mod.rs:462`); runs from AG-UI input get `no_auto`-style rails until the client is trusted; never put hook payloads (env, paths) in `rawEvent` |
| R6 | **Usage mis-accounting** (`inputTokens` includes cache) | medium / low | unit test the conversion against AG-UI's rule |
| R7 | **Dual maintenance** of native events and the projection | high / medium | generate the projection from one internal event in one module (as `crew/events.rs` is "the one place their JSON is built"), with golden fixtures like `testdata/hostd/*.json` |
| R8 | **SDK lag** in Rust and Dart | certain / low if wire-only | own types; contribute upstream only if useful |
| R9 | **Enforcement strips unknown fields.** Atlas extras put in a standard event as new properties vanish in conforming clients | certain / medium | extras go only in `metadata` (open by key) or `CUSTOM` with a `dev.atlas.` prefix |

---

## 9. Acceptance criteria for an adoption (option A)

1. **Schema.** Every event hostd emits validates against the vendored, hash-pinned `spec/1.0/schema.json` (Draft 2020-12). A Rust test runs the emitter over golden task scenarios and validates each event. The test fails on any `null` optional field or empty interrupt list (both checks were shown to work in this study).
2. **Sequencing.**
   - Recorded streams for these scenarios pass the reference verifier (`@ag-ui/client` `verifyEvents`, or a port of the lifecycle rules) with zero violations: done, failed, cancelled, OOM, timeout, `waiting_action` → resume (two runs, `resume` covering every interrupt), approval held then allowed or denied or timed out, `ask_user`, a hostd restart mid-task (truncated run, then a new `runId` with `resumed`), and a chain trigger (`parentRunId`).
   - A selection of AG-UI's own conformance fixtures (`spec/1.0/conformance/streams`) is replayed into the inbound engine's consumer.
3. **Interop.**
   - The official TS `HttpAgent` and the Python client consume the SSE endpoint, and a crewmate task renders end-to-end in the AG-UI Dojo or the terminal client.
   - The inbound `agui` engine runs a real LangGraph or Mastra agent (local), and an interrupt from it parks the task with a card on desktop and phone.
4. **Compatibility.** `PROTO` stays 2 and `kMinHostProto` stays 2. New pushes are behind a cap `agui` with a `push_scope` entry. `protocol_golden.rs` and `testdata/hostd/commands.json` change only additively. An old desktop or phone sees no difference.
5. **Security.**
   - Every SSE request is authenticated.
   - A read-scoped client never receives approval-bearing content.
   - `provide_secret` never travels as an interrupt payload.
   - Runs created from AG-UI input carry `no_auto` until an owner trusts the client.
   - Every decision still goes through `crew::approval::decide` and is audited.
6. **Accounting.** `TokenUsage` conversion tests: totals include cache, absent ≠ 0, cost and duration in metadata.
7. **Docs.** `docs/crew.md` documents the mapping table, the `dev.atlas.*` activity types and CUSTOM names, what is not supported (mid-run answers through AG-UI, steering), and the pinned spec version. The CHANGELOG has an entry.

---

## 10. Open questions

1. Who are the outbound consumers in practice: the company's own web dashboards, CopilotKit embeds in `../ac` / `../zelvo` / `../wi`, or Slack/Teams via channels-sdk? This decides whether A(2) is worth building before A(1).
2. Should a crewmate thread accept client-sent history at all (R4), or only the newest user message?
3. Is a durable interrupt for approvals wanted? Ending the run and killing or suspending the agent, as the Claude Agent SDK adapter's `deferTools` does, would make approvals portable AG-UI interrupts. That trades the 30-minute live hold for resume-from-session. It is a product decision about §15.5, not a protocol one.
4. Can the engines move to streaming output (`claude --output-format stream-json`, `codex exec --json` events, omp RPC events) so `TEXT_MESSAGE_CONTENT` and tool-call ids carry real-time data? [INFERENCE] The flags exist upstream; Atlas does not use them for engine tasks today (`claude.rs:16-18`).
5. Should Atlas weigh in on #2157 (client→server channel) and #2105 (resumption)? Both are exactly the gaps found in §6.

---

## Sources

**AG-UI repo** (`https://github.com/ag-ui-protocol/ag-ui`, commit `1de7b128`):
- `README.md`, `CONTRIBUTING.md`, `DEPRECATIONS.md`
- `spec/README.md`, `spec/RECONCILIATION.md`, `spec/1.0/schema.json`, `spec/1.0/conformance/README.md`
- `docs/spec/1.0/{index,architecture,changelog}.mdx`
- `docs/spec/1.0/basic/{index,run-input,capabilities,versioning,processing,metadata}.mdx`, `docs/spec/1.0/basic/patterns/interrupt-resume.mdx`, `docs/spec/1.0/basic/transports/{index,http-sse,http-protobuf}.mdx`
- `docs/spec/1.0/events/{index,lifecycle,text-messages,tool-calls,reasoning,state,activity,subagents,passthrough}.mdx`, `docs/spec/draft/changelog.mdx`
- `docs/concepts/{middleware,generative-ui-specs,interrupts}.mdx`, `docs/drafts/{overview,meta-events,generative-ui}.mdx`, `docs/agentic-protocols.mdx`, `docs/integrations.mdx`
- `integrations/claude-agent-sdk/typescript/README.md`, `integrations/a2a/typescript/README.md`, `middlewares/mcp-middleware/README.md`
- `sdks/typescript/packages/*/package.json`, `sdks/typescript/packages/client/src/agent/agent.ts`
- `sdks/community/{rust,dart,kotlin,go}`

**Live docs:** https://docs.ag-ui.com/spec/1.0/changelog (fetched 2026-11-20, matches the repo).

**Metrics:**
- GitHub REST API (repo, releases, contributors, commits, issues #369 #1454 #1568 #1593 #2084 #2105 #2106 #2154 #2157 #2186 #2192)
- https://api.npmjs.org/downloads/point/last-week/@ag-ui/core (and `client`, `encoder`)
- https://pypistats.org/api/packages/ag-ui-protocol/recent
- https://crates.io/api/v1/crates/ag-ui-core, https://crates.io/api/v1/crates/ag-ui
- https://pub.dev/api/packages/ag_ui

**Other:**
- AWS AgentCore AG-UI: https://aws.amazon.com/about-aws/whats-new/2026/03/amazon-bedrock-agentcore-runtime-ag-ui-protocol/ (as linked from `docs/integrations.mdx`; not fetched)
- ACP: https://agentclientprotocol.com/get-started/introduction

**Atlas** (`main` `72b47fc3`):
- `crates/atlas-host/src/protocol.rs`
- `crates/atlas-host/src/crew/{mod.rs,events.rs,approval.rs,mcp.rs}`
- `crates/atlas-host/src/crew/store/mod.rs`
- `crates/atlas-host/src/crew/runner/{mod.rs,conversation.rs,action.rs,outcome.rs,tape.rs}`
- `crates/atlas-host/src/crew/engine/{mod.rs,launch.rs,claude.rs,codex.rs,omp.rs,opencode.rs}`
- `crates/atlas-host/src/agents/{mod.rs,hook_server.rs}`
- `crates/atlas-host/src/crew/relay/mod.rs`, `crates/atlas-host/src/mobile/mod.rs`
- `apps/mobile/lib/core/protocol/protocol_version.dart`, `apps/mobile/lib/features/crew/crew_provider.dart`, `apps/desktop/src/lib/stores/crew.svelte.ts`

**Prior study:** `origin/docs/crew-agent-platforms-study:docs/studies/crew-agent-platforms/README.md` (PR #669; used for orientation only; its A4 and C5 items are referenced).
