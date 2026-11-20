# OpenBot (openbot.run): deep study

Checked 2026-11-20. Sources: the site in pt and en (every sitemap page type), the public repo `nightly-labs/openbot` at commit `fc628c4` (cloned; README, `docs/`, `docs/architecture/`, `CHANGELOG.md`, `SECURITY.md`, `PRIVACY.md`, selected source), GitHub API, App Store, Google Play, and third-party reviews.

> **Name collision.** This report covers **Nightly Labs' OpenBot** (openbot.run, `github.com/nightly-labs/openbot`). Another, unrelated "OpenBot" exists: `github.com/CopilotKit/OpenBot` (MIT, about 13k stars, created 2026-08-17, "self-hostable alternative to closed AI bots"; [repo](https://github.com/CopilotKit/OpenBot), [CopilotKit blog](https://www.copilotkit.ai/blog/introducing-openbot-1-0)). Third-party articles mix the two up ([iadecider](https://iadecider.com/articles/openbot-vps-review) says so too).

---

## 1. What it is, who it is for, business model, maturity

**One line:** a free (non-commercial), source-available, local-first **Electron desktop app that turns the coding-agent CLIs you already pay for (Codex, Claude Code, Gemini/Antigravity, Grok, Cursor, Cline, OpenCode, Kimi, Devin, Copilot, Mistral, Hermes, Pi, any ACP agent, OpenAI-compatible/Ollama/LM Studio) into a team of persistent, named agents**. Each agent has its own workspace, thread, queue and identity. You reach them from desktop, web, iOS, Android, Slack, Discord, Telegram and WhatsApp. The same build runs headless as an always-on Linux server, and the vendor also sells hosted servers. ([home](https://openbot.run/), [README](https://github.com/nightly-labs/openbot#readme))

- **Positioning:** "a free, source-available, local-first alternative to Grok Bot" ([launch post](https://openbot.run/news/introducing-openbot)). It mainly targets Grok Bot (xAI/Cursor's cloud agent product), OpenClaw, Hermes Agent, Claude Cowork, Manus, Devin, ChatGPT "dots", Muse, Paperclip, Viktor and Instinct, with one comparison page each ([/compare](https://openbot.run/compare), 15 pages).
- **Audience:** individuals who already pay for ChatGPT, Claude or Google AI plans and want those plans "working overnight", small teams, agencies that deploy agent setups for clients, and companies that want "AI coworkers" on managed servers ([teams](https://openbot.run/teams)).
- **Company:** Synthetify Labs sp. z o.o. (Poland). It signs the Windows builds and publishes the mobile apps. The author and main committer is Norbert Bodziony ([README license section](https://github.com/nightly-labs/openbot#license-and-attribution), [Play listing](https://play.google.com/store/apps/details?id=run.openbot.mobile)). The GitHub org `nightly-labs` (created 2022) otherwise holds web3 and wallet repos.
- **License:** PolyForm Noncommercial 1.0.0. Personal and non-commercial use is free. Commercial use needs a paid license. Versions up to 0.1.11 were Apache-2.0. External contributors sign a CLA ([README](https://github.com/nightly-labs/openbot#license-and-attribution)). The site calls it "open source", but it is source-available.
- **Revenue lines** ([pricing](https://openbot.run/pricing), [servers.md Billing](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/servers.md#billing)):
  - **Hosted server** (per server, Stripe): Starter €30/month (1 person, 25 GB), Standard €50 (up to 5 members, 50 GB on a boat sandbox or 75 GB on Leaseweb, "2× speed"), Pro €100 (up to 10 members, 100/150 GB, "4× speed"). Yearly billing takes 20% off. No model access is included.
  - **Commercial license**: Starter €10/month for 1 user. Team €25/month for 2 users, +€15 per extra user, up to 10. Yearly takes 20% off, a hosted-server bundle another 10%. The site says it is "Free until December 1, 2026". Enterprise (more than 10 members, larger servers) is contact-sales.
  - **Services**: custom deployment, integrations, onboarding and support ([teams](https://openbot.run/teams)). A dedicated Mac mini can be arranged on request ([hosted servers guide](https://openbot.run/guides/openbot-hosted-servers)).
  - Hosted sites (static site publishing) are limited per plan: Starter 3, Standard 10, Pro 50, 3 per licensed user ([servers.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/servers.md#hosted-sites-per-server)).
- **Maturity and activity** (GitHub API, 2026-11-20):
  - Repo created 2026-08-12. v0.1.0 shipped 2026-08-13. **v0.75.0 shipped 2026-11-20**: 130 changelog versions in about 100 days. The README says "A new release ships almost every day" ([CHANGELOG](https://github.com/nightly-labs/openbot/blob/main/CHANGELOG.md)).
  - 9,400 stars, 905 forks. About 8,095 issues (3,402 open) and 5,534 PRs.
  - Contributors: NorbertBodziony 2,121 commits, Sniezka1927 2,073, nrb-dev 546, then a long tail. [INFERENCE] Most of the code is written by agents; the repo ships `.agents/skills` (babysit, ship, biome-anti-slop, release-upgrade-safety, …).
  - Stars grew about 2.4k → 3.4k → 7k+ between Oct 28 and Nov 11 ([iadecider](https://iadecider.com/articles/openbot-vps-review)). The project was 3rd on Product Hunt on 2026-10-06 ([indie-machine](https://indie-machine.com/posts/openbot-multi-agent-workspace-review-confinement-gap)).
  - The README now says "ready for production use". Earlier copies said "development preview".
  - Mobile: iOS "OpenBot.run" rated 5.0 from 51 ratings, 216 MB ([App Store](https://apps.apple.com/us/app/openbot-run/id6807024266)). Android rated 4.8 from 138 reviews, 10K+ downloads, release 1.4.12 ([Play](https://play.google.com/store/apps/details?id=run.openbot.mobile)). Reviewers mostly call it a cheaper replacement for Grok Bot that runs on an always-on Mac mini or home server.
- **Stack:** Bun 1.4.2 workspace; Electron 45; SolidJS 2 renderer; Effect 4 for backend services; `node:sqlite` with an event log and projections; Expo 58 / React Native 0.88 mobile app; Cloudflare Workers + D1 + R2 account API (TanStack Start); a self-hostable "Signal" WebRTC signalling service plus coturn; i18n in 15 languages including RTL ([package.json](https://github.com/nightly-labs/openbot/blob/main/package.json), [ARCHITECTURE](https://github.com/nightly-labs/openbot/blob/main/docs/ARCHITECTURE.md)).

## 2. The concept: agent, teammate, the computer the agent uses

The vendor's own vocabulary ([wtf-is-openbot](https://openbot.run/guides/wtf-is-openbot), [openbot-101](https://openbot.run/guides/openbot-101), [glossary](https://github.com/nightly-labs/openbot/blob/main/docs/glossary.md)):

- **Model ≠ provider runtime ≠ agent.**
  - The *model* generates text.
  - The *provider runtime* is the vendor CLI (Codex App Server, Claude Code, Grok CLI, an ACP agent). OpenBot launches it as a local process and **reuses the CLI's own login**. It never copies provider credentials.
  - The *agent* is OpenBot's persistent object. It owns four things that survive restarts and provider switches: **a workspace** (`~/OpenBot/Agents/<id>` plus the shared `~/OpenBot/Shared`), **a thread** (durable record of everything it did), **an identity** (name, avatar, title, an `AGENTS.md` instruction file in its workspace), and **a private provider session** (CLI resume state, kept separate on purpose).
  - "A model is just a setting": you can move an agent from Codex to Claude and keep the folder, the history and the name ([one-agent-many-providers](https://openbot.run/news/one-agent-many-providers)).
- **"Teammates, not chats."**
  - The mental model is a Slack-like workspace where named agents (Ada, Linus, Grace…) sit next to humans.
  - Agents message each other, hand off tasks, share files and review each other's work.
  - **Channels** are rooms with one task owner at a time and explicit delegation ([channels post](https://openbot.run/news/channels-put-agents-in-one-room)).
  - Agents create agents: "describe the agent you want in one prompt", then `openbot.create_agent`.
- **The agent's computer is the host machine itself.** It is not a VM per agent.
  - All agents of a server share one OS user, one filesystem, one embedded Chromium browser (with optional per-agent or named profiles) and, optionally, the real desktop through Computer Use.
  - The workspace "organizes work; it does not limit what the agent can touch" ([every-agent-gets-a-workspace](https://openbot.run/news/every-agent-gets-a-workspace)).
  - On a headless server, agents get a **virtual Xvfb display with Openbox** (1280×800 by default), kept apart from the display that shows OpenBot's own window ([self-hosted-server.md](https://github.com/nightly-labs/openbot/blob/main/docs/self-hosted-server.md#how-it-works)).
  - A hosted server is **one machine per server** (a boat.dev sandbox or a Leaseweb VPS), shared by all agents and members of that server ([hosted-servers.md](https://github.com/nightly-labs/openbot/blob/main/docs/hosted-servers.md)).
  - The vendor contrasts this with Grok Bot, where "one cloud computer belongs to each account, and all of its Bots share it" ([compare/grok-bot](https://openbot.run/compare/grok-bot)).
- **A "server" is the unit of tenancy.** It holds its own `openbot.db`, agents, MCP servers, browser data and settings. A desktop can run several servers on one computer (headless child processes), join remote servers, and switch between them like Slack workspaces ([servers.md "Servers on this computer"](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/servers.md#servers-on-this-computer)).
- **"Local-first is a claim about storage, not about the network."** OpenBot "adds no hop of its own". Chats and files never transit the vendor's cloud: the vendor's Cloudflare account service holds only accounts, memberships, invites and the logical sessions that let devices find each other ([your-work-stays-on-your-computer](https://openbot.run/news/your-work-stays-on-your-computer)).
- **Full access is a product decision, not a security boundary.**
  - After one consent at first launch, agents run with `danger-full-access` and `approvalPolicy: never` by default ([README warning](https://github.com/nightly-labs/openbot#readme), [SECURITY.md](https://github.com/nightly-labs/openbot/blob/main/SECURITY.md)).
  - Narrower levels exist since about 0.20 / 0.43: Workspace only and Ask first (see §4.12).
- **Routines are "the reading and the noticing", not a job runner.** A routine has no retry policy and no dependency graph. The deploy stays "on the deploy system" ([routines post](https://openbot.run/news/routines-give-an-agent-a-schedule)). Workflows (since 0.50, about 2026-10-25) later added a real graph engine.

## 3. Feature inventory

Version and date in brackets = first changelog mention found by keyword search [INFERENCE: approximate].

### 3.1 Agents and runtime

| Feature | What the user can do | How it works underneath | Source |
|---|---|---|---|
| Prompt-driven agent creation | Describe an agent in one prompt, review its instructions, avatar and sections, then save, on desktop or phone. A new agent opens with a welcome question that offers 3 first tasks. | `openbot.create_agent` / `update_profile` tools run inside a normal chat. The welcome turn uses `AskUserQuestion` or `openbot.ask_user`. | [agents.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#prompt-driven-agent-profiles), [home FAQ](https://openbot.run/) |
| Agent instruction file | Edit the agent's standing rules as `AGENTS.md` (or `CLAUDE.md`) at the workspace root. Edits by the agent itself show "Updated its instructions". | Conflict-checked save: the editor sends its base text. A change is detected at turn start and end. | [agents.md#agent-instruction-file](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#agent-instruction-file) |
| 12+ provider runtimes | Codex (ChatGPT plan), Claude Code (Claude plan), Gemini (Antigravity CLI, Google AI Pro/Ultra), Grok, OpenCode (free models / Go key), Cursor, Cline, Kimi, Devin, GitHub Copilot, Mistral Vibe, Hermes Agent, Pi (bundled), Inception Mercury, Muse Code, DeepSeek, OpenRouter, any ACP agent (Goose, Qwen Code, Oh My Pi), any OpenAI-compatible endpoint, Ollama, LM Studio. | Codex App Server over stdio JSONL; Claude Agent SDK stream JSON; others over **ACP** (`cursor-agent acp`, `cline --acp`, `kimi acp`, `devin acp`, `copilot --acp`, `hermes acp`). OpenBot downloads and **pins managed copies** of each CLI (`native-runtime.lock.json`), checks hourly for upstream updates and offers "Update all". `OPENBOT_*_PATH` overrides are honoured. | [README Agent setup](https://github.com/nightly-labs/openbot#agent-setup), [providers.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md) |
| Switch provider, keep agent | Move an agent between providers mid-project. The workspace, thread and identity stay. | The provider session is kept separate from the public thread. The replacement session receives a **handoff of at most 32,000 characters** (24k recent messages, 4k older excerpts, …). | [providers.md#history-retrieval](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#history-retrieval-and-replacement-sessions) |
| Backup models / failover [0.48, 2026-10-22] | Up to 8 ordered backup models. The agent moves to the next one on a plan usage limit or a provider error, then returns when the hold ends. A note in the chat shows the switch. | `UsageLimitGate` holds every agent on that account/model until reset. `classifyFailure` classifies billing, unavailable, rate_limit, auth, network and timeout errors. It retries **only if the turn has not yet acted** (no text or tool call), and then passes over that model for 10 minutes. | [providers.md#backup-models-and-failover](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#backup-models-and-failover) |
| Provider account pool | Several accounts per provider; pick an account per agent. | `provider-account-pool.ts` / `providerAccountId`. | [App Store notes 1.4.10](https://apps.apple.com/us/app/openbot-run/id6807024266), `src/backend/agent/provider-account-pool.ts` |
| Usage display, reset credits, work on credits | Plan usage clocks in the sidebar, polled about every 5 minutes. A reset credit can lift a usage hold. | Usage read pacing respects provider endpoint limits. | [providers.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#usage-read-pacing) |
| Context monitoring and compaction | The context meter compacts automatically before the window fills. | Codex compacts at 80% of its reported window; Claude uses its native compaction. Antigravity has no compact operation, so OpenBot rotates the session with a handoff. Compaction stops after 2 ineffective tries. | [providers.md#compaction-ownership](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#compaction-ownership) |
| Idle session unloading | Many agents stay cheap on RAM. | Claude and ACP processes close when idle and resume from their sessions. A Codex thread is unsubscribed 5 minutes after its last turn. Provider process groups are reaped (SIGTERM, then SIGKILL after 2 s). | [providers.md#idle-provider-sessions](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#idle-provider-sessions) |
| FIFO message queue | Send work at any time; messages wait until the agent is free. Pause, resume, cancel, reorder, **edit a queued message**, or **steer** a running turn. | SQLite mailbox with persisted retry keys (crash-safe). Edit uses a hold so the message cannot be dispatched mid-edit. | [README What works](https://github.com/nightly-labs/openbot#what-works), [channels-and-messaging.md#mobile-chat-queue](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/channels-and-messaging.md#mobile-chat-queue) |
| Side threads / tasks / branches [0.52] | One agent runs several parallel threads ("tasks"), each with its own provider thread, queue and result. **Branch** a conversation from any finished turn. | `projection_agent_threads`. At most 4 task turns per agent by default; the host default can be raised up to 64. `DrainScheduler` drains one lane per task. Hold reasons: `task-limit`, `usage-limit`, `server-busy`, … | [agents.md#agent-threads](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#agent-threads) |
| Subagents display | See which subagents a provider spawned in a turn. | `subagents-v1` capability. | [servers.md#subagents](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/servers.md#subagents) |
| Side questions | Ask the agent something in the side panel without adding it to the chat. | Side-question transcript; deleted when the tab closes. | [CHANGELOG](https://github.com/nightly-labs/openbot/blob/main/CHANGELOG.md) |
| Plan card | Before a long task the agent shows its plan; the plan auto-continues after a countdown if nobody answers. | `acknowledge_request` with `ui-block-auto-answer-v1`. Host default in `agent-plan-card-v1`. | [servers.md#ui-blocks](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/servers.md#ui-blocks) |
| Generative UI blocks | The agent asks questions as cards: choice, form, confirm with an editable preview. | The `ask_ui` tool; the contract lives in `packages/contracts/src/ui-blocks.ts`. Secret form values are never stored. | [servers.md#ui-blocks](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/servers.md#ui-blocks) |
| Hidden agents, sidebar folders, colours, pins, chat backdrops | Organise many agents. Hidden agents keep working. | Layout stored in JSON; capabilities `sidebar-folders-v1`, etc. | [agents.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#hidden-agents) |
| Agent import (Grok Bot, Hermes) [0.21 / 0.23] | Import agents (instructions, avatar, skills, routines, memories, workspace files, group chats) from a Grok Bot `.zip` export or a local Hermes folder. | `openbot-import.json` manifest. Streaming ZIP reader with no size limit. Paths are checked against traversal. | [agents.md#agent-import](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#agent-import), [import-hermes guide](https://openbot.run/guides/import-hermes-agents) |
| Instance transfer [0.44] | Copy all agents and server settings to another server in one step, or through a `.zip`. Give a server to another account (accept-based ownership transfer, 0.75). | `openbot-transfer.json` v1 manifest. Server-to-server copy for admins of both. | [agents.md#instance-transfer](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#instance-transfer), [CHANGELOG 0.75.0](https://github.com/nightly-labs/openbot/blob/main/CHANGELOG.md) |
| Agent templates (share link) | Publish an agent as a public link with an OG share card (`openbot.run/agents/<id>`). Others install it through `openbot://`. | Snapshot without workspace or memories. The publish is refused if a field looks like a secret. Max 5 published agents per account. | [agents.md#agent-templates](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#agent-templates) |
| Image and video generation | Agents generate images (Codex imagegen) and videos (Higgsfield/Sunra apps); `read_image` / `read_video`. | `image-generation.ts`, `video-generation-v1`. | [servers.md#video-generation](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/servers.md#video-generation) |
| Commit co-author | Agents add a co-author line to their git commits. | `set_commit_co_author` tool. | tool list in `src/backend/openbot-tools.ts` |

### 3.2 Collaboration between agents and humans

| Feature | What the user can do | How it works | Source |
|---|---|---|---|
| Agent-to-agent messaging | Agents message, reply, react, send images and transfer files to each other. | `openbot.send_message`, `list_agents`. Managed transfers live in `~/OpenBot/Shared/Transfers` with a `.openbot-transfer.json` manifest (owner, recipients, SHA-256). Communication policy: no acknowledgement loops; delegate to a teammate whose profile covers the work before refusing. | [README Local data](https://github.com/nightly-labs/openbot#local-data-and-network-boundaries), [agents.md#agent-communication-policy](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#agent-communication-policy) |
| Channels [0.5, 2026-09-07] | A shared room with several agents and humans, a purpose and a lead. **One task owner at a time**, explicit delegation, and Stop / Resume / Reassign / Archive / Restore. Channel memories, channel routines and a picture. | Each channel-agent pair gets its own execution thread. Channel tools: `channel_assign`, `channel_transfer`, `channel_result`, `channel_reply`, `channel_history`, `channel_remember`, `channel_create_routine`, … The context packet is 120k characters: assignment first, then a background summary, then the newest transcript at 1/4 of the budget, with the assignment repeated at the end. | [channels-and-messaging.md#shared-channel-chats](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/channels-and-messaging.md#shared-channel-chats), [#how-a-channel-keeps-context](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/channels-and-messaging.md#how-a-channel-keeps-context) |
| Multiplayer / team server | Invite coworkers to your host. People and agents work in the same channels and agents. | The host's **Team API stays on loopback**. A hidden sandboxed Chromium page holds a WebRTC peer to each client. Team API protocol v9 uses three DataChannels (`rpc`, `events`, `files`). Signal carries SDP/ICE only; coturn relays when direct ICE fails. Capability negotiation uses frozen per-feature codecs (`*-v1`). | [team-api.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/team-api.md), [README](https://github.com/nightly-labs/openbot#commands) |
| Roles and agent access | Server roles: owner, admin, member. Custom roles set each member's level per agent: none, view, chat, manage. The **Everyone** role is the default. | `agent-access-v1`. The host hides `none` agents from lists, events and push. | [servers.md#roles](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/servers.md#roles) |
| Audit log | Owners and admins read access changes: joins, removals, role changes, per-agent levels. | `server-audit-v1`. | [servers.md#audit-log](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/servers.md#audit-log) |
| Company sign-in (SSO) [0.60] | Connect a server to an OIDC provider (Entra ID, Okta, Google Workspace). Domain claim; SCIM token. SAML is not supported. | Runs in the account Worker; the host sees only OpenBot sessions. | [servers.md#company-sign-in](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/servers.md#company-sign-in) |
| Shared tables | Agents create SQLite tables in one shared DB (`~/OpenBot/Shared/Data/agent-data.db`). The owner agent alters them; others read and write rows. | `query_data`, `execute_data`, `list_tables`, `delete_table` tools. | [glossary](https://github.com/nightly-labs/openbot/blob/main/docs/glossary.md) |
| Library folders | A cross-chat file library view. | `list_library_folders`, `organize_library`. | tool list, [CHANGELOG 0.65](https://github.com/nightly-labs/openbot/blob/main/CHANGELOG.md) |
| CRM (experimental) [0.72] | Leads, customers, plans and payments that agents read and update. Payment-source sync. | Migration 63 `crm-*` tables; tools `crm`, `crm_customer`, `crm_find`, `crm_payments`, `crm_propose`. Off by default. | [agents.md#crm](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#crm) |
| Hosted sites | An agent publishes a static site or HTML preview to a public URL. | `publish_site`, `replace_site`; served by the account Worker; per-plan limits. | [servers.md#hosted-sites-per-server](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/servers.md#hosted-sites-per-server) |

### 3.3 Automation: routines, flows, workflows, triggers

| Feature | What the user can do | How it works | Source |
|---|---|---|---|
| Routines (schedule) [0.2] | One instruction plus a schedule per agent or channel; one-time routines too. Test run, run history, missed-run policy. Runs land in the agent's chat or in a dedicated **routine thread**. | `RoutineScheduler`. Each run is an **ephemeral new provider session** with an 8,000-character handoff from earlier runs and the chat. A non-quiet run adds a ≤1,500-character digest to the chat session. | [agents.md#routine-sessions](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#routine-sessions) |
| Quiet runs | "Nothing to report" is a first-class outcome: no notification, no clutter. | `routine_no_update` tool, or a `[[no-update]]` / `[SILENT]` marker (Hermes-compatible). | [agents.md#quiet-routine-runs](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#quiet-routine-runs) |
| Failure reporting | An agent marks a run failed with a reason, even though the turn completed. | `routine_report_failure`. | [agents.md#failed-routine-runs](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#failed-routine-runs) |
| Self-pausing routines | A run pauses its own routine when its task says to stop; the UI shows "Paused by its 8:00 AM run". | `routine.paused-by-run` event. | [agents.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#routine-sessions) |
| Webhook routines [0.33] | Each routine gets a URL and a secret. HMAC-SHA256 over `timestamp.deliveryId.body`. Event-type and JSON-Pointer filters (up to 128). Dedup by delivery ID for 7 days. **Standard Webhooks** format accepted (Svix, Clerk, Resend, Composio). Up to 64 per host. | A public route on Signal relays to the host; the relay never holds the secret. Event data is marked untrusted and "cannot give tools to the agent or skip approvals". A hosted server that sleeps answers 503 to the first delivery and wakes. | [docs/webhooks.md](https://github.com/nightly-labs/openbot/blob/main/docs/webhooks.md), [events.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/events.md) |
| Routine flows [0.39] | Chain routines: the answer of agent A's routine goes on to agents B and C. Fan-in waits for all inputs. | `RoutineFlows` Effect service with idempotent sweeps. Agents edit flows with `connect_routine_agents`. | [agents.md#routine-flows](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#routine-flows) |
| **Workflows canvas** [0.50, 0.58] | Draw a graph on a canvas. Nodes: trigger, agent, LLM, condition (yes/no), approval, image/video generation (Higgsfield, Sunra), notification and messaging outputs, **GitHub actions** (10 kinds). **Loops** are allowed. Versions (50 kept), retry, "Run from this step", per-step settings replay. Agents build workflows themselves, and the canvas replays their edits with a cursor. | `src/backend/workflows/`, migration 46. An agent step is a routine handoff. Model and condition steps run in a warm, tool-less process. Triggers: schedule, webhook, GitHub App events, Discord / Telegram / Slack mentions. Only a person's message can make an agent edit, run or delete a workflow. Delete always asks. | [agents.md#workflows](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#workflows), [events.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/events.md) |
| Local scripts API + `openbot-cli` [0.29] | Scripts on the host send messages, run routines with a payload, wait for answers, answer pending questions and approvals, switch model or account, clear context. Example: `openbot-cli run "Ada" "Run the tests" --wait`. | Loopback HTTP with a per-start bearer token in `<userData>/automation/` (0700/0600). Requests with an `Origin` header are refused. Per-agent opt-in. 30 runs/hour by default. `await_event` lets an agent park a task until a script wakes it. | [docs/automation.md](https://github.com/nightly-labs/openbot/blob/main/docs/automation.md), [agents.md#local-scripts-api](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#local-scripts-api) |
| Routine calendar feed | Subscribe to the next 30 days of routine runs as an iCalendar feed. | Loopback `.ics` server with the token in the path. | [agents.md#routine-calendar-feed](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#routine-calendar-feed) |

### 3.4 Tools, browser, computer

| Feature | What the user can do | How it works | Source |
|---|---|---|---|
| OpenBot tool catalog | Agents manage the platform itself: agents, routines, channels, workflows, memories, skills, MCP servers, API keys, sites, CRM, data, sections, settings. | About 60 `openbot.*` tools plus about 35 `openbot_browser.*` tools. Only **11 direct tools** are always visible; the rest are found with `tool_search` / `tool_describe` / `tool_call` to keep the prompt small. `bun run check:agent-context` fails the build if the prompt grows. | [providers.md#built-in-tool-discovery](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#built-in-tool-discovery), `src/backend/openbot-tools.ts` |
| Embedded browser | A persistent built-in Chromium the agent opens, inspects and controls. Tabs outlive turns. The user sees and can take over tabs. | Sandboxed `WebContentsView` driven over CDP. Tools: navigate, snapshot (ref-based), click, type, fill_fields, run_script (multi-step JS in a `node:vm`), handle_dialog, upload_files, recording, … Browser profiles: Shared, per-agent, or named. | [browser.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/browser.md) |
| Browser takeover / human-in-the-loop sign-in | When a site needs a login, 2FA or a captcha, the agent asks the user to take over the tab, from the desktop or the phone. "Sign in on this computer" sends that site's cookies to the remote host. | `request_takeover`, routed through the attention registry. The call waits minutes for the user. A routine run reports instead of waiting. | [agents.md#agent-communication-policy](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#agent-communication-policy), [PRIVACY.md](https://github.com/nightly-labs/openbot/blob/main/PRIVACY.md) |
| **Vault** [0.49] | Store logins, payment cards and contact info encrypted by the OS. Agents fill passwords and cards but see only the card name and last 4 digits. The first card fill on each site needs approval. 1Password, Bitwarden and Proton Pass connectors. | `submit_secret`, `fill_card`, `request_vault_item`, … Secrets are registered for redaction. | [compare/grok-bot](https://openbot.run/compare/grok-bot), [browser.md#secure-browser-authentication](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/browser.md#secure-browser-authentication) |
| Computer Use | Agents drive desktop apps (screen capture, accessibility tree, mouse, keyboard) on macOS, Windows and Linux. Per-agent toggle. | Bundled MIT `cua-driver`. One long-lived `serve` daemon is owned by OpenBot; each provider CLI spawns a short MCP proxy to it. macOS needs Screen Recording and Accessibility. On servers it uses the agent's Xvfb display. | [computer-use.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/computer-use.md) |
| Remote desktop | Watch and control the host's or agent's screen from another computer or the phone. Roles decide who may start a session; optional per-session consent at the machine. | Sunshine streams; the Moonlight Web viewer is embedded. X11 only on Linux. Permission checks and live tests. | [computer-use.md#host-controls](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/computer-use.md#host-controls) |
| Screen capture | Agents screenshot a screen or window. | `list_capture_sources`, `capture_screen`. | tool list |
| Web search fallback | Providers without their own search get `openbot.web_search`. | A side Codex thread runs search only, on the host's Codex sign-in; benchmarked against inline search. | [providers.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#built-in-tool-discovery) |
| MCP servers | Add MCP servers (OAuth sign-in, multiple accounts), shared by every agent of the host. Tool consent at Ask first. Members may manage them if allowed. | Host-global MCP registry, an MCP gateway, a callback relay for OAuth on remote hosts. | [integrations.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/integrations.md) |
| Connectors | Built-in sign-ins turned into tools (the GitHub App with Device Flow, password managers). | `GitHubConnectorService`, `scripts/create-github-app.ts`. | [glossary](https://github.com/nightly-labs/openbot/blob/main/docs/glossary.md) |
| API keys store | An agent asks the user for an API key through a secure card and keeps it in the host's encrypted store. | `request_api_key`, `save_api_key`, `list_api_keys`. | tool list |

### 3.5 Memory, skills, marketplace

| Feature | What the user can do | How it works | Source |
|---|---|---|---|
| Agent memories | Agents `remember` and `forget_memory`. The user marks a memory **essential** (always in the prompt, 8 KB budget) or **searchable** (`search_memories`). Channel and messaging workspaces have their own memory lists. | Stable instruction order: rules, then profile, then memories last. | [team-api.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/team-api.md), [messaging.md#workspace-memories](https://github.com/nightly-labs/openbot/blob/main/docs/messaging.md#workspace-memories) |
| Background memory review [0.53] | Every 10/20/40 turns (or Off), a background pass updates memories from the conversation. | A tool-less generation on the cheapest model of the same provider. JSON add/update/forget ops pass through the memory checks. It never overrides memories the user wrote. Text is redacted first. | [providers.md#background-memory-review](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#background-memory-review) |
| Skill learning | The review proposes "save this as a skill" (or does it silently, or never). The agent creates and revises skills itself. | Rules borrowed from Hermes Agent (MIT). `create_skill`, `revise_skill`, `install_local_skill`. Immutable revisions in a local library. | [providers.md#skill-suggestions](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#skill-suggestions), [skills-and-plugins.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/skills-and-plugins.md) |
| Skills format | Agent Skills spec (`SKILL.md`) written to each provider's folder (`.agents/skills`, `.claude/skills`). Packs; `example-prompt` frontmatter. | The installer writes into both provider directories. | [skills-and-plugins.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/skills-and-plugins.md#skill-folders-and-mcp-configuration) |
| Marketplace | Tabs Agents (27 curated), Skills (41), Apps (**254** plugin pages). Install into a chosen agent. Submit your own agent for review. | A plugin is an MCP server plus pinned skills plus a listing. The catalog is a static file set served by the Worker, cached locally, and opened by `openbot://` deep links. | [marketplace guide](https://openbot.run/guides/openbot-marketplace), [plugin-distribution.md](https://github.com/nightly-labs/openbot/blob/main/docs/plugin-distribution.md), [sitemap](https://openbot.run/sitemap.xml) |
| External memory plugins | Honcho, Hindsight, Supermemory, Mem as apps. | MCP plugins. | [App Store 1.4.10 notes](https://apps.apple.com/us/app/openbot-run/id6807024266) |

### 3.6 Surfaces and reach

| Feature | What the user can do | How it works | Source |
|---|---|---|---|
| Desktop app | macOS 13+ (arm64/x64, signed and notarized), Windows 10+ x64 (signed), Linux AppImage x64/arm64, Nix flake. Auto-update from GitHub Releases. 15 languages. Custom theme palette files. | Electron; renderer and preload isolated; typed IPC (`window.openbot`). | [README Install](https://github.com/nightly-labs/openbot#install) |
| Web client `openbot.run/app` [0.19] | Full workspace in a browser, connected to your host. | The same `WorkspaceFrame` as desktop; WebRTC to the host. No offline cache. | [browser.md#browser-client](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/browser.md#browser-client), [PRIVACY.md](https://github.com/nightly-labs/openbot/blob/main/PRIVACY.md) |
| iOS / Android apps [Android 0.30] | Chat, create and edit agents, channels, queue edit, approvals, browser takeover, remote desktop, voice, video attachments. Face ID / PIN lock. **Live Activity / Dynamic Island** shows agent state. Push notifications. | Expo app. WebRTC inside a hidden Expo DOM WebView. Pairing by QR "Mobile Connect". iOS push goes through APNs via a Worker relay with **sealed payloads** (the Worker stores and logs nothing). Android push goes through FCM. Notifications carry no action buttons. | [channels-and-messaging.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/channels-and-messaging.md#iphone-live-activity-updates) |
| Slack / Discord / Telegram [0.28-0.34] | Mention @OpenBot; an auto-added **Orchestrator agent** answers small things itself and routes other work to the best agent. A thread maps to one agent conversation. Agents can also **post** to any connected place on their own (`messaging_post`). | One shared vendor Slack/Discord app and Telegram bot. **Signal receives the platform webhooks** and forwards them to the host over an outbound WebSocket, with an ES256 route ticket. Bot tokens are sealed to the host key (Slack) or held by Signal (Telegram). A messaging connection is a `MessagingDriver` plugin. | [docs/messaging.md](https://github.com/nightly-labs/openbot/blob/main/docs/messaging.md), [channels-and-messaging.md#messaging-connections](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/channels-and-messaging.md#messaging-connections) |
| WhatsApp [0.11] | Link a number as a WhatsApp Web device. Personal-number mode (self-chat plus an allowlist) or dedicated-number mode (approval codes). Replying "1"/"2" answers approvals. | Unofficial client running directly on the host; no vendor service involved. | [docs/messaging.md#connect-whatsapp](https://github.com/nightly-labs/openbot/blob/main/docs/messaging.md#connect-whatsapp) |
| Voice | Dictation (local Whisper large-v3 turbo, Groq Whisper), spoken replies (local voices in 31 languages, ElevenLabs, xAI), **live voice** via Codex/OpenAI realtime over WebRTC. Push-to-talk (0.75). | Live voice audio flows client <-> OpenAI directly, never through the host. | [servers.md#live-voice](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/servers.md#live-voice), [compare/grok-bot](https://openbot.run/compare/grok-bot), [CHANGELOG 0.75.0](https://github.com/nightly-labs/openbot/blob/main/CHANGELOG.md) |
| Notifications | Desktop, web and phone notify on a finished turn, an error, or **needs-you** (question, approval, browser step). Levels per server and per agent. "Needs you" inbox. | `notificationForAgentEvent` is shared by all clients. | [channels-and-messaging.md#iphone-notifications](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/channels-and-messaging.md#iphone-notifications) |
| Attachments | Up to 100 MB per file, 250 MB per message, 10 files. Any type (MP3/MOV/MP4/ZIP/SVG/…). The agent gets the original file. | Capability-gated per host version. | [README Chat attachments](https://github.com/nightly-labs/openbot#chat-attachments) |
| Keep awake / Private network | Stop sleep while on power; restrict connections to the LAN or Tailscale only. | ICE filter keeps private host candidates only. | [compare/grok-bot](https://openbot.run/compare/grok-bot), [team-api.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/team-api.md) |

### 3.7 Deployment and operations

| Feature | What the user can do | How it works | Source |
|---|---|---|---|
| Headless Linux server ("server mode") [0.30] | `curl … install-server.sh \| sudo bash; sudo openbot login`, or a **one-time 15-minute claim** generated by the app (`OPENBOT_CLAIM` piped on stdin so sudo and ps never see it). An agent can install a server over SSH (`create_server_install_command`). | The AppImage runs under systemd as the `openbot` user, on Xvfb plus Openbox with `OPENBOT_SERVER=1`. `openbot status/logs/restart/update/name/logout` talk to a 0600 Unix control socket. Staged updates, optional automatic updates when idle. | [docs/self-hosted-server.md](https://github.com/nightly-labs/openbot/blob/main/docs/self-hosted-server.md), [VPS guide](https://openbot.run/guides/install-openbot-vps) |
| Docker / ECS / Umbrel | `ghcr.io/nightly-labs/openbot` for amd64 and arm64, with a seccomp profile that keeps the Electron sandbox on. Compose file. Several instances per host. | [docs/docker.md](https://github.com/nightly-labs/openbot/blob/main/docs/docker.md), [aws-ecs.md](https://github.com/nightly-labs/openbot/blob/main/docs/aws-ecs.md), [umbrel.md](https://github.com/nightly-labs/openbot/blob/main/docs/umbrel.md) | |
| Memory-aware scheduling | Turn concurrency follows RAM: 2 turns up to 5 GiB, 8 up to 10 GiB, 16 above (1-128 or unlimited configurable). The levels ok / low / critical hold new turns, give priority to a person's message, unload idle pages and stop leftover processes. | `ServerMemory` reads cgroup v2 every 5 s and reserves 300 MB per newly started turn. The unit has `MemoryMax=90%`. Agent processes get `oom_score_adj` 500 and nice 10; main gets -500. | [hosted-servers.md#memory](https://github.com/nightly-labs/openbot/blob/main/docs/hosted-servers.md#memory), [self-hosted-server.md#memory](https://github.com/nightly-labs/openbot/blob/main/docs/self-hosted-server.md#memory) |
| Hosted servers [0.25] | Buy a server in the app; it is provisioned and opens provider setup. Plan end → 14-day encrypted archive → restore on renewal. | Account Worker plus a boat.dev sandbox (sleeps after 15 minutes idle; woken by clients, by a cron before the next routine, and by Slack/Discord/Telegram events) or an always-on Leaseweb VPS pool. Stripe billing per server. | [hosted-servers.md](https://github.com/nightly-labs/openbot/blob/main/docs/hosted-servers.md), [hosted guide](https://openbot.run/guides/openbot-hosted-servers) |
| macOS multi-tenant host | One Mac serves several tenants, each a standard macOS user with its own OpenBot (signed Host PKG, `openbot-host setup/verify/status`). | [multi-tenant-hosting.md](https://github.com/nightly-labs/openbot/blob/main/docs/multi-tenant-hosting.md) | |
| Fully self-hosted remote access | Replace openbot.run with your own Cloudflare Worker (D1, R2), Signal, coturn and SMTP. The web client, Slack, Telegram and the GitHub App then stop working. | [docs/self-hosting.md](https://github.com/nightly-labs/openbot/blob/main/docs/self-hosting.md), [iadecider](https://iadecider.com/articles/openbot-vps-review) | |
| Status page and status webhooks | `status.openbot.run` on GitHub Pages; status webhooks to the team. | [docs/status-webhooks.md](https://github.com/nightly-labs/openbot/blob/main/docs/status-webhooks.md) | |

## 4. System architecture

### 4.1 Process model ([README Architecture](https://github.com/nightly-labs/openbot#architecture))
```
Electron main
├── provider CLIs as child processes (Codex App Server stdio JSONL, Claude Agent SDK, ACP agents, Pi …)
├── cua-driver daemon (Computer Use) + Sunshine (remote desktop)
├── SQLite command/event log + read projections (openbot.db, WAL)
├── Team API (loopback HTTP) ⇄ hidden sandboxed Chromium page ⇄ WebRTC DataChannels ⇄ clients
├── loopback servers: automation (local scripts), routine .ics feed, control socket (server mode)
└── sandboxed WebContentsView browser host (CDP)
SolidJS renderer (desktop) / same UI at openbot.run/app / Expo mobile
Cloudflare Workers: account API (TanStack Start; D1 accounts, hosts, memberships, invitations, routes, billing; R2 avatars/sites)
Signal (self-hostable): WebRTC signalling + Slack/Discord/Telegram/webhook ingress relay; coturn TURN
```

### 4.2 Runtime and the agent's computer
- No VM or container per agent. Agents share the host OS user. Each agent has its own workspace folder plus a shared folder ([README](https://github.com/nightly-labs/openbot#local-data-and-network-boundaries)).
- Containment levels per agent ([ipc-agent-admin.ts](https://github.com/nightly-labs/openbot/blob/main/packages/contracts/src/ipc-agent-admin.ts)):
  - **Workspace only**: writes are confined to the workspace, the shared folder and temp folders.
    - Codex and Claude use their own sandboxes.
    - Other CLIs get an **Apple Seatbelt profile on macOS**.
    - On Linux and Windows, OpenBot fails closed for those other CLIs.
    - On Ubuntu, the Claude sandbox needs a bwrap AppArmor rule that the server installer adds.
    - Sources: [process-confinement.ts](https://github.com/nightly-labs/openbot/blob/main/src/backend/process-confinement.ts) (`canConfineProcesses` returns true only on darwin), [self-hosted-server.md](https://github.com/nightly-labs/openbot/blob/main/docs/self-hosted-server.md#how-it-works).
  - **Ask first**: tool consent cards for MCP and provider tools; only offered for providers that support approvals in full access.
  - **Full access** (default): no confirmations.
- On servers, an agent display (Xvfb + Openbox) is separate from the app's display. Agent processes get a lower OOM priority and nice value.

### 4.3 Model providers
- CLI-first: OpenBot is a **wrapper of agent CLIs**. It never calls model APIs itself for agent turns, except for tool-less helper generations through the profile client (memory review, workflow LLM and condition steps, web search). Those still go through the provider CLI.
- A provider driver registry lives in `src/backend/provider-drivers.ts`. Each new CLI means a driver plus a catalog rule ([providers.md#model-catalog](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#model-catalog)).
- OpenBot tunes each vendor's prompt to cut tokens: it disables Codex plugin suggestions (about 1,050 tokens saved per request), host-only Claude Code tools, Pi `<docs>`, and so on. Each tweak is measured by bench scripts ([providers.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#built-in-tool-discovery)).

### 4.4 Memory and context
- Essential memories (8 KB, always in the prompt) and searchable memories. Channel, messaging-workspace and agent memory scopes.
- Background memory review plus skill learning (§3.5).
- Handoffs: 32k characters on a provider switch, 8k for routine runs, a 120k-character channel packet.
- Compaction is owned per provider.

### 4.5 Tools and MCP
- OpenBot exposes its own tools to every provider as an MCP server: an in-process SDK MCP server for Claude, a local MCP bridge for ACP, Pi and Muse.
- A discovery layer (`tool_search` / `describe` / `call`) keeps about 95 tools out of the base prompt.
- User MCP servers are host-global with OAuth (a callback relay for remote hosts). Plugins add MCP plus skills.

### 4.6 Browser and computer use
- Embedded Chromium over CDP: ref-based snapshots, `run_script` batches, dialogs, downloads, profiles, takeover, Vault.
- Desktop control through `cua-driver` over MCP.
- Remote viewing through Sunshine/Moonlight.

### 4.7 Scheduling and triggers
- Triggers: schedule, webhook (HMAC or Standard Webhooks), local script, iCal (output only), routine flows, workflow triggers (schedule, webhook, GitHub App, Discord, Telegram, Slack), messaging mentions.
- A hosted server that sleeps is woken by a Worker cron before the next routine and by Signal on inbound events.

### 4.8 Multi-agent
- Patterns: direct messages, channels with a single task owner plus delegation, an Orchestrator agent per messaging connection, side threads, subagents spawned by the provider CLI, routine flows, workflow graphs.
- The agent communication policy is enforced in the developer instructions: no acknowledgement loops, delegate before refusing.

### 4.9 Persistence
- `openbot.db` (Electron userData) is an **event log plus projections**: agents, threads, turns, provider sessions, mailbox, channels, routines, workflows, CRM. About 63+ numbered migrations.
- An upgrade-safety skill gates SQLite, IPC and Team API changes ([.agents/skills/release-upgrade-safety](https://github.com/nightly-labs/openbot/tree/main/.agents/skills/release-upgrade-safety)).
- Files live in `~/OpenBot/…`. Provider state stays in each CLI's own home (`~/.codex`, `~/.claude`, …).

### 4.10 Remote access and identity
- Account: email one-time codes; codes and sessions are stored hashed.
- The host registers with a machine token.
- Client to host: WebRTC, encrypted, peer-to-peer when possible, TURN as fallback.
- Optional OIDC company sign-in plus SCIM.
- Protocol versioning: frozen `*-v1` capability codecs; the host advertises its capabilities; old clients degrade gracefully ([team-api.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/team-api.md)).

### 4.11 Observability
- Per-turn activity: files read, commands run.
- Run history for routines and workflows.
- Per-agent usage records with tokens per agent. Plan usage clocks.
- `openbot status` / `logs`; diagnostics export with redaction; status page.
- Audit log for access changes.
- Product analytics with a self-hosted OpenPanel; development builds send none ([PRIVACY.md](https://github.com/nightly-labs/openbot/blob/main/PRIVACY.md)).
- Dev tooling: `dev:status`, `dev:verify`, `dev:automation` (CDP-driven app QA), `dev:cpu`, `dev:memory`, `dev:bench`.

### 4.12 Security model (as stated)
- **Not a sandbox by default.** Full access is the documented default ([SECURITY.md](https://github.com/nightly-labs/openbot/blob/main/SECURITY.md)).
- Boundaries that are meant to hold: renderer and remote pages cannot reach Node or IPC; IPC is accepted only from the trusted renderer; attachment IDs cannot escape their roots; provider credentials stay in the CLIs; diagnostics are redacted; browser tabs are scoped to their agent.
- **Redaction at every exit** (logs, diagnostics, crash reports, analytics) ([news](https://openbot.run/news/what-gets-redacted-before-it-leaves)).
- Untrusted-content marking for webhook data, messaging facts and script payloads.
- Privileged tool gates:
  - Only a person's message may edit, run or delete workflows.
  - `delete_workflow` always asks.
  - Server install commands need owner approval.
  - The Local scripts flag can be changed only by the user, never by an agent.
- Secrets: the OS keychain/secret storage; the Vault exposes only the last 4 digits of a card; secret form values are never stored.

### 4.13 Pricing limits
- Per plan: members (1/5/10), storage (25/50-75/100-150 GB), relative speed, hosted sites (3/10/50).
- Per account: max 50 servers, max 5 published agent templates.
- Per host: 64 webhook routines; local scripts 30 runs/hour per agent by default; agent task limit 4 (max 64).
- Messaging: per-agent Slack queue max 20 waiting requests, 2 per person ([messaging.md#limits](https://github.com/nightly-labs/openbot/blob/main/docs/messaging.md#limits)).

## 5. UX surfaces
- **Desktop** (macOS, Windows, Linux): sidebar of agents, channels, folders and pins. Chat with a side panel (files, side threads, panes). Workflows canvas. Marketplace. Server settings. "Needs you". Dynamic-island style status. Backdrops and themes.
- **Web**: `openbot.run/app`, the same UI.
- **Mobile**: iOS (Live Activities, Face ID) and Android (FCM). Remote desktop viewer, voice conversation, queue sheet, channels.
- **Chat apps**: Slack, Discord, Telegram, WhatsApp. Agents can also post proactively.
- **Voice**: dictation, read-aloud, live voice, push-to-talk.
- **CLI / HTTP**: `openbot-cli`, the local scripts API, the `openbot` server command.
- **Calendar**: iCal routine feed.
- **Public pages**: agent template cards, marketplace pages, hosted sites.

## 6. What they do notably better or differently (evidence)
1. **Bring-your-own-subscription across about 20 agent CLIs, with failover.** It reuses consumer plans (ChatGPT, Claude, Google AI Pro) through the vendors' own CLIs, pins and updates those CLIs, and fails over between providers while keeping the agent ([providers.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#backup-models-and-failover)). Most competitors pick one model family.
2. **The agent is decoupled from the runtime.** Workspace, thread and identity survive a provider switch through a bounded handoff ([news](https://openbot.run/news/one-agent-many-providers)).
3. **Zero inbound ports and no tunnel setup.** The phone and web reach a home server or VPS through WebRTC with vendor signalling and TURN, with a "Private network" mode for LAN or Tailscale only ([team-api.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/team-api.md), [iadecider](https://iadecider.com/articles/openbot-vps-review)).
4. **Shipping velocity.** 130 versions in about 100 days, a public changelog for every version, and a mobile OTA pipeline ([CHANGELOG](https://github.com/nightly-labs/openbot/blob/main/CHANGELOG.md)).
5. **Token-budget engineering.** Deferred tool discovery, per-provider prompt trimming measured by bench scripts, and a CI context budget check ([providers.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#built-in-tool-discovery)).
6. **Human-in-the-loop primitives that work everywhere.** Questions, approvals, browser takeovers and secret forms all go through one attention registry and reach desktop, web, phone (Live Activity), Slack and WhatsApp ("Reply 1 to approve") ([channels-and-messaging.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/channels-and-messaging.md)).
7. **Agent-operable platform.** Agents create agents, routines, workflows, skills, channels, sites, MCP servers and servers (install over SSH). The workflow canvas replays an agent's edits live ([agents.md#workflows](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#workflows)).
8. **Self-improvement loop.** Background memory review plus skill suggestions, modelled on Hermes ([providers.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/providers.md#background-memory-review)).
9. **Quiet-by-design routines.** `routine_no_update` / `routine_report_failure` make "nothing to report" and "failed" first-class, which keeps scheduled work from flooding notifications ([agents.md](https://github.com/nightly-labs/openbot/blob/main/docs/architecture/agents.md#quiet-routine-runs)).
10. **Migration funnels from competitors.** One-click import from Grok Bot and Hermes, 15 comparison pages, and a dedicated "export agent" for Grok Bot ([guides](https://openbot.run/guides/import-grok-bot-agents)).
11. **Ops-grade server mode.** A claim-based install that an agent can run, memory-tiered concurrency, an OOM policy, staged updates and a control socket threat model ([self-hosted-server.md](https://github.com/nightly-labs/openbot/blob/main/docs/self-hosted-server.md)).
12. **Payments and logins.** A Vault with card filling that exposes only the last 4 digits, plus password-manager connectors ([compare/grok-bot](https://openbot.run/compare/grok-bot)).

**Weaknesses seen**
- Full access by default.
- Workspace only is effectively macOS-only for non-Codex/Claude CLIs ([indie-machine review](https://indie-machine.com/posts/openbot-multi-agent-workspace-review-confinement-gap); still true in current source).
- No per-agent isolation: agents can read each other's files.
- Non-commercial license.
- Daily releases mean upgrade risk for unattended servers ([iadecider](https://iadecider.com/articles/openbot-vps-review)).
- Heavy dependence on vendor services (Signal, Worker) unless you self-host the whole stack.
- An unofficial WhatsApp client, with a risk of the number being banned.
- About 3,400 open issues.

## 7. What Atlas Crew would need to do the same things (parity checklist derived from OpenBot)
This is derived from OpenBot's features only. It makes no claim about what Atlas already has (MapAtlasCrew covers that).

**Core / runtime**
1. Agent object independent of the runtime: workspace, durable thread, identity and instruction file, with provider session state kept separate.
2. Bounded history handoff on runtime switch; provider-owned compaction.
3. Multi-provider CLI drivers: Codex App Server, Claude Agent SDK, ACP generic. Managed CLI download, pinning and update. Reuse the CLI's own login.
4. Backup model chain with usage-limit and provider-error classification. Retry only when the turn has not acted. Multi-account pooling per provider.
5. Crash-safe FIFO mailbox per agent with pause, resume, cancel, edit (hold), steer, idempotency keys, and hold reasons.
6. Parallel side threads per agent with per-agent and host-wide turn limits. Branch from a turn.
7. Memory-aware admission control on the server: cgroup reads, a turn reservation, priority for a person's message, OOM scoring, idle session unloading.
8. Event-sourced persistence with versioned migrations and upgrade-safety gates.
9. Optional OS confinement levels (workspace only, ask first, full) that actually work on Linux. OpenBot only fails closed there.

**Collaboration**
10. Agent-to-agent messages and managed file transfers. Channels with lead and single-owner task delegation, plus channel memory and a bounded context packet.
11. Server roles plus per-agent access levels (none/view/chat/manage), audit log, OIDC SSO plus SCIM.
12. An attention registry: questions, approvals, confirm/form UI blocks, secret inputs, browser takeover, routed to every client and push channel.

**Automation**
13. Routines (schedule, one-time, webhook with HMAC and Standard Webhooks, dedup) running in their own ephemeral session with a digest, quiet/failure tools, self-pause.
14. A local scripts API and CLI (loopback, token file) with `await_event`-style waits.
15. A workflow graph engine: agent, LLM, condition and approval nodes; loops; versions; run-from-step; triggers from GitHub, chat and schedule; GitHub/messaging outputs. Agents can edit it, gated to a person's request.

**Tools**
16. A platform tool catalog exposed over MCP, with deferred discovery to protect context.
17. An embedded persistent browser per host with profiles, ref snapshots, a multi-step script tool, takeover, and a Vault (passwords and cards, last-4-only).
18. Computer Use through an MCP driver plus a virtual agent display (Xvfb + WM) on headless servers. Remote desktop viewing with role and consent policy.
19. Host-global MCP servers with an OAuth callback relay for remote hosts. Connectors (GitHub App). A plugin = MCP + skills marketplace.
20. Memories (essential/searchable), background memory review and skill learning, a local skill library with revisions.

**Reach and ops**
21. Desktop, web, iOS and Android clients over a single versioned capability protocol. Live Activity/Dynamic Island. Sealed push relays.
22. Slack, Discord, Telegram, WhatsApp drivers with an orchestrator agent, per-connection memories and proactive posting.
23. Voice: local STT/TTS and realtime voice.
24. No-inbound-port remote access (WebRTC + TURN) or an equivalent, with a LAN/Tailscale-only mode.
25. One-command server install with a one-time claim (agent-runnable), a control socket CLI (`status/logs/update`), staged updates, Docker image with seccomp, instance transfer and ownership transfer.
26. Hosted servers with sleep/wake on activity, routine and webhook (if offered commercially).

**Docs**
27. A glossary that fixes product terms.
28. Per-topic architecture notes with a "Not confirmed" section in each doc.
29. A public guide per surface: VPS install, import, marketplace, webhooks, local scripts, messaging, Docker, self-hosting.
30. Comparison pages and a privacy document that lists every outbound connection.

## 8. Unknowns
- **Hosted regions conflict.** The hosted-servers guide says Leaseweb Amsterdam and near Washington, D.C. ([guide](https://openbot.run/guides/openbot-hosted-servers)). The Grok Bot comparison says "EU (Germany, Finland or France)" ([compare](https://openbot.run/compare/grok-bot)). Not resolved.
- **Plan specs conflict.** The pricing page shows Standard "50 GB"; the hosted guide and docs say 75 GB on Leaseweb versus 50 GB on boat. "2×/4× speed" is not defined publicly. [INFERENCE] It likely means vCPU/RAM tiers.
- **Real usage.** No public numbers on paying customers, hosted-server counts or MAU. 10K+ Android downloads is the only hard number.
- **First-appearance versions in §3** come from keyword matches in the changelog. They are approximate.
- **Not run.** The app was not run locally; this research did not install or execute it. Behaviour is taken from docs and source text, not observed. Several docs carry their own "Not confirmed" sections (for example, Telegram end-to-end is only tested against fakes: [messaging.md](https://github.com/nightly-labs/openbot/blob/main/docs/messaging.md#test-telegram-locally)).
- **No demo videos.** I found no official product videos. The site uses an interactive in-page preview. An X account `@OpenBot_` is linked from third-party pages and was not read (it needs login and JS).
- **Third-party claims are dated.** Reviews were written against older versions (v0.30, v0.53). Claims checked against current source: Linux/Windows confinement still fails closed for non-Codex/Claude CLIs. Default concurrency up to 5 GiB is now 2 turns, not the 4 the reviews report ([self-hosted-server.md](https://github.com/nightly-labs/openbot/blob/main/docs/self-hosted-server.md#memory)).
- **License timing.** The commercial license is "free until December 1, 2026". What happens to existing commercial users after that is not stated.
