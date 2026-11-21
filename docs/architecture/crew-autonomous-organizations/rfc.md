# RFC: Atlas Crew: Autonomous Organizations & Agent Operating System

**Status:** Proposed  
**Type:** Architecture / Product / Research / Roadmap  
**Audience:** Atlas Crew engineering agents, architecture agents, product research agents, UX/AX agents  
**Target:** Next major evolution of Atlas Crew  
**Date:** 2026-11-20  
**Owners:** Atlas / CodeAtlas Crew  
**Related study:** `docs/studies/crew-agent-platforms/`  
**Related work:** Crew runtime, hostd, crewmates, task execution, approvals, integrations, scheduling, memory, desktop/mobile surfaces

---

## 0. Executive Summary

Atlas Crew should evolve beyond a platform for creating and operating individual AI agents into a platform for **building, configuring, and operating autonomous organizations of agents**.

The intended experience is not merely:

> Create several agents and ask them to collaborate.

It is:

> Describe a business, its goals, systems, constraints, and operating model. Atlas Crew analyzes it, proposes an organization of agents, establishes responsibilities and policies, connects the required capabilities, simulates the proposed organization, and, after authorization, operates it continuously. Agents perform work, coordinate with one another, detect events and opportunities, delegate, recover from interruptions, communicate meaningful results, and escalate decisions to humans when appropriate.

The human remains the owner, strategic authority, and final decision-maker for actions outside delegated authority.

The strategic opportunity is to combine:

1. **A trustworthy agent runtime**: isolation, execution, tool capabilities, authority, recovery, and durable runs.
2. **An organization model**: goals, departments, responsibilities, agents, policies, shared knowledge, and resources.
3. **An autonomous work system**: events, proactive work, delegation, handoffs, scheduling, and continuous operation.
4. **A human-agent operating experience**: organizational overview, work visibility, approvals, reports, and intelligent notifications.
5. **A composable capability ecosystem**: MCP, native integrations, APIs, browser/computer capabilities, external agents, and reusable skills.

This RFC defines the target capabilities, research requirements, architectural questions, workstreams, acceptance criteria, and phased roadmap. It is intentionally not a final implementation specification for every subsystem. Agents assigned implementation work must inspect the existing repository, identify current primitives, and propose the smallest compatible evolution.

---

## 1. Motivation

The current agent-platform landscape is moving from conversational assistants and task executors toward persistent, proactive, goal-oriented agents and agent workspaces.

Atlas Crew already has important foundations that differentiate it:

- Crewmate identity and execution environments.
- Agent isolation and Linux-user boundaries.
- Host-level authority and hostd-mediated controls.
- Tool and integration execution.
- Approval and security rails.
- GitHub and development workflows.
- Runtime lifecycle management.
- Desktop and mobile surfaces.
- Local/self-hosted infrastructure direction.
- Existing task, scheduling, and agent concepts.

However, operating a company through autonomous agents requires capabilities beyond the individual-agent runtime.

The next strategic question is:

> Can Atlas Crew configure and operate a coordinated organization of agents that continuously performs meaningful work for a business, while remaining safe, understandable, observable, and controllable?

This requires studying and implementing organizational orchestration rather than only adding individual agent features.

---

## 2. Vision

### 2.1 Product vision

**Atlas Crew is an operating system for autonomous agent organizations.**

A human or development agent can describe an organization and its desired outcomes. Crew can analyze available systems and knowledge, propose an organization, create agents and responsibilities, configure policies and integrations, simulate operations, and activate the organization.

Once active, the organization should:

- Understand its mission and goals.
- Know its responsibilities and boundaries.
- Observe relevant systems and events.
- Detect work, risks, opportunities, and exceptions.
- Prioritize work.
- Execute autonomously within delegated authority.
- Delegate work to specialized agents.
- Coordinate across departments.
- Maintain durable context and organizational knowledge.
- Verify outcomes.
- Recover safely from failures and interruptions.
- Report meaningful outcomes rather than every internal action.
- Ask humans for decisions when required.
- Improve operational procedures through explicit, reviewed feedback loops.

### 2.2 Non-goals

This initiative does **not** mean:

- Every agent must always be running.
- Every agent should have unrestricted access to every system.
- Agents should autonomously change their own permissions.
- All workflows should be represented as a visual workflow canvas.
- Crew should replace existing ERP, CRM, accounting, or business systems.
- Agent-to-agent communication should be unrestricted natural-language chatter.
- A successful model response is equivalent to a successful business outcome.
- Autonomy means removing human accountability.
- All external actions should be retried automatically.
- All agents should share one unrestricted memory store.

### 2.3 Product principle

> Give agents enough autonomy to produce value, enough structure to coordinate, enough authority to execute their responsibilities, and enough observability for humans to remain in control.

---

## 3. Strategic Outcome

The target milestone is an **Autonomous Organization** that can be configured from a business description and safely operate within delegated boundaries.

Example user request:

> Configure Atlas Crew for my label manufacturing company. Understand our commercial, production, finance, customer service, and marketing operations. Analyze our existing systems and documents. Recommend an agent organization, responsibilities, integrations, and autonomy policies. Simulate the organization before activation. Once approved, begin operating and report meaningful outcomes, risks, opportunities, and decisions requiring my attention.

Expected system behavior:

1. Discover and analyze the organization.
2. Produce an organization blueprint.
3. Identify required agents, capabilities, integrations, and policies.
4. Identify unknowns and request missing information.
5. Create a draft organization.
6. Simulate likely workflows and actions.
7. Show risks, permissions, and expected cost.
8. Obtain authorization for activation.
9. Activate agents and event subscriptions.
10. Continuously operate, coordinate, report, and recover.

---

## 4. Current-State Assessment

The current competitive study under `docs/studies/crew-agent-platforms/` provides the baseline. Agents must validate all statements against the current repository before making architectural assumptions.

### 4.1 Existing foundations to preserve

Investigate and document the current implementation of:

- Crewmate identity.
- Crewmate runtime and lifecycle.
- Isolation model.
- Linux user boundaries.
- Hostd authority.
- Cgroups and resource controls.
- Network and egress controls.
- Tool execution.
- Approval system.
- Task/run/session model.
- Scheduling.
- Integrations.
- GitHub and development workflows.
- Memory.
- Browser execution.
- Desktop/mobile UI.
- Installation, update, rollback.
- Audit and logging.

### 4.2 Existing features are not assumed to be complete

The RFC does not assume that a feature is production-ready merely because a concept, API, or UI exists.

For each capability, agents must classify it:

- `Implemented and verified`
- `Implemented, incomplete`
- `Prototype`
- `Planned`
- `Not present`
- `Unknown`

Verification should include code references, tests, and runtime behavior where possible.

### 4.3 Required current-state artifact

Create or update:

