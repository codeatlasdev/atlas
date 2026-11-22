# Architecture decision records

Proposed decisions behind [recommended-architecture.md](../recommended-architecture.md), in the template of the RFC's Appendix B. A decision moves from Proposed to Accepted when the maintainer approves it in the pull request or in the issue that implements it.

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-policy-files-owned-by-hostd.md) | Policy lives in files hostd owns, separate from `crew.json` | Proposed |
| [0002](0002-delegation-is-a-task.md) | A delegation is a task, not a message | Proposed |
| [0003](0003-ag-ui-at-the-edge.md) | AG-UI at the edge, not as hostd's own protocol | Proposed |
| [0004](0004-no-new-task-states-before-tolerant-clients.md) | No new task states before clients tolerate unknown values | Proposed |