`docs/architecture/crew-autonomous-organizations/current-state.md`

Include:

- Existing primitives.
- Existing data models.
- Existing APIs.
- Existing runtime behavior.
- Existing security boundaries.
- Gaps.
- Relevant files and code references.
- Compatibility constraints.
- Migration risks.

---

## 5. Target Capability Model

The target platform consists of the following capability layers.

```text
Human / Owner / Operator
        │
        ▼
Organization Experience
  Overview · Goals · Work · Attention · Value
        │
        ▼
Organization Control Plane
  Organization · Departments · Responsibilities
  Policies · Resources · Knowledge · Agent Registry
        │
        ▼
Organization Orchestrator
  Prioritization · Delegation · Routing · Coordination
  Event Evaluation · Escalation · Goal Tracking
        │
        ▼
Durable Execution Plane
  Runs · Checkpoints · Events · Recovery · Idempotency
        │
        ▼
Agent Runtime
  Identity · Context · Memory · Capabilities · Lifecycle
        │
        ▼
Capability Plane
  MCP · Native Integrations · APIs · Browser · Computer
  Files · Shell · External Agents · Skills
        │
        ▼
Host Authority / Security Plane
  Isolation · Policy Enforcement · Secrets · Approvals
  Resource Limits · Audit · Egress · Revocation
```

Cross-cutting systems:

- Organization memory and knowledge.
- Event bus.
- Agent communication.
- Artifacts.
- Observability.
- Notifications.
- Evaluation.
- Cost and resource accounting.
- Human feedback.

---

## 6. Organization Model

### 6.1 Organization as a first-class entity

The organization must not be represented only as a collection of agents.

Proposed conceptual entity:

```typescript
Organization {
  id: OrganizationId;
  name: string;
  description: string;
  mission: string;
  goals: Goal[];
  departments: Department[];
  members: OrganizationMember[];
  agents: CrewmateReference[];
  policies: PolicySet;
  systems: ConnectedSystem[];
  resources: ResourceSet;
  knowledge: KnowledgeSpaceReference[];
  operatingMode: OrganizationOperatingMode;
  status: OrganizationStatus;
  createdAt: Date;
  updatedAt: Date;
}
```

This is illustrative only. Agents must inspect existing models and recommend compatible types, not blindly implement this shape.

### 6.2 Departments

Departments group responsibilities, knowledge, resources, policies, and agents.

A department may contain:

- Mission.
- Goals.
- Manager or coordinator agent.
- Agents.
- Responsibilities.
- Shared knowledge.
- Capabilities.
- Policies.
- Work queues.
- Metrics.
- Event subscriptions.

Departments must be flexible. They are not required to mirror a traditional org chart.

### 6.3 Roles and responsibilities

A role is distinct from an agent instance.

Examples:

- Sales Intelligence.
- Production Planner.
- Financial Analyst.
- Customer Success.
- Executive Coordinator.

Responsibilities should be explicit and machine-readable where possible:

- Responsibility statement.
- Inputs.
- Outputs.
- Scope.
- Decision authority.
- Success criteria.
- Escalation conditions.
- Required capabilities.
- Knowledge domains.
- Ownership boundaries.

### 6.4 Agent lifecycle

Investigate and define:

- Draft.
- Configured.
- Simulated.
- Active.
- Idle.
- Running.
- Waiting.
- Paused.
- Degraded.
- Disabled.
- Archived.

Agents must not be permanently active merely because they exist. Execution should be demand-driven, event-driven, scheduled, or explicitly persistent where justified.

---

## 7. Organization Architect / Agentic Onboarding

### 7.1 Goal

A user or coding agent should be able to configure an organization through a natural-language goal and guided discovery.

Example:

> Configure Crew to operate our company. Analyze our existing systems and documents, understand our processes, propose an organization, create the necessary agents, and prepare everything for activation.

### 7.2 Discovery process

The Organization Architect should:

1. Identify company type and mission.
2. Analyze connected systems.
3. Analyze documents and knowledge sources.
4. Identify major workflows.
5. Identify departments and responsibilities.
6. Identify recurring work.
7. Identify event sources.
8. Identify opportunities for automation.
9. Identify risks and sensitive actions.
10. Identify required capabilities.
11. Identify unknowns.
12. Ask targeted questions only when necessary.

### 7.3 Blueprint

The output should be a versioned blueprint:

```text
Organization Blueprint
├── Business Overview
├── Mission and Goals
├── Departments
├── Agent Roles
├── Responsibilities
├── Workflows
├── Systems and Integrations
├── Required Capabilities
├── Policies
├── Autonomy Profiles
├── Event Sources
├── Knowledge Sources
├── Reporting Plan
├── Expected Outcomes
├── Risks
├── Unknowns
├── Estimated Cost / Resource Usage
└── Activation Plan
```

### 7.4 Blueprint operations

Users must be able to:

- Accept.
- Reject.
- Edit.
- Regenerate selected sections.
- Compare revisions.
- Simulate.
- Activate.
- Roll back.
- Export.

The blueprint must not silently create unrestricted agents or grant permissions.

### 7.5 Simulation / dry-run

Before activation, simulate representative scenarios:

- Normal work.
- Missing information.
- Tool failure.
- Approval-required action.
- Conflicting responsibilities.
- Duplicate events.
- Agent unavailable.
- External system unavailable.
- Uncertain outcome.
- Sensitive information access.

Simulation should produce:

- Proposed actions.
- Delegations.
- Required permissions.
- Expected reports.
- Risks.
- Human interventions.
- Estimated usage.

---

## 8. Agent Identity and Operating Context (AX)

Agent Experience (AX) is a first-class design domain.

An agent should have a structured operating context rather than relying on a monolithic system prompt.

Conceptual context:

```text
Agent Identity
├── Name
├── Role
├── Mission
├── Responsibilities
├── Goals
├── Department
├── Reporting Relationships
├── Delegation Relationships
├── Authority
├── Capabilities
├── Policies
├── Memory
├── Knowledge
├── Environment
├── Operating Schedule
├── Success Criteria
└── Escalation Rules
```

### 8.1 AX requirements

Agents should be able to answer:

- Who am I?
- What is my mission?
- What am I responsible for?
- What is outside my scope?
- What are my current goals?
- Who do I report to?
- Who can I delegate to?
- What can I execute autonomously?
- What requires approval?
- What information may I access?
- What should I prioritize?
- What constitutes success?
- When should I ask for help?
- When should I notify a human?
- How do I verify my work?

### 8.2 AX observability

Agents should expose a human-readable and machine-readable status:

- Current objective.
- Current run.
- Current action.
- Current blocker.
- Waiting reason.
- Recent outcomes.
- Available capabilities.
- Authority summary.
- Resource usage.
- Confidence or uncertainty, where meaningful.
- Health.
- Last meaningful activity.

Do not expose chain-of-thought. Observability should consist of plans at an appropriate level, actions, decisions, evidence, outcomes, and structured summaries.

---

## 9. Agent Communication Protocol

### 9.1 Communication types

Distinguish:

1. Message: informational communication.
2. Request: asks another agent or department for information/action.
3. Task: transfers a defined work item.
4. Delegation: assigns responsibility.
5. Handoff: transfers ownership of a workflow.
6. Escalation: asks a higher-authority agent or human to resolve a blocker.
7. Event: announces an occurrence.
8. Broadcast: notifies multiple subscribers.
9. Response: answers a request.
10. Report: summarizes outcome or insight.

Natural-language conversation alone is not a sufficient communication substrate.

### 9.2 Message envelope

Investigate a compatible schema resembling:

```typescript
AgentMessage {
  id;
  organizationId;
  sender;
  recipient;
  type;
  subject;
  payload;
  contextReferences;
  priority;
  correlationId;
  parentRunId;
  taskId;
  requiresResponse;
  deadline;
  createdAt;
  expiresAt;
  status;
}
```

### 9.3 Context transfer

Messages should reference context rather than indiscriminately copy entire conversations.

Context references may include:

- Run.
- Task.
- Artifact.
- Customer.
- Event.
- Knowledge record.
- System entity.
- Document.
- Approval.
- Goal.

The receiving agent must receive sufficient context to perform the delegated work, but access must remain policy-controlled.

### 9.4 Routing

Routing should support:

- Agent ID.
- Department.
- Role.
- Capability.
- Responsible agent.
- Organization coordinator.
- Human owner.
- Broadcast subscriptions.

### 9.5 Communication safety

Investigate:

- Message authentication.
- Sender identity.
- Authorization to request work.
- Delegation limits.
- Message size.
- Rate limits.
- Cycles.
- Spam.
- Prompt injection.
- Untrusted content.
- Context poisoning.
- Infinite delegation.
- Expired messages.
- Duplicate delivery.

### 9.6 Acceptance criteria

- Agents can communicate without sharing unrestricted memory.
- A task request is auditable.
- The recipient can accept, reject, or request clarification.
- Delegation has an owner and status.
- Context references are permission-checked.
- Communication supports durable delivery.
- Duplicate messages are handled safely.
- Delegation loops are bounded.
- Humans can inspect communication history.

---

## 10. Delegation, Handoffs, and Dynamic Orchestration

### 10.1 Delegation

Delegation should be a structured operation.

A delegation includes:

- Parent run.
- Delegating agent.
- Recipient.
- Objective.
- Scope.
- Context.
- Expected output.
- Deadline.
- Priority.
- Permissions.
- Budget.
- Acceptance criteria.
- Cancellation policy.

### 10.2 Handoff

A handoff transfers ownership of a workflow or task.

Examples:

- Sales closes an opportunity → onboarding owns the next steps.
- Production detects delay → customer communication agent takes responsibility for affected customers.
- Finance detects overdue payment → customer success follows up.
- Marketing detects an opportunity → commercial agent evaluates it.

### 10.3 Dynamic delegation

The system may select agents based on:

- Role.
- Capabilities.
- Availability.
- Load.
- Expertise.
- Permissions.
- Cost.
- Department.
- Historical performance.
- Required context.

The orchestration layer must not rely only on hardcoded agent names.

### 10.4 Coordination

Investigate coordination primitives:

- Task queues.
- Agent availability.
- Reservations.
- Ownership.
- Locks or leases.
- Shared resource conflicts.
- Dependency graphs.
- Parallel work.
- Join conditions.
- Cancellation propagation.
- Deadlock detection.
- Delegation depth.
- Concurrency limits.

### 10.5 Acceptance criteria

- Agents can delegate work to authorized agents.
- Tasks have durable ownership.
- The system can track parent/child work.
- A parent can receive completion or failure.
- Cancellation propagates according to policy.
- Parallel work is bounded.
- Duplicate assignments are avoided.
- Delegation cycles are detected or bounded.
- The human can inspect the delegation graph.

---

## 11. Durable Runs and Work Units

A durable `Run` is a critical research and architecture question.

The current task/session/run concepts must be examined to determine whether they are sufficient or need a formal execution primitive.

### 11.1 Conceptual run lifecycle

```text
Created
  ↓
Queued
  ↓
Running
  ├── WaitingApproval
  ├── WaitingInput
  ├── WaitingForAgent
  ├── Paused
  ├── Recovering
  └── Completed
        ├── Succeeded
        ├── Failed
        ├── Cancelled
        └── OutcomeUncertain
```

### 11.2 Run data

Investigate:

- Input.
- Context references.
- Agent identity/version.
- Organization.
- Objective.
- Plan summary.
- Events.
- Tool calls.
- Approvals.
- Checkpoints.
- Artifacts.
- Delegations.
- Usage.
- Outcome.
- Failure.
- Recovery.
- Audit.

### 11.3 Durable execution

Research and compare principles from Temporal, Restate, Inngest, and similar systems:

- Durable state.
- Replay.
- Idempotency.
- Retry policies.
- Timeouts.
- Cancellation.
- Checkpoints.
- Compensation.
- Human-in-the-loop.
- Recovery after process/host failure.
- External side-effect handling.

Do not automatically introduce a workflow engine. Determine which principles should be implemented in Crew and which should be delegated to an existing component.

### 11.4 Uncertain external outcomes

A tool call may execute successfully while the client experiences a timeout.

The system must distinguish:

- Not executed.
- Executed and confirmed.
- Executed but response lost.
- Execution outcome uncertain.
- Safe to retry.
- Requires reconciliation.

Never blindly retry potentially non-idempotent external actions.

---

## 12. Autonomous Work Loop

Agents should support work triggered by events, schedules, explicit goals, and delegated tasks.

Conceptual loop:

```text
Observe
  ↓
Normalize
  ↓
Evaluate Relevance
  ↓
Create / Update Work
  ↓
Prioritize
  ↓
Plan
  ↓
Execute or Delegate
  ↓
Verify
  ↓
Record Outcome
  ↓
Report
  ↓
Learn / Update Knowledge
```

### 12.1 Trigger types

- User request.
- Agent message.
- Organization event.
- System event.
- External webhook.
- MCP event/trigger.
- Native integration event.
- Schedule.
- Heartbeat.
- Deadline.
- Goal review.
- Follow-up.
- Condition.
- Manual activation.

### 12.2 Event-driven execution

Investigate:

- Event bus.
- Event schema.
- Subscription.
- Filtering.
- Routing.
- Debouncing.
- Aggregation.
- Deduplication.
- Delivery guarantees.
- Dead-letter handling.
- Replay.
- Retention.
- Event correlation.
- Backpressure.
- Rate limits.

### 12.3 Proactivity

Agents should not create unlimited work.

Proactive work must be bounded by:

- Mission.
- Goal.
- Policy.
- Schedule.
- Budget.
- Relevance.
- Priority.
- Confidence.
- Existing work.
- Cooldown.
- Human notification policy.

### 12.4 Example

Commercial agent subscriptions:

```text
New customer
Customer inactive for N days
Quotation expires
Customer email received
CRM opportunity updated
Production delay affecting customer
Daily sales review
Goal progress review
```

The agent should evaluate whether an event warrants work before initiating a run.

---

## 13. Organization Goals and Goal Tracking

Organizations should have goals that agents can understand and operate against.

Examples:

- Reduce overdue receivables.
- Increase repeat purchases.
- Reduce production delays.
- Improve customer response time.
- Reduce manual data entry.
- Increase qualified leads.

A goal should include:

- Name.
- Description.
- Owner.
- Metric.
- Baseline.
- Target.
- Time horizon.
- Scope.
- Supporting agents.
- Policy constraints.
- Review cadence.
- Status.
- Evidence.
- Outcome.

### 13.1 Goal hierarchy

```text
Organization Goal
    ↓
Department Goal
    ↓
Agent Goal
    ↓
Task / Run
```

Goals should not be interpreted as unlimited authority.

A goal to increase revenue does not grant permission to send arbitrary messages, issue discounts, spend money, or change customer data.

### 13.2 Goal review

Agents may periodically:

- Measure progress.
- Identify blockers.
- Propose new work.
- Recommend adjustments.
- Report evidence.

Goal changes should be versioned and governed.

---

## 14. Organizational Memory and Knowledge

Memory is not a single shared prompt or file.

### 14.1 Memory types

- Human preference memory.
- Agent-specific memory.
- Department memory.
- Organization memory.
- Customer/entity memory.
- Operational memory.
- Episodic memory.
- Procedural memory.
- Decision memory.
- Skill memory.

### 14.2 Knowledge records

Investigate a structured record containing:

- ID.
- Type.
- Content.
- Source.
- Provenance.
- Owner.
- Scope.
- Permissions.
- Confidence.
- Validity.
- Created/updated.
- Expiration.
- Related entities.
- Evidence.
- Review status.

### 14.3 Memory governance

Requirements:

- Provenance.
- Permissions.
- Temporal validity.
- Confidence.
- Contradiction handling.
- Retraction.
- Expiration.
- Human correction.
- Access control.
- Sensitive data classification.
- Audit.
- Prevent context poisoning.

### 14.4 Knowledge sharing

Agents should be able to reference shared knowledge without receiving unrestricted access to every memory record.

Knowledge should be scoped by:

- Organization.
- Department.
- Agent.
- Customer.
- Project.
- Run.
- Permission.

---

## 15. Authority, Autonomy, and Policy System

### 15.1 Policy hierarchy

Investigate composable policy layers:

```text
Platform Policy
    ↓
Organization Policy
    ↓
Department Policy
    ↓
Agent Policy
    ↓
Task Policy
    ↓
Action Policy
```

Policies must be evaluated by a trusted authority, not merely interpreted by model instructions.

### 15.2 Autonomy profiles

Potential profiles:

- Advisory.
- Assisted.
- Approval-based.
- Bounded autonomous.
- Fully autonomous within explicit scope.

These labels must have precise semantics.

### 15.3 Policy examples

```text
Commercial Agent:
  Read CRM: Allowed
  Create quote: Allowed
  Send customer email: Allowed
  Discount ≤ 5%: Allowed
  Discount > 5%: Approval required
  Commit delivery date: Requires operations validation

Finance Agent:
  Read invoices: Allowed
  Reconcile: Allowed
  Prepare payment: Allowed
  Execute payment: Approval required

Operations Agent:
  Re-plan internal schedule: Allowed
  Change customer delivery commitment: Approval required
```

### 15.4 Capability-based authority

Research capability-based security:

- Capabilities.
- Delegation.
- Revocation.
- Leases.
- Scoped access.
- Resource identity.
- Action constraints.
- Policy evaluation.
- Host authority.

### 15.5 Approval binding

Approvals should bind to the exact proposed action, including relevant:

- Action.
- Parameters.
- Target.
- Amount.
- Recipient.
- Context.
- Policy.
- Expiration.
- Run.
- Agent.
- Version.

Changing material action details should invalidate the approval.

### 15.6 Authority invariants

- Agents cannot grant themselves permissions.
- Delegation cannot exceed the delegator's authority.
- Policy changes are auditable.
- Revocation takes effect promptly.
- Approval is not a generic authorization to perform unrelated actions.
- Human approval must be explicit and attributable.
- Sensitive operations require trusted enforcement.

---

## 16. Reporting and Notification Intelligence

The system must not notify users about every internal action.

### 16.1 Report categories

- Completed work.
- Important progress.
- Attention required.
- Approval required.
- Blocker.
- Risk.
- Opportunity.
- Insight.
- Failure.
- Uncertain outcome.
- Periodic digest.

### 16.2 Notification decision

Evaluate:

- Importance.
- Urgency.
- Impact.
- Confidence.
- User preference.
- Required action.
- Duplicate reports.
- Current notification load.
- Quiet hours.
- Channel.
- Escalation.

### 16.3 Report structure

```text
Report
├── Title
├── Category
├── Summary
├── Impact
├── Evidence
├── Actions Taken
├── Actions Pending
├── Decision Required
├── Related Runs
├── Related Agents
├── Artifacts
└── Suggested Next Step
```

### 16.4 Notification channels

Investigate:

- Crew UI.
- Push.
- Email.
- WhatsApp.
- Slack.
- Discord.
- Other integrations.

Channels should be optional and policy-controlled.

### 16.5 Digest

Support:

- Daily.
- Weekly.
- Department.
- Goal.
- Exception.
- Activity.
- Value.
- Pending decisions.

---

## 17. Organization Experience (UX)

The product should make the organization understandable at a glance.

### 17.1 Core surfaces

1. Organization Overview.
2. Organization Structure.
3. Agents.
4. Work.
5. Goals.
6. Attention.
7. Reports.
8. Knowledge.
9. Systems and Integrations.
10. Policies.
11. Activity.
12. Simulation.
13. Organization Setup.

### 17.2 Organization overview

Show:

- Organization health.
- Active agents.
- Work in progress.
- Completed work.
- Blockers.
- Human decisions.
- Important events.
- Goal progress.
- Opportunities.
- Risks.
- Value produced.
- Resource usage.

Avoid making the dashboard merely a count of model calls or tool calls.

### 17.3 Work view

A unified work surface should represent:

- Tasks.
- Runs.
- Delegations.
- Handoffs.
- Approvals.
- Events.
- Artifacts.
- Goals.

Users should be able to inspect the relationship between work items.

### 17.4 Attention

A prioritized inbox of:

- Decisions.
- Approvals.
- Escalations.
- Risks.
- Uncertain outcomes.
- Blockers.
- Important reports.

The user should not need to inspect every agent.

### 17.5 Organization graph

Investigate an interactive visualization of:

- Departments.
- Agents.
- Responsibilities.
- Reporting.
- Delegation.
- Work dependencies.
- Integrations.

The graph is a visualization, not the authoritative data model.

---

## 18. Agent Experience (AX)

AX should be treated as a product and architecture discipline.

### 18.1 Agent operating context

Agents receive structured context assembled from:

- Identity.
- Role.
- Mission.
- Goals.
- Responsibilities.
- Policies.
- Capabilities.
- Relationships.
- Memory.
- Knowledge.
- Environment.
- Current run.
- Relevant events.
- Success criteria.

### 18.2 Agent status

Agents should expose:

- Idle.
- Working.
- Delegating.
- Waiting.
- Waiting for approval.
- Blocked.
- Recovering.
- Reporting.
- Completed.
- Failed.
- Disabled.

### 18.3 Agent health

Health must not be inferred solely from process existence.

Consider:

- Runtime health.
- Model provider.
- Tool availability.
- Queue health.
- Memory access.
- Event delivery.
- Last successful work.
- Error rate.
- Resource usage.
- Policy violations.
- Stuck runs.

### 18.4 Feedback

Human feedback may apply to:

- Report quality.
- Outcome quality.
- Delegation.
- Priority.
- Policy.
- Memory.
- Procedure.
- Skill.

Feedback must not silently alter permissions or policies.

---

## 19. Capability Ecosystem

Agents should be assembled from composable capabilities.

### Capability categories

- MCP.
- Native integrations.
- API tools.
- Browser.
- Computer use.
- Files.
- Shell.
- Database.
- Messaging.
- Calendar.
- Email.
- CRM.
- ERP.
- Skills.
- External agents.
- Internal agent services.

### Capability registry

Investigate:

- Discovery.
- Versioning.
- Health.
- Permissions.
- Tool schemas.
- Deferred loading.
- Context cost.
- Rate limits.
- Credential requirements.
- Ownership.
- Lifecycle.
- Revocation.
- Audit.

### MCP research

MCP must be evaluated as a capability protocol, not merely an integration checkbox.

Research:

- Server lifecycle.
- Transport.
- Discovery.
- OAuth.
- Credential handling.
- Tool schemas.
- Schema changes.
- Tool filtering.
- Tool loading.
- Context budget.
- Timeouts.
- Cancellation.
- Reconnect.
- Health.
- Approval.
- Rate limits.
- Audit.
- Isolation.
- Untrusted servers.
- Prompt injection.

---

## 20. Integration and External-Agent Interoperability

The organization should not require all agents to be native Crew agents.

Investigate interoperability with:

- MCP servers.
- External agent runtimes.
- Existing Claude Code agents.
- Codex.
- OpenAI-compatible agents.
- Webhooks.
- APIs.
- Existing automation platforms.
- n8n.
- Other workflow systems.

The integration contract should define:

- Identity.
- Capability.
- Input/output.
- Context.
- Authentication.
- Authority.
- Status.
- Artifacts.
- Events.
- Cost.
- Timeout.
- Failure.
- Cancellation.
- Audit.

Crew should remain the organization/control plane where appropriate, without requiring every execution capability to be rebuilt.

---

## 21. Observability and Audit

### 21.1 Organization activity

Track meaningful events:

- Run started.
- Run completed.
- Agent delegated.
- Task assigned.
- Approval requested.
- Approval granted/denied.
- Policy blocked action.
- Integration failure.
- Recovery.
- Report generated.
- Goal progress.
- Memory updated.
- Agent activated/disabled.

### 21.2 Trace

A user should be able to inspect:

```text
Organization Event
    ↓
Agent Run
    ↓
Decision Summary
    ↓
Delegation
    ↓
Child Run
    ↓
Tool Calls
    ↓
Approval
    ↓
External Action
    ↓
Verification
    ↓
Outcome
```

Do not expose hidden chain-of-thought. Store and present:

- Intent.
- Plan summary.
- Decision.
- Evidence.
- Action.
- Result.
- Verification.
- Policy outcome.

### 21.3 Audit

Audit must include:

- Actor.
- Agent.
- Organization.
- Run.
- Action.
- Target.
- Authority.
- Policy.
- Approval.
- Timestamp.
- Result.
- Evidence.
- Correlation.

---

## 22. Value Measurement

Crew should measure outcomes, not merely activity.

Possible value metrics:

- Tasks completed.
- Time saved.
- Work items resolved.
- Revenue opportunities discovered.
- Customer response time.
- Errors prevented.
- Risks detected.
- Process cycle time.
- Human interventions.
- Approval latency.
- Cost.
- Outcome quality.
- Goal progress.

Do not claim monetary value without evidence and a transparent methodology.

### Value report

```text
Period: This week

Work:
  126 tasks completed
  17 tasks delegated
  9 escalations

Commercial:
  4 opportunities identified
  2 quotations prepared

Operations:
  3 delivery risks detected
  2 resolved

Finance:
  18 reconciliations completed

Human time:
  Estimated 6.5 hours saved
  Method: documented task baseline

Cost:
  Model usage
  Infrastructure
  Integrations
```

---

## 23. Evaluation and Organization Simulation

A system of autonomous agents requires evaluation beyond model benchmarks.

### 23.1 Agent evaluations

- Task success.
- Tool success.
- Quality.
- Policy adherence.
- Delegation quality.
- Context quality.
- Recovery.
- Cost.
- Latency.
- Human intervention.
- False-positive proactive work.
- Missed important events.

### 23.2 Organization evaluations

- Goal progress.
- Cross-agent coordination.
- Duplicate work.
- Communication overhead.
- Escalation quality.
- Decision latency.
- Resource allocation.
- Failure recovery.
- Policy violations.
- Human workload.

### 23.3 Simulation

Before activation or major policy changes, simulate:

- Typical workflows.
- Failure scenarios.
- Conflicting tasks.
- Missing data.
- Integration outages.
- Unauthorized requests.
- Prompt injection.
- Approval delays.
- Agent unavailability.
- Cost overruns.
- Delegation loops.

Simulation must clearly indicate that simulated actions were not executed.

---

## 24. Competitive Research Workstream

Research agents should investigate competitors and adjacent systems. Do not treat marketing claims as verified capabilities.

### 24.1 Grok Bot / xAI Agent Experience

Research:

- Persistent agent identity.
- Proactivity.
- Computer use.
- Work monitoring.
- Follow-up.
- Team collaboration.
- Human escalation.
- Notifications.
- Memory.
- Agent lifecycle.
- Autonomy.
- UX.
- AX.

Questions:

- How does work begin?
- What triggers proactive activity?
- How does it monitor ongoing tasks?
- How does it decide to notify?
- How does it decide to ask?
- How does it coordinate?
- How are agents configured?
- What is the permission model?
- What is the recovery model?
- What is the value-reporting model?

### 24.2 OpenBot

Research:

- Workspace.
- Agent composition.
- External agents.
- Human-agent collaboration.
- Governance.
- Approval.
- Organization model.
- Sharing.
- UX.
- Integrations.
- Agent lifecycle.
- Observability.

### 24.3 OpenClaw

Research:

- Gateway.
- Agent identity.
- Routing.
- Bindings.
- Sessions.
- Heartbeats.
- Scheduling.
- Skills.
- Memory.
- Channels.
- Multi-agent isolation.
- Security.
- Runtime lifecycle.

### 24.4 Navin OpenBot

Research:

- Goal-oriented execution.
- Dynamic agent selection.
- Handoffs.
- Parallel execution.
- Artifacts.
- Run replay.
- Cards/skills.
- Decks.
- Agent creation.
- Resource selection.
- Cost.
- UX.
- AX.

### 24.5 Durable execution

Research:

- Temporal.
- Restate.
- Inngest.
- Other relevant durable execution systems.

Extract principles, not necessarily dependencies.

### 24.6 Agent protocols and UI

Research:

- MCP.
- AG-UI.
- Agent-to-agent protocols.
- Events.
- Human-agent interaction.
- Streaming.
- Artifact interaction.

### 24.7 Research evidence

For each claim, classify:

- Vendor documentation.
- Source-code verified.
- Product behavior verified.
- Independent evidence.
- Inference.
- Unknown.

Include:

- Source.
- Date.
- Claim.
- Evidence.
- Confidence.
- Implication for Atlas.

---

## 25. Architecture Research Questions

Before implementation, agents must answer these questions.

### Organization

- Is Organization a first-class entity?
- What is the relationship to current Crew entities?
- Are departments first-class or lightweight groups?
- How are goals represented?
- How are responsibilities represented?
- How is organization versioning handled?

### Agent

- How is role separated from agent identity?
- How is operating context assembled?
- How are capabilities assigned?
- How are policies inherited?
- How are agent versions tracked?
- How is lifecycle handled?

### Runs

- Is the current task/run/session model sufficient?
- What requires durability?
- How are checkpoints stored?
- How are uncertain outcomes represented?
- What is the idempotency model?
- How are parent/child runs related?

### Communication

- Is there an event bus?
- Is messaging durable?
- How are messages routed?
- How are permissions evaluated?
- How are context references resolved?
- How are cycles prevented?

### Autonomy

- What is the policy enforcement point?
- Which authority belongs to hostd?
- Which policy belongs to the organization?
- How are approvals bound?
- How are capabilities revoked?
- How is delegated authority constrained?

### Proactivity

- What is the trigger model?
- What is the scheduling model?
- How are heartbeats implemented?
- How is work deduplicated?
- How are budgets applied?
- How are notifications governed?

### Memory

- What memory systems exist?
- What scopes exist?
- How are permissions enforced?
- How is provenance stored?
- How are contradictions handled?
- How is retention handled?

### UX / AX

- What UI surfaces already exist?
- What is the organization overview?
- What is the work inbox?
- What is the attention inbox?
- How do agents expose status?
- How does setup work?
- How does simulation work?

---

## 26. Proposed Workstreams

Workstreams may be executed by multiple research and implementation agents, but each must produce artifacts and avoid overlapping code changes without coordination.

### WS0 - Repository Architecture Audit

**Objective:** Establish verified current state.

Deliverables:

- Current-state document.
- Existing models/API inventory.
- Runtime inventory.
- Security boundaries.
- Feature matrix.
- Gap analysis.
- Dependency graph.

### WS1 - Organization Model

**Objective:** Define Organization, Department, Goals, Responsibilities, Membership, Resources, and Policy relationships.

Deliverables:

- Domain model.
- API proposal.
- Persistence proposal.
- Migration plan.
- ADR.

### WS2 - Agent AX / Operating Context

**Objective:** Define structured agent identity and operating context.

Deliverables:

- Agent context schema.
- Context assembly.
- Role/responsibility model.
- Goals.
- Capabilities.
- Relationships.
- Memory scope.
- Status and health.

### WS3 - Agent Communication & Delegation

**Objective:** Define durable communication and delegation.

Deliverables:

- Message protocol.
- Routing.
- Task delegation.
- Handoff.
- Escalation.
- Context references.
- Cycle prevention.
- Concurrency.

### WS4 - Durable Runs & Recovery

**Objective:** Define a durable execution primitive and recovery behavior.

Deliverables:

- Run lifecycle.
- Event model.
- Checkpoint strategy.
- Idempotency.
- Uncertain outcomes.
- Retry/reconciliation.
- Cancellation.
- Parent/child execution.

### WS5 - Autonomous Work Loop

**Objective:** Enable proactive, event-driven work.

Deliverables:

- Event schema.
- Trigger system.
- Scheduling.
- Heartbeat strategy.
- Work prioritization.
- Deduplication.
- Cooldowns.
- Budgets.
- Follow-up.

### WS6 - Authority & Autonomy

**Objective:** Define composable, enforceable policies.

Deliverables:

- Policy model.
- Autonomy profiles.
- Capability model.
- Delegation limits.
- Approval binding.
- Revocation.
- Trusted enforcement points.
- Security tests.

### WS7 - Organization Memory

**Objective:** Define scoped, governed organizational knowledge.

Deliverables:

- Memory taxonomy.
- Knowledge schema.
- Provenance.
- Permission model.
- Temporal validity.
- Confidence.
- Contradiction handling.
- Retention.
- Retrieval.

### WS8 - Reporting & Notification Intelligence

**Objective:** Report meaningful outcomes without overwhelming users.

Deliverables:

- Report schema.
- Importance model.
- Notification policy.
- Digest.
- Channels.
- Escalation.
- Deduplication.
- User preferences.

### WS9 - Organization UX

**Objective:** Design organization management and operation surfaces.

Deliverables:

- Organization overview.
- Organization graph.
- Work view.
- Attention inbox.
- Goals.
- Reports.
- Agent detail.
- Policies.
- Integrations.
- Simulation.
- UX prototypes.

### WS10 - Organization Architect / Onboarding

**Objective:** Configure organizations from natural language and existing systems.

Deliverables:

- Discovery process.
- Blueprint.
- Questions.
- Capability matching.
- Policy proposal.
- Simulation.
- Activation.
- Versioning.
- Rollback.

### WS11 - Capability Ecosystem

**Objective:** Make agents composable with tools and external runtimes.

Deliverables:

- Capability registry.
- MCP research.
- Native integration model.
- External-agent contract.
- Credential lifecycle.
- Health.
- Versioning.
- Context budgets.

### WS12 - Observability & Evaluation

**Objective:** Measure system health and actual value.

Deliverables:

- Trace model.
- Audit.
- Agent metrics.
- Organization metrics.
- Evaluation suite.
- Simulation suite.
- Value methodology.

### WS13 - Competitive Research

**Objective:** Deeply benchmark the products and systems relevant to autonomous organizations.

Deliverables:

- Evidence-backed benchmark.
- Architecture comparison.
- UX/AX comparison.
- Feature matrix.
- Strategic implications.
- Recommendations.

---

## 27. Agent Collaboration Protocol for This RFC

Multiple agents may work in parallel.

### 27.1 Research-first

Agents must inspect the repository before proposing changes.

They must not assume the architecture based on this RFC.

### 27.2 Each workstream must produce

1. Scope.
2. Current-state findings.
3. Existing code references.
4. Competitor research.
5. Gaps.
6. Proposed architecture.
7. Alternatives considered.
8. Dependencies.
9. Risks.
10. ADR(s), where applicable.
11. Implementation plan.
12. Acceptance criteria.
13. Open questions.

### 27.3 No premature implementation

Research agents should not modify production code unless explicitly assigned.

Architecture agents should not independently introduce:

- New databases.
- New queues.
- New workflow engines.
- New runtime dependencies.
- New abstractions.

They must first document the rationale and compare against current infrastructure.

### 27.4 Shared artifact structure

Recommended:

```text
docs/architecture/crew-autonomous-organizations/
├── README.md
├── current-state.md
├── target-architecture.md
├── organization-model.md
├── agent-operating-context.md
├── communication-and-delegation.md
├── durable-runs.md
├── autonomous-work-loop.md
├── authority-and-autonomy.md
├── organization-memory.md
├── reporting-and-notifications.md
├── organization-ux.md
├── agent-experience.md
├── organization-onboarding.md
├── capabilities-and-integrations.md
├── observability-and-evaluation.md
├── competitive-benchmark.md
├── roadmap.md
├── open-questions.md
└── adr/
    ├── README.md
    └── ...
```

Research documents may be split further if needed.

---

## 28. Proposed Roadmap

This is a dependency-oriented roadmap, not a commitment to specific release dates.

### Phase 0 - Research and Architecture

- Repository audit.
- Competitive benchmark.
- Current-state inventory.
- Organization model.
- Run/durable execution analysis.
- Communication model.
- Authority model.
- UX/AX research.
- ADRs.

**Exit criteria:** Architecture decisions are documented, current capabilities verified, and implementation scope agreed.

### Phase 1 - Trustworthy Autonomous Runtime

- Durable runs.
- Recovery.
- Uncertain outcomes.
- Idempotency.
- Persistent approvals.
- Approval binding.
- Authority model.
- Capability policy.
- Secret isolation.
- Browser isolation.
- Audit.

**Exit criteria:** Agents can safely execute and recover work within explicit authority.

### Phase 2 - Organization Foundations

- Organization.
- Departments.
- Roles.
- Responsibilities.
- Goals.
- Agent operating context.
- Agent lifecycle.
- Communication.
- Delegation.
- Handoffs.
- Work relationships.

**Exit criteria:** Multiple agents can coordinate durable work within an organization.

### Phase 3 - Autonomous Work

- Event bus.
- Triggers.
- Schedules.
- Heartbeats.
- Work queues.
- Proactive evaluation.
- Follow-ups.
- Goal reviews.
- Budgets.
- Notification intelligence.

**Exit criteria:** Agents can initiate and maintain meaningful work without explicit per-task user requests.

### Phase 4 - Organization Intelligence

- Organization memory.
- Department memory.
- Knowledge provenance.
- Goal tracking.
- Reporting.
- Value measurement.
- Simulation.
- Evaluation.

**Exit criteria:** The organization understands its knowledge, goals, work, and outcomes.

### Phase 5 - Organization Architect

- Business discovery.
- Blueprint generation.
- Agent composition.
- Capability matching.
- Integration setup.
- Policy proposal.
- Dry-run.
- Activation.
- Revision.
- Rollback.

**Exit criteria:** A user or development agent can configure an organization through a guided agentic process.

### Phase 6 - Ecosystem

- External agent interoperability.
- MCP expansion.
- Channels.
- Additional integrations.
- Shared organization workspaces.
- Advanced coordination.
- Reusable skills.
- Cross-organization capabilities where appropriate.

**Exit criteria:** Crew can operate as an extensible platform, not a closed collection of agents.

---

## 29. Acceptance Criteria for the Autonomous Organization Milestone

A milestone should not be considered complete merely because APIs exist.

### Scenario A - Configure an organization

Given a company description and connected knowledge sources:

- Crew generates an organization blueprint.
- Blueprint includes departments, roles, agents, responsibilities, capabilities, policies, goals, and event sources.
- Unknowns are identified.
- User can review and edit.
- Simulation is available.
- Activation requires explicit authorization.

### Scenario B - Cross-agent delegation

Given an agent receives a task outside its responsibility:

- It can identify an appropriate recipient.
- It creates a structured delegation.
- Context is permission-checked.
- The child task has an owner and durable state.
- Parent receives a result.
- Failure or rejection is handled.
- No infinite delegation occurs.

### Scenario C - Proactive work

Given a subscribed event:

- Event is normalized.
- Relevant policy is evaluated.
- Duplicate work is prevented.
- Agent determines whether work is necessary.
- Work is created or ignored with an auditable outcome.
- Work is bounded by budget and policy.

### Scenario D - Human approval

Given an action requiring approval:

- Exact action is shown.
- Context and impact are shown.
- Approval is bound to the action.
- Approval expires.
- Material changes invalidate approval.
- Agent cannot bypass approval.
- Approval is auditable.

### Scenario E - Failure recovery

Given a run is interrupted:

- State is recovered.
- Completed work is not duplicated.
- External uncertain outcomes are reconciled.
- Run status is truthful.
- Human intervention is requested if needed.

### Scenario F - Meaningful reporting

Given multiple runs complete:

- Important outcomes are aggregated.
- Duplicates are suppressed.
- Notification priority is applied.
- User receives an actionable report.
- Report links to evidence and related work.

### Scenario G - Organizational memory

Given one agent discovers a relevant fact:

- Fact has source and provenance.
- It is stored in appropriate scope.
- Authorized agents can retrieve it.
- Unauthorized agents cannot.
- Contradictions and stale knowledge are handled.

### Scenario H - Organization visibility

Human can inspect:

- Organization.
- Departments.
- Agents.
- Current work.
- Delegations.
- Approvals.
- Goals.
- Events.
- Reports.
- Policies.
- Health.
- Audit.

---

## 30. Security Requirements

This initiative increases autonomy and therefore increases the security surface.

Must investigate and preserve:

- Agent isolation.
- Host authority.
- Capability-based permissions.
- Egress controls.
- Resource limits.
- Secret isolation.
- Browser sandbox.
- Prompt-injection handling.
- Untrusted input labeling.
- Context boundaries.
- Approval binding.
- Policy enforcement.
- Revocation.
- Delegation constraints.
- Audit.
- Rate limits.
- Budgets.
- External agent trust.
- MCP server trust.
- Organization-wide access controls.
- Cross-tenant isolation, where applicable.

Security policies must be enforced by trusted components. Model instructions alone are not security boundaries.

---

## 31. Cost and Resource Management

Autonomous organizations can create uncontrolled usage.

Investigate:

- Organization budget.
- Department budget.
- Agent budget.
- Run budget.
- Model budget.
- Tool budget.
- Compute budget.
- Concurrency.
- Delegation budget.
- Proactive-work budget.
- Scheduling limits.
- Cost attribution.

Agents must be able to stop or defer work when limits are reached.

Budget policies must not be bypassed through delegation.

---

## 32. UX/AX Design Principles

### UX

- The user sees outcomes, not internal chatter.
- Work is organized around goals and tasks.
- Important decisions are prioritized.
- Reports are concise and actionable.
- Organization setup is guided.
- Agents are understandable.
- Authority is visible.
- Simulation precedes activation.
- Users can inspect evidence.
- Users can pause, disable, or revoke.

### AX

- Agents have explicit identity and responsibility.
- Context is structured.
- Delegation is clear.
- Capabilities are discoverable.
- Memory is scoped.
- Status is truthful.
- Errors are recoverable.
- Authority is explicit.
- Goals are measurable.
- Feedback is attributable.
- Agents understand their boundaries.

### Design principle

> The system should be sophisticated internally but feel calm and simple externally.

---

## 33. Open Questions

These must be answered during research and architecture work.

### Organization

- Is Organization a new entity or an extension of existing Crew structures?
- Are departments hierarchical?
- Can agents belong to multiple departments?
- Can one agent serve multiple organizations?
- How are organization versions managed?

### Runs

- Is the current run/task model sufficient?
- What persistence guarantees exist?
- What is the recovery mechanism?
- What is the event log?
- How are external side effects reconciled?

### Orchestration

- Central orchestrator, distributed coordination, or hybrid?
- Who assigns work?
- Can agents self-assign?
- How are concurrency and resources managed?
- What is the maximum delegation depth?

### Communication

- Durable messages?
- Event bus?
- Request/response?
- Shared task graph?
- Agent sessions?
- How much context is transferred?

### Autonomy

- What is the policy evaluation engine?
- Which policies are enforced by hostd?
- How is authority delegated?
- How is it revoked?
- How are approvals represented?

### Proactivity

- Heartbeat vs event-driven?
- How are events normalized?
- How are duplicate triggers prevented?
- What are budgets and cooldowns?

### Memory

- Existing memory architecture?
- Organization-level storage?
- Knowledge graph or structured records?
- Vector search?
- Provenance?
- Permission model?

### UX

- What is the primary workspace?
- Organization graph?
- Work inbox?
- Attention inbox?
- Reports?
- Mobile experience?
- Notification strategy?

### Interoperability

- Can external agents participate?
- What contract is required?
- How are identity and authority handled?
- MCP?
- AG-UI?
- A2A?
- Webhooks?

---

## 34. Recommended First Agent Assignment

The first agent should **not implement the entire RFC**.

Recommended assignment:

> Audit the current Atlas Crew repository against this RFC. Identify existing primitives, verified capabilities, partial implementations, missing capabilities, and architectural constraints. Produce a current-state report with file references and a dependency graph. Do not modify production code. Then recommend the smallest architecture evolution that enables Autonomous Organizations without introducing unnecessary new infrastructure.

Subsequent agents can be assigned to:

1. Competitive research.
2. Organization domain model.
3. Durable execution.
4. Agent communication and delegation.
5. Autonomy and authority.
6. Proactive work.
7. Organization memory.
8. UX/AX.
9. Organization Architect onboarding.
10. Observability and evaluation.

---

## 35. Final Strategic Position

Atlas Crew should not attempt to win by copying every feature from competing agent products.

Its potential differentiation is:

- Agents with real execution environments.
- Strong isolation.
- Host-controlled authority.
- User-owned infrastructure.
- Composable capabilities.
- Durable execution.
- Organizational coordination.
- Proactive work.
- Clear UX.
- Strong AX.
- Open interoperability.

The evolution should be:

```text
Agent Runtime
    ↓
Agent Platform
    ↓
Multi-Agent System
    ↓
Autonomous Organization
    ↓
Agent Operating System
```

The ultimate product promise:

> Describe what your organization needs to accomplish. Atlas Crew builds the agent organization, connects its capabilities, establishes its responsibilities and boundaries, and operates it continuously. The agents coordinate, execute, recover, learn from approved feedback, and report meaningful value, while humans retain strategic control and authority over consequential decisions.

The most important architectural principle:

> **Autonomy is not the absence of control. Autonomy is the ability to operate continuously within explicit, enforceable boundaries.**

The next release should move Crew toward this vision by strengthening the foundations of durable execution, organization-aware coordination, proactive work, authority, and meaningful human-agent interaction.

---

## Appendix A - Suggested Research Deliverable Template

Each research agent should use:

```markdown
# [Research Topic]

## Executive Summary

## Current Atlas Implementation
- Verified:
- Partial:
- Missing:
- Unknown:

## Competitor Findings
- Product:
- Capability:
- Evidence:
- Confidence:

## Architectural Concepts

## Options
### Option A
### Option B
### Option C

## Recommendation

## Dependencies

## Risks

## Open Questions

## Proposed ADR

## Implementation Plan

## Acceptance Criteria

## References
```

---

## Appendix B - Suggested ADR Template

```markdown
# ADR-NNN: [Decision]

## Status
Proposed

## Context

## Problem

## Decision Drivers

## Considered Options

## Decision

## Consequences

## Security Implications

## Migration

## Open Questions
```

---

## Appendix C - Research Quality Rules

1. Inspect source code and current runtime behavior when available.
2. Do not assume marketing claims are implementation details.
3. Label inference.
4. Include source URLs and research dates.
5. Avoid introducing dependencies before architecture review.
6. Preserve current security boundaries.
7. Prefer incremental compatible changes.
8. Identify shared primitives before building new subsystems.
9. Do not implement features merely because competitors have them.
10. Prioritize capabilities that produce measurable user value.
11. Treat autonomy, authority, reliability, and UX as one integrated system.
12. Every proposed capability must have an acceptance criterion.
