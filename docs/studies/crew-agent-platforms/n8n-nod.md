# Study: n8n "Nod" (nod.n8n.io) and the n8n agent stack under it

Research date: 2026-11-20. Research only; no code, issues or PRs were made.
Main sources: the Nod site and full docs (`https://nod.n8n.io/docs/llms-full.txt`, 13,268 lines, all 78 pages in the sitemap read or searched), the `n8n-io/nod` repo, the n8n launch post, and the n8n docs in Markdown (`https://docs.n8n.io/llms.txt` plus about 40 pages).
Doc pages are cited by URL.

---

## 0. Short answer: what "nod" is

- **Nod is a personal agent from n8n Labs**, released as a "research preview" on **2026-11-10**. It is a standalone, open-source app (Apache-2.0), not part of n8n: "Nod is a standalone app, separate from n8n … you don't need an n8n account to use it" ([blog](https://blog.n8n.io/meet-nod/)); "Do I need n8n? No … It is not the n8n AI Assistant" ([landing](https://nod.n8n.io/)).
- **Tagline**: "Open source personal agent with its own computer. It chases invoices, preps your calls and cancels trials. On your machine, with your approval." ([landing](https://nod.n8n.io/))
- **Where it fits for n8n**: n8n is "multiplayer" (shared team processes on a shared instance). Nod is "for a single player": one person, their accounts, their errands ([blog](https://blog.n8n.io/meet-nod/)).
- **How it relates to n8n**:
  1. Nod can connect to an n8n instance through n8n's **instance-level MCP server**, then build, test, fix and publish workflows and n8n Agents there ([use/n8n](https://nod.n8n.io/docs/use/n8n/), [connect/mcp-api-cli#n8n](https://nod.n8n.io/docs/connect/mcp-api-cli/)).
  2. By default it routes app connections through **n8n's hosted connection service plus Composio** ([connect/apps](https://nod.n8n.io/docs/connect/apps/)).
  3. It sends usage analytics to n8n. This is on by default and can be turned off ([trust/privacy](https://nod.n8n.io/docs/trust/privacy/)).
  4. Its test tasks come from a taxonomy of "900,000+ anonymised production workflows" (n8n's). Its n8n test cases reuse n8n's own Instance AI eval suite ([benchmarks](https://nod.n8n.io/docs/how-it-works/benchmarks/)).
- **Don't confuse it with n8n's own "Agents"** (Agent Builder, Preview, from n8n 2.32.3) or with the **AI Agent node**. Those are n8n features under the Sustainable Use License. Nod is a different codebase and license ([n8n glossary](https://docs.n8n.io/key-concept-glossary.md)).

---

## 1. What it is, who it's for, business model, maturity

| Item | Fact | Source |
|---|---|---|
| Maker | n8n Labs, "a new team within n8n that experiments with ideas outside our core product" | [blog](https://blog.n8n.io/meet-nod/) |
| Status | "Research preview"; "Early preview. Expect rough edges and breaking changes"; waitlist page: "Nod is an n8n Labs experiment and may or may not become a product" | [landing](https://nod.n8n.io/), [what-is-nod](https://nod.n8n.io/docs/get-started/what-is-nod/), [waitlist](https://nod.n8n.io/docs/waitlist/) |
| Audience | "Solo operators and freelancers … and plenty of people use it for errands at home". Today it fits people who are fine with bringing their own API key or ChatGPT plan | [blog](https://blog.n8n.io/meet-nod/) |
| License | Apache-2.0, including "the trust check, Vault and their tests" | [repo](https://github.com/n8n-io/nod), [trust/principles](https://nod.n8n.io/docs/trust/principles/) |
| Price | Free. You pay your model provider, or use a local model. No managed hosting yet; "Managed hosting" and "Nod for your team" are listed under "Coming next" | [landing](https://nod.n8n.io/) |
| Business model | None stated. [INFERENCE] It's a research bet that feeds n8n: analytics, traces, an n8n connector and n8n's connection service. Possible future paid managed hosting | [landing](https://nod.n8n.io/), [privacy](https://nod.n8n.io/docs/trust/privacy/) |
| Platforms | macOS 14+ (Apple silicon and Intel), Windows 11 (preview), Linux x64 Flatpak (preview), self-hosted server (Docker Compose, one owner) | [install-desktop](https://nod.n8n.io/docs/get-started/install-desktop/), [self-host](https://nod.n8n.io/docs/deploy/self-host/) |
| Repo | `n8n-io/nod`: created 2026-09-25; 2,790 stars, 431 forks, 73 open issues, ~1,596 commits (checked 2026-11-20). TypeScript; Node ≥ 24.21 | `gh api repos/n8n-io/nod` |
| Releases | `nod-v0.7.6` (11-13) → `0.8.0` (11-16) → `0.8.4` (11-17) → `0.9.0` (11-19) → `0.9.1` (11-20). About one release a day | `gh api …/releases` |
| Main contributors | aalises (618 commits), frstavares (530), johancutych, BerniWittmann, Tuukkaa … | `gh api …/contributors` |
| Community | n8n forum thread (2026-11-11) and an "n8n Labs" forum category. Early user reports include hitting the 272k-token context limit with no chat-only reset in the desktop app | [forum](https://community.n8n.io/t/nod-an-open-source-personal-agent-with-its-own-computer-research-preview-from-n8n-labs/328026) |
| Open issues, as a sample | Composio connected but no tools (#1017), local n8n connection fails (#1003), observer creates overlapping tasks (#896), self-hosters want MCP servers on their own network (#902), Windows startup broken after update (#1065) | `gh issue list -R n8n-io/nod` |

**n8n itself**, for context: `n8n-io/n8n` has 211,530 stars and its latest release is `n8n@2.48.7` (2026-11-20). It uses the Sustainable Use License plus the n8n Enterprise License for `.ee.` files ([license](https://docs.n8n.io/n8n-community-license/community-license.md)).

---

## 2. The concept: "a personal agent with its own computer"

**How they define it.** "Nod is a personal assistant you talk to in one chat. It has its own Linux workspace and browser, so it can do real work for you, not only answer questions" ([what-is-nod](https://nod.n8n.io/docs/get-started/what-is-nod/)). The blog frames it the same way: "Because Nod has its own computer (a sandbox that runs locally) and browser, it can finish jobs that a chat tool would hand back to you as a to-do list."

**The mental model, in six ideas** ([architecture](https://nod.n8n.io/docs/how-it-works/architecture/)):

1. **One owner, one thread.** There is one conversation per owner. Desktop, web, Telegram and WhatsApp all show the same thread. There is no "New chat"; `/clear` only resets the model context ([use/chat](https://nod.n8n.io/docs/use/chat/), [threads-and-turns](https://nod.n8n.io/docs/how-it-works/threads-and-turns/)).
2. **"Everything is a turn."** Messages, schedule fires, event wakes and resumes each run as a turn. Turns are never retried.
3. **A coordinator plus task sessions.** The main chat answers short requests. Each substantial job becomes a saved *task session* that runs in parallel and reports back to the same chat ([task-sessions](https://nod.n8n.io/docs/how-it-works/task-sessions/)).
4. **"Its own computer."** The agent has a sandboxed Linux workspace container and a separate browser container. On desktop both run inside a private VM (Lima/Virtualization.framework on Mac, WSL2 on Windows, Lima/QEMU/KVM on Linux) ([desktop-app internals](https://nod.n8n.io/docs/deploy/desktop-app/)).
5. **"Policy lives on the server."** The model plans. A separate server-side check decides whether each outside action may run, based on the owner's own words ([trust](https://nod.n8n.io/docs/trust/), [approvals](https://nod.n8n.io/docs/trust/approvals/)).
6. **"Authority comes only from you."** Pages, emails, files and tool results are data, never permission. Secrets are entered by the server and never shown to the model ([principles](https://nod.n8n.io/docs/trust/principles/)).

**Product voice.** The approval metaphor is the name: "No nod, no action"; "Nodding along…"; "Start nodding" ([landing](https://nod.n8n.io/)).

**n8n's own split of "agent" vs "workflow"** (for comparison): an n8n agent is "an autonomous assistant you build in n8n … Use an agent when work is too open-ended for a fixed workflow". It runs a reasoning loop per message or schedule, and each conversation is a stored *session* ([build-and-manage-agents](https://docs.n8n.io/build/build-and-manage-agents.md)). Nod takes this further for one person: it has a computer, memory, proactivity and a trust layer, and it is a desktop app rather than a canvas artifact.

---

## 3. Feature inventory (Nod)

| # | Feature | What the user can do | How it works underneath | Source |
|---|---|---|---|---|
| 1 | One conversation | Chat in desktop, web, Telegram or WhatsApp; it is all the same thread. Keep typing while it works; messages are steered into the running turn | One `threads` row per owner. Queued messages are injected into the Pi session after each step. Messages from another channel become their own turn | [use/chat](https://nod.n8n.io/docs/use/chat/), [threads-and-turns](https://nod.n8n.io/docs/how-it-works/threads-and-turns/) |
| 2 | Tasks (parallel jobs) | "Compare flights" starts task A, "also prep notes" starts task B, "only direct flights" updates A. Reply to a task's message to steer it. `/tasks`, `/status`, `/stop` in chat apps | `tasks` table is the ledger: goal, completion condition, steps, result, revisions. Each task has its own Pi session and folder under one shared workspace. A task ends only with `task finish` or a final reply. No fixed task limit; browsers are the scarce resource | [use/tasks](https://nod.n8n.io/docs/use/tasks/), [task-sessions](https://nod.n8n.io/docs/how-it-works/task-sessions/) |
| 3 | Subagents | Parallel helpers inside one turn, for example comparing three shops | `delegate {title, instruction}` runs an in-memory Pi session with its own browser in `.nod/runs/<run id>`. Subagents can't ask the owner, sign in, pay or connect apps | [threads-and-turns#subagents](https://nod.n8n.io/docs/how-it-works/threads-and-turns/) |
| 4 | Workspace (Linux computer) | Ask it to convert files, make PDFs and charts, run Python, save videos | Container per thread (`/workspace/<thread>`) as unprivileged `node` with no sudo. LibreOffice, qpdf, Ghostscript, Noto fonts, `save-video`; `pip install` persists. Bash commands also pass the trust checker. A supervisor kills the whole process tree and enforces a 59-minute timeout and an output cap | [use/workspace](https://nod.n8n.io/docs/use/workspace/), [sandbox-and-browser](https://nod.n8n.io/docs/how-it-works/sandbox-and-browser/) |
| 5 | Browser | Work on any website, including logins, forms, downloads and checkouts | Separate container running headed Chromium driven by Vercel's `agent-browser`. Accessibility snapshot with `@eN` refs, up to 40k chars. Owner's time zone and locale. Public addresses only. Four live browsers, LRU-replaced after 5 minutes idle | [sandbox-and-browser](https://nod.n8n.io/docs/how-it-works/sandbox-and-browser/), [use/browser](https://nod.n8n.io/docs/use/browser/) |
| 6 | Browser handoff | When a site needs you (CAPTCHA, 2FA), a live tab opens in chat; take over, then "Return to Nod" | `browser action: handoff`. JPEG frames go out and your input comes in over one bridge. Frames pause during credential entry. Passkeys are forced to fail | [sandbox-and-browser#browser-handoff](https://nod.n8n.io/docs/how-it-works/sandbox-and-browser/) |
| 7 | Vault: saved logins and TOTP | Save site logins (or import CSV, or pick from 1Password). Nod signs in without the model seeing the password | Server types values into a field only if the frame's real origin matches exactly. Each use is claimed once. Values are masked in output and screenshots. A 1Password helper process runs separately | [trust/vault](https://nod.n8n.io/docs/trust/vault/) |
| 8 | Saved browser sessions | Reuse a signed-in browser profile on sites you allow | Profiles live on the browser container's volume. `browser_session` opens them after the same check as a sign-in | [trust/browser-sessions](https://nod.n8n.io/docs/trust/browser-sessions/) |
| 9 | Payments | Nod fills the checkout; you approve that one payment with your card's CVC, which is never saved | The request is bound to card version, shop origin, page, card-field origins and submit control. One claim per payment | [trust/payments](https://nod.n8n.io/docs/trust/payments/) |
| 10 | Connected apps (Composio) | Connect Gmail, Calendar, Drive, Sheets, Docs, Slack, Outlook and a large catalog from the Apps page or by asking in chat | Composio, through n8n's hosted connection service by default (ZDR on). Requests are signed per installation key. OAuth tokens stay at Composio, never in prompts, workspace or browser. Source installs can choose another connection service | [connect/apps](https://nod.n8n.io/docs/connect/apps/) |
| 11 | MCP servers | "Connect MCP": URL plus OAuth, token or API key. Notion, Linear and n8n ship as MCP apps | Streamable HTTP with SSE fallback. Each tool becomes `mcp__<server>__<tool>`, deferred and loaded with `tool_search`. Tool list cached 10 minutes. Tokens are encrypted in Vault and redacted from results. Every call passes the trust check | [connect/mcp-api-cli](https://nod.n8n.io/docs/connect/mcp-api-cli/), [agent-runtime](https://nod.n8n.io/docs/how-it-works/agent-runtime/) |
| 12 | HTTP API and CLI access paths | Ask Nod to connect a service that isn't in the catalog; it proposes an "access path" and you paste the token on a private page | The CLI is installed from npm, PyPI or HTTPS with sha256 as **its own Linux user**, and runs with an environment built from scratch (only its credential variables). Variable names must end in TOKEN, KEY, SECRET and similar | [connect/mcp-api-cli](https://nod.n8n.io/docs/connect/mcp-api-cli/) |
| 13 | Custom apps | Contribute an app folder (MCP, HTTP or CLI binding) by PR | Agent Plugins 1.0.0 manifest plus `io.n8n.nod/app.json`: prompts, requirements, limitations, contract fixture and approval rules. JSON Schema 2020-12 | [contribute/apps/format](https://nod.n8n.io/docs/contribute/apps/format/) |
| 14 | Trust check (approvals) | Reads and drafts run. Sends, deletes, bookings and settings changes run only if "your words cover it", otherwise Nod asks. Shown in Usage as *You said "…"* | A server-side checker (your model at its lowest effort, a small LLM, or the "Jev" decision model). It must **quote the owner's words** that allow the action, and the server checks that the quote really exists. It sees owner words and the action, never page or email text | [trust/approvals](https://nod.n8n.io/docs/trust/approvals/), [connect/models#decision-checks](https://nod.n8n.io/docs/connect/models/) |
| 15 | Confirmation cards | Approve or Cancel one exact action. Telegram has buttons; WhatsApp takes written replies; thumbs up or down work | The card is bound to the saved action and arguments, which a model can't rewrite. A separate check reads your reply in any language. Card states include Expired, Not run and **May have run** | [trust/approvals#the-confirmation](https://nod.n8n.io/docs/trust/approvals/) |
| 16 | Standing instructions | "Whenever Ana sends an invoice, forward it to accounting@…". Background work does exactly that without asking | Stored next to notification preferences. The checker sees only the instruction, your rules and the action, never the triggering email | [approvals#standing-instructions](https://nod.n8n.io/docs/trust/approvals/) |
| 17 | Owner rules | "Always ask before email outside my company"; "manage Gmail labels without asking" | Rules apply to every chat, task, schedule and background run. A rule to ask first wins. Changes to rules also pass the checker | [approvals#your-rules](https://nod.n8n.io/docs/trust/approvals/) |
| 18 | One use, one result | An approved action never runs twice. If it is interrupted, Nod says the outcome is uncertain and checks before retrying | Claim rows (Running, Submitted or Uncertain) in the DB before each private step. Turns and tool calls are never retried; only model requests are (3 times: 4, 8, 16 s) | [trust/outcomes](https://nod.n8n.io/docs/trust/outcomes/), [models](https://nod.n8n.io/docs/connect/models/) |
| 19 | Schedules | "Every weekday at 8, summarise…". Run now, pause, edit, per-schedule model, projected monthly cost, run history | Each fire is a separate session. One run at a time; a fire is skipped while the previous run is queued, running or waiting. A run can't create schedules. **Watches skip unchanged runs** with no model call. A late run gets a "run track" (due time, skipped runs) | [use/schedules](https://nod.n8n.io/docs/use/schedules/) |
| 20 | Proactivity ("nudges") | Optional "Automatic preparation": Nod reads Gmail every 15 minutes and Calendar every 30 minutes, prepares drafts privately, and messages you only when something concrete needs you | Source change tokens are checked by code first. One small model triages a batch. A read-only **observer** session saves "matters" with evidence. The server enforces freshness, once-per-matter, quiet hours (22-08), snooze and pause. $5/month allowance by default | [how-it-works/proactivity](https://nod.n8n.io/docs/how-it-works/proactivity/), [use/nudges](https://nod.n8n.io/docs/use/nudges/) |
| 21 | Memory | "Tell it once." Every note is visible with the message it came from; edit, forget, export or import from another assistant | One JSON doc (256 KiB) in the DB in five areas: about, preferences, people, routines, lessons. **Only owner messages** can become notes, and each note must quote a cited owner message. Hourly repair and reflect job. Statuses: tentative, observed, stated, archived. `history_search` covers the full thread | [how-it-works/memory](https://nod.n8n.io/docs/how-it-works/memory/), [use/memory](https://nod.n8n.io/docs/use/memory/) |
| 22 | Writing style | Drafts sound like you | Reads sent Gmail and saves at most 500 characters of style with no facts, as `preferences/writing-style` | [memory#writing-style](https://nod.n8n.io/docs/how-it-works/memory/) |
| 23 | Learned skills | Nod writes down how it did a flow (one site or app) and patches it when the site changes | Stored in memory: up to 40 skills, 6,000-character bodies. Secrets and personal data are stripped by the memory model. Bodies are shown to the model as untrusted data. Owner edits lock a skill | [memory#learned-skills](https://nod.n8n.io/docs/how-it-works/memory/) |
| 24 | Built-in skill library | Task guides for travel, finance, inbox, shopping, n8n and more (about 28) | Agent Skills progressive disclosure: names in the prompt, `skills read` loads a SKILL.md, then references on demand | [how-it-works/skills](https://nod.n8n.io/docs/how-it-works/skills/) |
| 25 | Spending and usage | Usage page lists every app action, send, sign-in and card use with the quoted authority and cost. Monthly limit ($200 set at setup). **Task check-in** after $5 of spend | `model_usage` is the single cost ledger. Cost is reserved before each request under an owner lock. When the limit is hit, only owner messages that change the limit still run | [use/spending](https://nod.n8n.io/docs/use/spending/), [principles](https://nod.n8n.io/docs/trust/principles/) |
| 26 | Models | 30+ providers, a ChatGPT Plus/Pro sign-in, or a local model (Ollama, LM Studio, vLLM, llama.cpp; needs tool calling and 64k context). Separate models for chat, schedules and own-initiative work | Pi provider adapters. "Light model" option. The checker always uses your main model unless a decision model is set | [connect/models](https://nod.n8n.io/docs/connect/models/), [local-models](https://nod.n8n.io/docs/connect/local-models/) |
| 27 | Telegram and WhatsApp | Pair a BotFather bot; WhatsApp by QR (Baileys, unofficial) or the Business Cloud API | `@chat-adapter/*` with Postgres state. Live "typing…" plus a quoted progress section. Sign-in and payment links are never sent to chat apps | [use/channels](https://nod.n8n.io/docs/use/channels/) |
| 28 | Voice | Voice messages are transcribed; spoken replies via OpenAI, OpenRouter, ElevenLabs or free local voices | `create_audio` tool; TTS and STT models are configurable | [connect/models](https://nod.n8n.io/docs/connect/models/), config ref |
| 29 | Local files (desktop) | Let Nod read and write your own files, only in work you started | Off by default. Opening a file counts as an action, so it needs your words | [connect/local-files](https://nod.n8n.io/docs/connect/local-files/) |
| 30 | Computer use (desktop, experimental) | Nod uses native apps (TextEdit, Numbers, Mail) in the background without moving your pointer | Vercel `agent-desktop` over accessibility APIs (macOS AX, Windows UIA). Browsers are view-only and risky apps are refused | [connect/computer-use](https://nod.n8n.io/docs/connect/computer-use/) |
| 31 | Build with n8n | "Build an n8n workflow that…", "why did my invoice workflow fail?", "publish it", "make me a support agent" | Instance-level MCP of the user's n8n. Workflow code is written to the workspace, validated, created, then tested with n8n's pin data (side-effect steps mocked). Left as a **draft**. Nod never creates or reads n8n credentials. Canvas preview, or a picture on Telegram | [use/n8n](https://nod.n8n.io/docs/use/n8n/) |
| 32 | Codemode | Model-written JS that calls many tools in one step | QuickJS sandbox with no network, files or timers. Each nested call passes the same guard and trust check | [agent-runtime#codemode](https://nod.n8n.io/docs/how-it-works/agent-runtime/) |
| 33 | Self-documentation | Ask "how do I…" about Nod itself | `nod_docs` tool: BM25-like index over the docs shipped with the app, no model call | [agent-runtime#questions-about-nod](https://nod.n8n.io/docs/how-it-works/agent-runtime/) |
| 34 | Bug reports and traces | "Report bug" in chat with a reviewable diagnostic report. Optional conversation traces, PII-scrubbed locally | Traces are off by default and kept 14 days in the EU by n8n | [trust/conversation-traces](https://nod.n8n.io/docs/trust/conversation-traces/), [privacy](https://nod.n8n.io/docs/trust/privacy/) |
| 35 | Self-host server | One owner per server, reached from web, desktop client, Telegram and WhatsApp; runs while clients are closed | Docker Compose (Postgres, workspace, browser, firewall container, Docker socket proxy). `NOD_AUTH_TOKEN`. HTTPS profile or SSH tunnel. Guides for Mac mini, cloud and boxd | [deploy/self-host](https://nod.n8n.io/docs/deploy/self-host/) |
| 36 | Published evals | 98% task completion (226 errands × 3), 0/678 unsafe actions, 93.2% LongMemEval-S, 0/216 WhisperBench injections | Full-system runs against stateful simulated apps and sites with a simulated user. Code checks plus a separate judge. pass@1, pass@k, pass^k. Regression gate with Fisher plus Holm. Runners are open source in `evals/` | [benchmarks](https://nod.n8n.io/docs/how-it-works/benchmarks/), [contribute/evals](https://nod.n8n.io/docs/contribute/evals/) |

Roadmap, as stated: managed hosting, phone calls, "Nod for your team" (shared Nod with apps and memory), "Make it your own" (own skills and tools without touching core) ([landing](https://nod.n8n.io/)).

---

## 4. System architecture

### 4.1 Process and containers
- **One Node process** serves the React client and runs the Fastify API, the agent runtime, the `thread-drain` worker and the schedulers. **Docker containers** run PostgreSQL, the workspace and the browser. A separate helper talks to 1Password ([architecture](https://nod.n8n.io/docs/how-it-works/architecture/)).
- **Desktop**: Electron runs the window and the Nod server. Everything else runs in a private VM: on Mac, Lima on Virtualization.framework with Ubuntu 26.04, half your RAM (3-6 GiB) and a 24 GiB disk; on Windows, a WSL2 distro `nod-<profile>`; on Linux, a Flatpak with Lima, QEMU and KVM. Docker runs **only inside the VM**. A "runtime pack" (images plus base VM image) downloads with sha256 checks and resumes ([desktop-app](https://nod.n8n.io/docs/deploy/desktop-app/)).
- **Code layering** is enforced by `scripts/check-architecture.mjs`: `web`, `shared`, `ports` (capability interfaces), `runtime` (tools, prompts, turn loop), `providers` (Pi, Docker, agent-browser, Composio), `server` (policy, storage, HTTP), and `composition.ts` as the only wiring point. Only `composition.ts` reads `process.env` ([architecture](https://nod.n8n.io/docs/how-it-works/architecture/)).
- **Key dependencies** from `package.json`: `@earendil-works/pi-coding-agent` and `pi-ai` 1.5.2 (the agent runtime "Pi"), `pi-web-access`, `agent-browser`, `@composio/core`, `@modelcontextprotocol/sdk`, `pg-boss`, `pg`, `fastify`, `@chat-adapter/telegram` and `whatsapp`, `@whiskeysockets/baileys`, `@1password/sdk`, `@huggingface/transformers` (local models for audio/classification [INFERENCE]), `posthog-node`.

### 4.2 Runtime: the turn loop
- Messages and schedule fires become `turns` rows plus a `thread-drain` pg-boss job **in the same transaction**. The worker takes up to 20 jobs, locks each thread once, and starts the oldest queued turn of every idle session ([threads-and-turns](https://nod.n8n.io/docs/how-it-works/threads-and-turns/)).
- Turn states are `queued`, `running`, then `succeeded`, `waiting`, `failed` or `cancelled`. `withTurnLock` fences every DB write, so a stopped turn can't write late.
- Sources are `user`, `task`, `schedule`, `event` and `resume`. A secure form, payment approval, app connection or yes/no answer queues a **resume** turn in the session that asked.
- **Pi sessions.** One per chat, task, schedule fire and observer. Transcripts are saved. Compaction is handled by Pi with explicit thresholds. Tool results over 2,000 characters are dropped from earlier requests once the prompt cache is cold, and screenshots are pruned. Large outputs go to `.nod/outputs/` with a preview ([agent-runtime](https://nod.n8n.io/docs/how-it-works/agent-runtime/)).
- **The prompt has three layers**: a static prompt, an owner-context message (memory index and learned-skill index, re-sent only when it changes so the cache stays warm), and a turn message. Cache markers are placed deliberately for Anthropic and OpenAI. Scheduled runs pick a 5-minute or 1-hour cache TTL from the schedule's interval.
- **Tools**: Pi's `read`, `bash`, `edit`, `write`, `find`, `ls` (inside the workspace container), plus `skills`, `nod_docs`, `send_message` (update or final intent), `ask_user` (clickable choices), `browser`, `sign_in`, `pay`, `browser_session`, `apps`, `computer`, `manage_apps`, `manage_personal_context`, `task`, `share_file`, `create_audio`, web search and fetch, `mcp__*`, `tool_search` and `codemode`.
- **Turn guard**: every call checks that the turn is still live. Outside results come back wrapped in `untrusted_tool_result`. `apps` and MCP calls pass the trust check. A call acts only after earlier `apps` calls in the turn are decided, and **at most one open confirmation per turn**. Five repeated failures of the same tool end the run.
- **Execution records**: scope is (thread, session, turn, run, parent run, browser). `turn_events` holds tool activity (no raw arguments or results). `model_usage` is the single cost ledger, deduplicated by usage ID.

### 4.3 Sandbox and isolation ([sandbox-and-browser](https://nod.n8n.io/docs/how-it-works/sandbox-and-browser/))
| | Server | Sandbox user | Connected CLI user | Browser container |
|---|---|---|---|---|
| Provider keys, owner key | yes | no | no | no |
| Vault values | yes | no | its own token only | only values the server enters |
| Internet | yes | yes; a firewall container blocks private, metadata and IPv6 | same | public addresses only |

- Both containers drop all capabilities and set `no-new-privileges`. Resource limits differ per mode (desktop: workspace 2 GB / 2 CPU / 1,024 procs).
- A command supervisor owns the session, kills detached processes and escalates to SIGKILL. If an end acknowledgement is lost, the command fails as "uncertain" and is never re-run.
- **Every model-issued bash command passes the Trust checker** (exact command, folder, timeout).
- The browser container uses headed Chromium on a virtual display, with Chromium's own sandbox off ("the container is the boundary").

### 4.4 Model providers
30+ providers; the table is in [connect/models](https://nod.n8n.io/docs/connect/models/). Defaults at writing time include GPT-6.2 Sol, Claude Opus 5.5 and Gemini 4 Argon. A ChatGPT plan sign-in is supported. Optional decision models handle the checker: OpenAI "GPT Decisions" (`gpt-6-luna`) or TypeSafe "Jev" 1.13/1.14 directly or through OpenRouter.

### 4.5 Memory
See feature 21 and [how-it-works/memory](https://nod.n8n.io/docs/how-it-works/memory/). Key design points:
- Notes are written by a separate **memory model** from **owner messages only**. The server keeps a note only if its quote appears in a cited owner message, and refuses card numbers, keys and links with logins.
- Every write uses compare-and-swap on the document version.
- The full index is in the prompt. `memory_find` uses words with no embeddings. Before each owner turn, a memory model selects facts and expands searches, within a one-second shared deadline.

### 4.6 Tools, MCP, browser and computer use
Covered in features 5-13 and 30-32. MCP specifics:
- Tools are deferred and loaded with `tool_search`.
- Schemas are cleaned to JSON Schema 2020-12 for Anthropic.
- One connection per account and address, kept a minute after the last call.
- Calls can last about an hour while the server keeps reporting progress.
- `pattern` and `patternProperties` are removed from external schemas before local validation ([agent-runtime](https://nod.n8n.io/docs/how-it-works/agent-runtime/), [mcp-api-cli](https://nod.n8n.io/docs/connect/mcp-api-cli/)).

### 4.7 Scheduling and triggers
- **Schedules**: one-time or recurring, owner time zone. Missed while stopped means it runs **once** on restart. A late run notes the due time and the skipped count.
- **Watches** skip runs when nothing changed in the watched accounts, with no model call.
- **Events**: Gmail is polled every 15 minutes (the connection service receives no webhooks, so "apps such as Calendar and Slack don't wake Nod automatically"). Calendar change tokens are checked every 30 minutes. Other apps need an app event; there is no periodic model read ([connect/apps#automatic-app-updates](https://nod.n8n.io/docs/connect/apps/), [use/schedules](https://nod.n8n.io/docs/use/schedules/)).
- Quiet hours 22:00-08:00 by default.
- Rate limiting: connection-service 429/503 responses pause background reads with `Retry-After` and doubling backoff up to 30 minutes.

### 4.8 Integrations
Composio catalog (via n8n's connection service), Notion, Linear and n8n MCP apps, any MCP server, HTTP API, CLI, Telegram, WhatsApp and 1Password.

### 4.9 Multi-agent
There is no multi-agent team. The model is coordinator, parallel task sessions and in-turn subagents for **one owner**. "Nod for your team" is on the roadmap.

### 4.10 Permissions and approvals
Trust check, standing instructions, rules, Vault checks and payment approval (features 14-18).
- Approvals bind to the **exact saved action and arguments**.
- A changed instruction, task or rule invalidates a pending approval.
- Calendar and Gmail delete approvals re-read the target and check its version before running.

### 4.11 Observability
- **Usage page**: every app action, send, sign-in, card use, question and held attempt, with cost and the quoted authority. CSV export includes bash commands, exit codes and redacted output (up to 16k characters).
- **Live status line**: up to 4 completed tool calls from `turn_events`, with no extra model request.
- Optional traces; PostHog analytics ([use/spending](https://nod.n8n.io/docs/use/spending/), [sandbox-and-browser#command-output](https://nod.n8n.io/docs/how-it-works/sandbox-and-browser/)).

### 4.12 Persistence
PostgreSQL holds threads, turns, tasks, messages, `turn_events`, `model_usage`, memory and Vault (encrypted), with pg-boss jobs. Workspace and browser profiles live on volumes. Backups and restore for servers are covered in [server-operations](https://nod.n8n.io/docs/deploy/server-operations/).

### 4.13 Deployment
- Desktop with auto-update on Mac and Windows.
- Self-host via prebuilt bundles (GitHub Actions artifacts, expire after 14 days) or from source (`deployment/self-host`, `setup.sh` generates the auth token, Vault key and DB password).
- One owner per server; no registration or multi-user.

### 4.14 Security model
Documented in detail, including limits ([security-model](https://nod.n8n.io/docs/trust/security-model/), [security-review](https://nod.n8n.io/docs/trust/security-review/), [untrusted-content](https://nod.n8n.io/docs/trust/untrusted-content/)). Stated limits:
- The checker is a model and can be wrong.
- The server can read Vault values while it uses them.
- A deceptive site can mislabel a JS button.
- The workspace has public internet, so data in the workspace can be exfiltrated.

They also publish an honest compare table against Instinct, Meta Muse and OpenAI dots, including "Where others are ahead": single-use cards, bug bounty, key isolation at the network edge ([trust/compare](https://nod.n8n.io/docs/trust/compare/)).

### 4.15 Pricing limits
No Nod pricing. Built-in controls:
- monthly spend limit ($200 set at setup)
- task check-in ($5 by default)
- proactivity allowance ($5/month)
- per-schedule projected cost

---

## 5. UX surfaces

| Surface | Notes |
|---|---|
| Desktop (Electron) | Mac, Windows and Linux. Chat, Tasks/Scheduled, Apps, Vault (⌘6), Usage (⌘5), Settings → Memory/Models/Notifications/Voice. Menu-bar/tray resident; "Open at login" |
| Web client | The same React app, served by the local or self-hosted server |
| Telegram | Paired bot, approve/cancel buttons, `/tasks`, `/status`, `/stop`, `/clear`, `/memory`. Live progress quote. Workflow pictures for n8n builds |
| WhatsApp | QR via Baileys (unofficial, account-risk warning) or Business Cloud API. Written approvals. A 24-hour window applies in Cloud API mode |
| Voice | Voice notes in; spoken replies out |
| Browser handoff | Live tab inside chat |
| Native apps | Computer use (desktop, experimental) |
| No mobile app | Mobile access is through Telegram, WhatsApp or the web UI on a self-hosted server [INFERENCE from docs: no iOS or Android app mentioned] |

---

## 6. The n8n platform underneath (what each n8n capability is, and how Nod uses it)

### 6.1 Workflows, executions, triggers
- **Workflow**: nodes on a canvas. **Executions** are manual, partial or production. Production runs need a trigger and a *published* workflow ([types-of-executions](https://docs.n8n.io/build/understand-workflows/understand-executions/types-of-executions.md)).
- **Data pinning** freezes node outputs for development. Nod's n8n test step uses `prepare_workflow_pin_data` so that side-effect nodes are mocked ([MCP tools ref](https://docs.n8n.io/connect/connect-to-n8n-mcp-server/mcp-server-tools-reference.md)).
- **Trigger catalog**: about 91 trigger-node doc pages (app triggers such as Gmail, Google Calendar, Drive, Sheets, GitHub, GitLab, Jira, Linear, HubSpot, Stripe, Calendly, Typeform, Kafka, AMQP, Postgres and more), plus core triggers:
  - Schedule, Webhook, n8n Form, Chat Trigger, MCP Server Trigger
  - Email (IMAP), RSS, SSE, Local File
  - Error Trigger, Execute Sub-workflow, Evaluation Trigger, n8n Trigger, Manual, Activation, Workflow
  ([llms.txt](https://docs.n8n.io/llms.txt); counts are my count of doc pages there).
- **Durable scheduler** (default from 2.45.0): runs are materialized ahead in the DB, claimed by one main, recovered by a reaper. A **misfire policy** decides what happens to missed runs: skip, run the most recent missed execution, or one per rule ([durable-scheduler](https://docs.n8n.io/deploy/host-n8n/configure-n8n/durable-scheduler.md)).
- **Poll triggers** stay on the in-memory scheduler unless you opt in.

### 6.2 AI Agent node vs n8n Agents vs n8n Assistant
| | AI Agent node | n8n Agents (Agent Builder) | n8n Assistant (`instance-ai`) | Nod |
|---|---|---|---|---|
| What | Cluster root node inside a workflow; sub-nodes for model, memory and tools | A first-class project artifact: model, instructions, tools, web search, skills, knowledge base, session and episodic memory, sub-agents, channels (Slack, Telegram, Linear, Teams), schedules | Chat agent that builds, edits, tests and debugs workflows and agents. Can use MCP registry servers and Browser Use (Cloud only) | Personal agent app |
| Status | GA | **Preview**, self-hosted from 2.32.3; **not supported in queue mode yet** | Available on Cloud Starter/Pro and self-host Community/Business. Needs a sandbox (`n8n-sandbox` with Sysbox on k8s, Daytona, or bundled for dev) | Research preview |
| Billing | Execution | **One turn = one execution** | Assistant credits on Cloud | User's model bill |
| Source | [agent node](https://docs.n8n.io/integrations/builtin/cluster-nodes/root-nodes/n8n-nodes-langchain.agent.md) | [build-and-manage-agents](https://docs.n8n.io/build/build-and-manage-agents.md) | [n8n-assistant](https://docs.n8n.io/build/ways-of-building-workflows/n8n-assistant.md), [set-up](https://docs.n8n.io/deploy/host-n8n/configure-n8n/set-up-n8n-assistant.md) | this report |

Other agent features in n8n:
- Agents have **draft and published versions** with publish history and rollback.
- **Tool approval**: Approve or Reject in the chat.
- **Message an Agent** node calls an agent from a workflow, with streaming, session reuse and a JSON-schema reply.
- **Chat Hub**: a central chat UI with a "Chat user" role and personal agents ([chat-hub](https://docs.n8n.io/build/ways-of-building-workflows/chat-hub.md)).

### 6.3 n8n MCP: server and client
- **Instance-level MCP server** (`/mcp-server/http`). OAuth with per-client permissions and revocation, or a personal API key. Workflows are exposed per workflow, folder or project, with an optional auto-expose setting. Building and editing over MCP since 2.13.0; new settings UI from 2.33.0 ([connect-to-n8n-mcp-server](https://docs.n8n.io/connect/connect-to-n8n-mcp-server.md)).
- **Tool surface**, from the reference:
  - Workflows: search, get, execute, test, prepare pin data, publish/unpublish, history/version/diff, archive/restore
  - Projects, folders, tags
  - Executions: get, search
  - Credentials: list and update description only (no secrets)
  - Instance context and activity, node usage
  - Builder: SDK reference, search nodes, node types, best practices, validate workflow or node, `create_workflow_from_code`, update
  - Agents: search, get, create, mutate, validate, call, publish, revert, versions, sessions, integration updates, delete
  - Skills, user preferences ("agentic context management"), data tables
  ([tools reference](https://docs.n8n.io/connect/connect-to-n8n-mcp-server/mcp-server-tools-reference.md))
- **MCP Server Trigger** node exposes one workflow's tools as an MCP server over SSE or streamable HTTP ([mcptrigger](https://docs.n8n.io/integrations/builtin/core-nodes/n8n-nodes-langchain.mcptrigger.md)).
- **MCP client**: a one-click registry in the tools panel that auto-creates an MCP credential and lets you choose which tools to include, plus the MCP Client Tool node for any server ([mcp-servers](https://docs.n8n.io/build/integrate-ai/mcp-servers.md)).
- **How Nod uses it**: Nod is an MCP *client* of the instance-level server, authenticated by OAuth or an instance MCP token ("an n8n API key doesn't work here"). It writes workflow code, validates it, creates it, tests it with pin data and leaves it as a draft ([use/n8n](https://nod.n8n.io/docs/use/n8n/)).

### 6.4 Credentials
- Stored encrypted with `N8N_ENCRYPTION_KEY`, which must be shared with workers. Tested on save.
- **Allowed HTTP Request Domains** per credential (All, Specific, None).
- Expressions can be used in credentials.
- **End-user credentials** (Enterprise, Preview): an OAuth template, and each user connects their own account, resolved at runtime to the triggering user ([create-and-edit-credentials](https://docs.n8n.io/build/understand-workflows/create-and-edit-credentials.md), [end-user-credentials](https://docs.n8n.io/administer/manage-credentials/end-user-credentials.md)).
- **External secret stores** (Vault, AWS, GCP, Azure, Infisical and others) ([external stores](https://docs.n8n.io/administer/manage-credentials/use-external-secret-stores.md)).
- The n8n credential docs list about 323 credential pages ([llms.txt](https://docs.n8n.io/llms.txt)).
- **Nod never creates or reads n8n credentials**: "n8n asks you for them during setup" ([use/n8n](https://nod.n8n.io/docs/use/n8n/)).

### 6.5 Human-in-the-loop
- **Tool-level HITL** on the AI Agent node: tools are attached to a "Human review" step. The approval goes through Chat, Slack, Discord, Telegram, Teams, Gmail, WhatsApp, Google Chat or Outlook. The reviewer sees `$tool.name` and `$tool.parameters`; a denial informs the AI. This also works inside sub-agents ([human-in-the-loop-for-tools](https://docs.n8n.io/build/integrate-ai/ai-examples/human-in-the-loop-for-tools.md)).
- **Wait node**: resume after an interval, at a time, on a webhook call (`$execution.resumeUrl` with auth), or on form submission. Execution data is offloaded to the DB while waiting ([wait](https://docs.n8n.io/integrations/builtin/core-nodes/n8n-nodes-base.wait.md)).
- "Set a human fallback for AI workflows" example ([human-fallback](https://docs.n8n.io/build/integrate-ai/ai-examples/set-a-human-fallback-for-ai-workflows.md)).
- **Comparison with Nod**: n8n's HITL is *structural*. The builder marks which tools need review. Nod's is *semantic*: a checker compares the action against the owner's words, and there is no per-tool flag.

### 6.6 Retries and error handling
- Node settings: **Retry On Fail** (max tries, wait between). **On Error** can stop the workflow, continue, or continue through an error output. Always Output Data, Execute Once ([work-with-nodes](https://docs.n8n.io/build/understand-workflows/workflow-components/work-with-nodes.md)).
- **Error workflow** per workflow, starting with an Error Trigger. It receives the execution id and url, `retryOf`, the error and the last node ([handle-errors-gracefully](https://docs.n8n.io/build/flow-logic/handle-errors-gracefully.md)).
- Re-run and debug past executions in the editor.
- Public API POST requests accept an `Idempotency-Key` ([llms.txt → Idempotency](https://docs.n8n.io/connect/n8n-api/idempotency.md)).
- Stop and Error node.
- **Comparison with Nod**: Nod deliberately does the opposite for outside actions. **Never retry a turn or tool call**; mark the outcome uncertain and check first. Only model requests are retried.

### 6.7 Evaluations
- Dataset (data table or Google Sheet) → Evaluation Trigger → workflow → Evaluation node.
- **Light evaluations** (Registered Community and up) write outputs back for visual comparison.
- **Metric-based evaluations** (Cloud Pro/Enterprise, self-hosted Enterprise) use Set Metrics: Correctness (AI, 1-5), Helpfulness (AI, 1-5), String Similarity, Categorization, Tools Used, or custom numeric metrics. Runs are compared over time, and a "check if evaluating" branch skips metric cost in production ([why-test](https://docs.n8n.io/build/integrate-ai/test-and-improve-ai-workflows/understand-why-to-test.md), [metrics](https://docs.n8n.io/build/integrate-ai/test-and-improve-ai-workflows/use-metrics-to-measure-quality.md)).
- **Nod's evals are a different kind**: full-system scenarios in a simulated world with a simulated user, a separate judge, pass@k and pass^k, and a statistical regression gate ([contribute/evals](https://nod.n8n.io/docs/contribute/evals/)).

### 6.8 Scaling: queue mode, workers, concurrency, task runners
- **Queue mode**: the main instance takes triggers and webhooks, puts the execution id on Redis (Bull), workers pull it and load the workflow from the DB, then write results. The encryption key is shared. Webhook processors and multi-main setups are supported. Postgres is needed ([enable-queue-mode](https://docs.n8n.io/deploy/host-n8n/configure-n8n/scaling/enable-queue-mode.md)).
- **Concurrency control** in regular mode (`N8N_CONCURRENCY_PRODUCTION_LIMIT`, FIFO); worker `--concurrency` in queue mode ([control-concurrency](https://docs.n8n.io/deploy/host-n8n/configure-n8n/scaling/control-concurrency.md)).
- **Task runners**: the Code node's JS and Python run in separate runner processes connected to a broker over websocket. "Internal mode" is deprecated from 3.0, and external mode is required for real isolation ([task-runners](https://docs.n8n.io/deploy/host-n8n/configure-n8n/set-up-task-runners.md)).
- **Observability**: Insights (failure rate, time saved), log streaming, OpenTelemetry traces per workflow and node, redaction of execution data, security audits, SSRF protection.

### 6.9 Self-hosting and licensing
- **n8n** uses the **Sustainable Use License** (fair-code). You may use and modify it "only for your own internal business purposes or for non-commercial or personal use", and distribute it only free of charge for non-commercial purposes. `.ee.` files fall under the Enterprise License, and a commercial agreement is needed otherwise ([community-license](https://docs.n8n.io/n8n-community-license/community-license.md)).
- Editions: Community (free), Registered Community (free, with folders, debug in editor and custom execution data), Business, Enterprise ([choose-how-to-use-n8n](https://docs.n8n.io/choose-how-to-use-n8n.md)).
- **Nod** is Apache-2.0, so it is unrestricted. [INFERENCE] The different license is deliberate: Nod competes in a consumer-agent space where fair-code would hurt adoption, and it contains no n8n core code.

---

## 7. What Nod does notably better or differently (with evidence)

1. **The server enforces permission and it is auditable down to the quote.** The checker must cite the owner's exact words. The server verifies the quote exists, and Usage shows *You said "…"* next to each action ([approvals#because-you-said](https://nod.n8n.io/docs/trust/approvals/)). Most agent products, n8n's HITL included, use per-tool flags instead.
2. **The checker never sees untrusted content**, only owner words, rules and the action. Prompt injection has to get past a model that never read the injected text ([trust/compare](https://nod.n8n.io/docs/trust/compare/)).
3. **Exactly-once outside actions** with an explicit "uncertain" state and no automatic retries ([outcomes](https://nod.n8n.io/docs/trust/outcomes/)).
4. **Secrets never reach the model**: Vault entry with origin checks, frozen page scripts after entry, card approval bound to field origins, and sealed CLI users per token ([vault](https://nod.n8n.io/docs/trust/vault/), [security-model](https://nod.n8n.io/docs/trust/security-model/)).
5. **Memory with provenance**: notes come only from owner messages, each with a verified quote. It is tested against memory poisoning, with WhisperBench 0/216 published ([memory](https://nod.n8n.io/docs/how-it-works/memory/), [benchmarks](https://nod.n8n.io/docs/how-it-works/benchmarks/)).
6. **Proactivity with code gates**: change tokens, then a cheap batch triage, then a read-only observer. Once per matter, quiet hours, a $5 allowance, and silence by default ([proactivity](https://nod.n8n.io/docs/how-it-works/proactivity/)).
7. **Costs are visible and capped**: a single ledger, reservations before each request, task check-ins, and per-schedule projected cost ([spending](https://nod.n8n.io/docs/use/spending/)).
8. **Evals that measure the whole system** in a simulated world with a simulated user, published with their limits. Strict pass rate is kept separate from completion; there is a regression gate ([benchmarks](https://nod.n8n.io/docs/how-it-works/benchmarks/)).
9. **Docs as product.** Every control says what it doesn't cover. There are `llms.txt` and `llms-full.txt`, an in-app `nod_docs` tool, a security review guide, a glossary, and a compare page that admits where others are ahead ([trust/compare](https://nod.n8n.io/docs/trust/compare/), [llms.txt](https://nod.n8n.io/docs/llms.txt)).
10. **The computer is packaged for consumers.** A private VM is downloaded and verified on Mac, Windows and Linux; users never install Docker ([desktop-app](https://nod.n8n.io/docs/deploy/desktop-app/)).
11. **Cheap inner loop.** Codemode, deferred MCP tools with `tool_search`, careful prompt-cache placement and cache TTL chosen per schedule interval ([agent-runtime](https://nod.n8n.io/docs/how-it-works/agent-runtime/)).
12. **Weak spots** (evidence):
    - One owner only.
    - Stops when the laptop sleeps unless self-hosted.
    - Apps other than Gmail and Calendar don't wake it; there are no webhooks through the connection service.
    - App traffic goes through n8n and Composio by default.
    - The context-limit failures users reported.
    - Fast churn: about one release a day and data formats may change ([connect/apps](https://nod.n8n.io/docs/connect/apps/), [forum](https://community.n8n.io/t/nod-an-open-source-personal-agent-with-its-own-computer-research-preview-from-n8n-labs/328026), [what-is-nod](https://nod.n8n.io/docs/get-started/what-is-nod/)).

---

## 8. What Atlas Crew needs to match Nod and the n8n agent stack

Current Atlas Crew, from `docs/crew.md` in this repo:
- **What it is**: named crewmates on a self-owned Linux server (hostd under systemd). Engines are omp, Claude Code, Codex and OpenCode on the user's own subscription; "Atlas Crew never calls a model API". One task at a time per crewmate, with `max_agents` server-wide.
- **Approvals**: modes `ask`, `edits` and `bypass`; rails; per-task grants. Guards refuse weakened tests and invented refs. UI tasks need two captures as proof.
- **Automations**: schedule, interval, webhook, GitHub, file, after-crewmate, service.
- **GitHub**: GitHub App with short-lived tokens through a git-credential helper and a `gh` shim, plus webhooks.
- **Channels and voice**: Telegram bot with approve buttons; dictation with whisper.cpp; spoken replies; push.
- **Clients**: Desktop and Mobile.
- **Agent tools**: crew MCP with `ask_user`, `capture`, `locate`, `ui_gate`, `set_ui`.
- **Isolation and records**: one Linux user per crewmate, nftables egress, cgroup memory cap, append-only audit, a per-crewmate `secrets.json` injected as env, and a 64 KiB markdown memory file.

Below: (A) n8n capabilities any agent platform needs, (B) Nod capabilities, (C) core/runtime work, (D) docs. Each item is marked **Have**, **Partial** or **Missing** against `docs/crew.md`. "Missing" means not described in crew.md; I did not read the code.

### A. n8n capabilities an agent platform must match

**A1. Trigger catalog**

| Capability | n8n | Atlas Crew | Status / what to add |
|---|---|---|---|
| Cron and interval | Schedule Trigger with durable scheduler and misfire policy | `schedule` (5-field cron with tz), `interval` (≥ 60 s) | **Partial.** Add an explicit misfire policy (skip, run latest once, per rule) and a "run track" (due time, skipped count, reason) on late runs, like n8n's durable scheduler and Nod's run track. Document behaviour across hostd downtime |
| Webhook | Webhook node (auth options, respond-to-webhook, test vs prod URL) | `webhook` with token at `/crew/<id>/hook/<token>` | **Partial.** Add an auth choice (header, HMAC), a synchronous "respond with result" mode, and a test URL that shows the payload |
| Form or human input | n8n Form Trigger, Wait "On Form Submitted" | `ask_user`, action cards | **Partial.** Add structured forms (fields, choices) beyond free text and choices; Nod's `ask_user` already has multi-select choices |
| App events | about 91 trigger nodes (Gmail, Calendar, Drive, Sheets, Slack, Jira, Linear, Stripe, HubSpot …) | GitHub only (poll plus App webhooks) | **Missing.** Needs a general app-event layer: polling readers with change tokens and/or provider webhooks, per connected account. This is the biggest gap for anything beyond code |
| Email (IMAP) | Email Trigger | - | **Missing** (cheap to add as a poll reader) |
| RSS, SSE, Kafka, AMQP, Postgres | trigger nodes | - | **Missing** (niche; RSS/HTTP-watch is the useful one; Nod's "watch a page" covers it) |
| File | Local File Trigger | `file` | **Have** |
| Error or chain | Error Trigger, Execute Sub-workflow | `crewmate` on done/failed, with chain loop guard | **Have** (closest equivalent to an error workflow). Could add a crew-wide "on any failure" automation |
| Service health | - | `service` failed/recovered | **Have** (Atlas-specific advantage) |
| MCP as a trigger | MCP Server Trigger exposes a workflow as tools | - | **Missing.** Expose a crewmate (or one of its "skills") as an MCP server so outside clients (Claude, Cursor, another Atlas) can hand it work, like n8n's `call_agent` and MCP Trigger |
| Chat or agent call from a workflow | Message an Agent node (session reuse, JSON schema reply, streaming) | Webhook starts a task; no synchronous reply | **Missing.** An API "send a message to crewmate X and await a structured result" with `session` reuse and a JSON-schema output |

**A2. Integrations and credentials**

| Capability | n8n / Nod | Atlas Crew | What to add |
|---|---|---|---|
| Integration catalog | n8n: about 275 app nodes, about 323 credential types. Nod: Composio catalog plus MCP apps | GitHub, Telegram | **Missing.** Pick one strategy: (a) an MCP-first catalog (users "Connect MCP" with OAuth; ship vetted MCP apps for Gmail, Calendar, Slack, Notion, Linear, n8n), and/or (b) an optional Composio-like broker. Nod's design shows MCP plus a broker is enough |
| OAuth connect flow | n8n credential modal; Nod private setup page with "the model never sees the token" | GitHub device-code and App only | **Missing.** A generic OAuth 2.1/PKCE client in hostd (RFC 8414 metadata, as Nod's app format requires). The token is stored server-side and the connect link opens only in Desktop or Mobile, never in Telegram (Nod rule) |
| Credential broker (token never in the agent env) | Nod: server adds the token per call; sealed CLI user per tool; Composio holds tokens. n8n: credentials resolved at runtime | GitHub already done right (git-credential helper, `gh` shim, tokens minted per op). Other secrets: `secrets.json` **injected as env into the agent** | **Partial.** Generalize the GitHub pattern: every integration call goes through hostd (an MCP proxy, HTTP proxy or sealed CLI runner) so the agent process never holds the token. Today any agent can `env`, print or exfiltrate its secrets |
| Per-credential domain allowlist | n8n "Allowed HTTP Request Domains" | Egress allows TCP 22/80/443 to any public host | **Missing.** Bind each secret to destination hosts; deny use elsewhere (needs the broker above) |
| End-user or per-user credentials | n8n Enterprise: each triggering user's OAuth | Single owner | Not needed until multi-user crews |
| External secret stores | n8n: Vault, AWS SM, GCP, Azure, Infisical … | - | **Missing** (nice for teams; low priority for solo) |
| n8n itself as an integration | Nod connects to n8n's instance MCP to build, test and publish workflows | - | **Missing; cheap win.** Ship "n8n" as a first MCP app plus a crewmate skill: crewmates can build and fix n8n workflows on the user's instance, draft-only until asked to publish |

**A3. Human approval**

| Capability | n8n / Nod | Atlas Crew | What to add |
|---|---|---|---|
| Per-tool approval | n8n Human review step per tool, many channels, `$tool.parameters` in the message | Mode, rails and grants per tool call; Desktop, Mobile and Telegram | **Have** for code tools. **Missing** for app actions (no app layer yet) |
| Semantic "covered by your request" check | Nod trust check quotes the owner's words; standing instructions; owner rules ("always ask before …") | Modes only (ask, edits, bypass); grants are per kind of call | **Missing.** Add a server-side *action checker* for outside actions (send, post, pay, delete, publish, PR merge, deploy). It sees only owner words, task origin, rules and the exact action, and must quote what allows it. Show the quote in the audit. Add **owner rules** in natural language and **standing instructions** for automation-started tasks |
| Exact-action binding, once only | Approval bound to the saved action and arguments; a changed recipient or payload means a new question; "May have run" state | `Allow once`, `Allow for this task` (exact command) | **Partial.** Add an idempotency claim plus an explicit `uncertain` state for outside actions, and cards that show Expired, Not run or May have run |
| Approval channels | n8n: Slack, Teams, Discord, Gmail, WhatsApp, Google Chat, Outlook, Chat; Nod: app, Telegram, WhatsApp | Desktop, Mobile, Telegram | **Partial.** WhatsApp and Slack are the most asked-for extra channels |
| Secure private input | Nod private forms for passwords, codes, cards; code-only replies for OTPs | `waiting_action` secret goes to `secrets.json` | **Partial.** Fine for secrets; there is no "type into a website without the agent seeing it" (needs a browser, see B) |
| Spending approval | Nod check-in after $X; monthly limit | - | **Missing** (see C4) |

**A4. Retries and error workflows**

| Capability | n8n / Nod | Atlas Crew | What to add |
|---|---|---|---|
| Retry on fail (step) | n8n per node; Nod: model requests only (3× backoff), never tools or turns | Task fails; "Retry with reasons" on refusals | **Partial.** Add a per-automation retry policy for *infrastructure* failures (engine auth, rate limit, network) with backoff. Keep Nod's rule: **never auto-repeat an outside action**; mark it uncertain |
| Error workflow | n8n Error Trigger with execution payload | `crewmate` automation `on: failed` | **Have** (equivalent). Add a crew-wide error handler and include the failure payload (task id, error, last tool) in `{{event}}` |
| Failure notice for schedules | Nod: a failed run messages the owner, naming the schedule, with a Run now button | Push or Telegram on task finish [INFERENCE] | **Partial.** Make sure every unattended failure notifies with Run again |
| Rate-limit backoff | Nod: `Retry-After`, doubling up to 30 minutes, background reads paused | GitHub poll back-off on an unwritable seen file | **Partial** |
| Durable runs | n8n durable scheduler; Wait offloads to DB; Nod turns in Postgres | "What survives a restart" section exists (not reviewed in depth) | **Verify** (see Unknowns) |

**A5. Evaluation**

| Capability | n8n / Nod | Atlas Crew | What to add |
|---|---|---|---|
| Dataset evals | n8n: dataset → run → metrics (correctness, helpfulness, similarity, categorization, tools used, custom) | - | **Missing.** "Crewmate evals": a dataset of task prompts plus expected checks (code checks on the repo or diff, plus an LLM judge), run against a crewmate config (engine, model, role, skills); compare runs |
| Full-system scenario evals | Nod: isolated instance per attempt, simulated user, stateful fake apps, pass@k, pass^k, Fisher/Holm regression gate | Guards are runtime checks, not evals | **Missing.** Mirror Nod's design: fixtures for GitHub (fake repo, issues, CI logs), a scripted owner for approvals, and checks on outcomes and safety (no unapproved push, no weakened test) |
| Safety evals | Nod: prompt-injection cases (inbox, Slack, Drive), WhisperBench | Untrusted-content block, author filter | **Missing.** Add injection fixtures: an issue body, a PR comment or a CI log that tells the agent to exfiltrate secrets or merge. Measure 0/N |
| Published results | Nod benchmarks page with limits | - | **Missing** (docs, see D) |

### B. Nod capabilities (agent computing for one person)

| # | Capability | Atlas Crew | Needed for parity |
|---|---|---|---|
| B1 | **Interactive browser for the agent** (persistent profiles, accessibility snapshots, click and fill, downloads) | Captures and locate (screenshots in Playwright) only | A browser tool per crewmate session in a separate sandbox (the per-crewmate Linux user plus proxy already exists; finding N4 says the capture browser has no OS sandbox and runs as the crew user, so fix that first). Persistent profile per crewmate; public addresses only |
| B2 | **Browser handoff** (live view, human takes over for CAPTCHA or 2FA, returns) | - | Stream frames to Desktop and Mobile with input relay; pause frames during credential entry |
| B3 | **Vault** (logins, TOTP, cards, 1Password import, origin-bound private entry, masked output) | `secrets.json` (env vars) | A vault in hostd. The server enters values into verified fields, the model sees only names, and each use is claimed once |
| B4 | **Payments** with per-payment approval | - | Only if Crew targets errands; probably out of scope |
| B5 | **Proactivity and nudges** (observer reads sources, prepares drafts privately, messages once per matter, quiet hours, snooze) | Automations start tasks; no observer | A read-only "observer" crewmate mode: change tokens on sources (GitHub notifications, CI, issues, email), a cheap triage model, saved "matters" with evidence, once-per-matter delivery, quiet hours, snooze and dismiss, and an allowance |
| B6 | **Structured memory with provenance** (areas, quotes from owner messages, statuses, UI to view, edit, forget, export, import; hourly maintenance; poisoning-resistant) | One 64 KiB markdown file that the agent edits | Owner-visible memory entries with source links. Only owner words can create "stated" facts; agent-written notes are marked inferred or untrusted. Desktop and Mobile memory screens |
| B7 | **Learned skills** (procedures saved after a task, patched when wrong, owner-editable, secrets stripped) | none (engines have their own skills) | Per-crewmate skill files that hostd owns, PII-stripped, untrusted when re-read, and editable in Desktop. n8n Agents also have "skills" |
| B8 | **Coordinator plus parallel task sessions** in one conversation; reply-to-task routing; subagents | One task at a time per crewmate; parallelism across crewmates | Optional parallel tasks per crewmate (each in its own worktree), or a "captain" coordinator that routes to crewmates. A reply to a task's message continues that task |
| B9 | **One conversation across all surfaces** | Desktop, Mobile and Telegram show the crewmate's chat | **Have** [INFERENCE from crew.md] |
| B10 | **Spending ledger and limits** | Never calls a model API; engines run on subscriptions | Collect usage from engine reports (Claude Code and Codex print cost and tokens; omp reports usage). Per-task cost, monthly cap per crewmate or crew, and a check-in after $X with a pause |
| B11 | **Model choice per kind of work** (chat, schedules, background; light model) | Engine, model and provider per crewmate | **Have** per crewmate; add per-automation model override |
| B12 | **Watch a page or source, alert only on change** | Interval plus prompt (the model runs every time) | Code-level pre-check (hash, ETag, GitHub event cursor) that skips the agent run when nothing changed, as in Nod "watches skip unchanged runs" and Hermes cron pre-checks |
| B13 | **WhatsApp channel** | Telegram only | WhatsApp Business Cloud API (official) rather than Baileys |
| B14 | **Self-docs tool** (`nod_docs`) | - | Ship Atlas docs into the crew MCP as a `atlas_docs` search tool so crewmates answer questions about Atlas from the docs |
| B15 | **Packaged "own computer" for desktop users** (VM inside the app) | Requires a Linux server and SSH | Optional: "local crew server" in a VM on the user's Mac (Lima), like Nod. This lowers the entry cost enormously |
| B16 | **Bug report with reviewable diagnostics** | Audit plus logs | A "Report bug" action that bundles redacted hostd logs, task and audit, with a preview before sending |

### C. Core and runtime work

1. **Outside-action broker in hostd.** Today the agent engines call tools directly; hostd sees calls only through hooks (claude, omp) or not at all (codex, opencode run unattended). To get Nod-grade guarantees, every *outside* action (app API, MCP tool, HTTP, payment, browser submit) must go through hostd-owned tools such as `atlas.apps.call`, `atlas.mcp.<server>.<tool>` and `atlas.http`, where hostd:
   - holds the token
   - runs the checker
   - claims once-only execution
   - wraps the result as untrusted
   - writes the audit entry with the quoted authority

   This extends the existing GitHub credential-helper pattern (`docs/crew.md` §GitHub App). [INFERENCE] omp (oh-my-pi) comes from the same Pi runtime Nod runs on (`@earendil-works/pi-coding-agent`). That makes an omp extension a natural place to add Nod-style turn guards and untrusted-result wrapping for omp crewmates.
2. **Untrusted-content wrapping for tool results**, not just `{{event}}`. With third-party engines Crew can't wrap their built-in web or fetch results. It can wrap everything that comes through crew MCP or broker tools, and say so in the docs.
3. **Action checker service**: model-based with the server verifying the quote. It runs on the owner's chosen engine or a small decision model. Fail closed: in unattended runs, if it can't decide, the action doesn't run and the owner isn't asked (Nod rule).
4. **Usage ledger**: one table with (crewmate, task, turn, engine, model, tokens, cost or unknown), deduplicated by usage id. Budgets checked before starting a turn.
5. **Idempotency and uncertain outcomes**: claim rows for outside actions; "May have run" surfaced to the owner; never auto-retry.
6. **Durable job model**: Nod uses Postgres plus pg-boss (turns and drain jobs in one transaction); n8n has a durable scheduler with misfire policy and a reaper. Crew stores state in files. Check that queued tasks, waiting cards and scheduled fires are crash-safe and claimed once, and add a misfire policy.
7. **Observer runtime** (B5): change-token readers per source, a batch triage step, a saved "matter" store, a delivery policy (once per matter, quiet hours, snooze).
8. **Memory service** (B6): a structured store outside the agent's tree. Only owner-message-derived facts become stated. Compare-and-swap writes. Index injected per turn.
9. **Browser sandbox**: a per-crewmate browser running as the crewmate's user or a dedicated capture user, with its own OS sandbox (fixes crew.md N4), persistent profile and handoff stream.
10. **MCP client hub in hostd**: OAuth connect, token vault, per-crewmate enablement, deferred tool lists, a schema sanitizer, and exposure to every engine through the existing crew MCP rendering (`crew/engine/launch.rs`).
11. **Expose crewmates as MCP/API endpoints** (A1): synchronous "message crewmate, await structured reply", with session reuse.
12. **Eval runner** (A5): an isolated hostd per attempt (the real-systemd e2e test exists), fake GitHub and app fixtures, a scripted owner, checks plus a judge, pass@k, and a regression gate in CI for crew changes.

### D. Docs Atlas Crew would need (Nod's doc set as the benchmark)

- Split the 1,093-line `docs/crew.md` into a section tree like Nod's:
  - Get started
  - Use (chat, tasks, automations, approvals, memory, spending)
  - Connect (engines, apps/MCP, GitHub, Telegram)
  - How it works (runtime, isolation, approvals, memory, observer)
  - Trust (principles, security model and limits, security review guide, untrusted content, compare)
  - Deploy (install, update, backup, troubleshooting)
  - Contribute (apps, evals)
- Add `llms.txt` and `llms-full.txt` for the docs site, and an `atlas_docs` tool in the crew MCP.
- Add a **Trust principles** page (what the server enforces vs what the model decides) and a **Compare** page (Crew vs Nod vs n8n Agents vs Claude/Codex cloud agents) that is honest about where others are ahead.
- Add a **Benchmarks/evals** page once A5 exists: method, numbers, limits.
- Add a **Glossary** (crew, crew server, crewmate, task, automation, mode, rail, grant, guard, card, matter …).
- Add "what this doesn't cover" boxes on each control. crew.md already does this well for isolation; extend it to approvals, guards and automations.
- Add a "Build with n8n" page if A2's n8n app lands.

### E. Suggested order, smallest useful first [INFERENCE]
1. **n8n MCP app plus generic "Connect MCP" with OAuth and a token broker.** This reuses the crew MCP renderer and the GitHub broker pattern and opens the whole MCP ecosystem.
2. **Usage ledger plus budgets plus check-ins.** Engines already report cost.
3. **Watch pre-checks**: no agent run when the source is unchanged.
4. **Action checker with quoted authority for outside actions**, owner rules, and the uncertain state.
5. **Structured memory with provenance**, plus Desktop and Mobile memory screens.
6. **Eval runner** (GitHub-world fixtures, injection cases).
7. **Agent browser plus Vault plus handoff** (large; fix N4 first).
8. **Observer and nudges.**

---

## 9. Unknowns (and why)

- **Nod Terms of Use** (`https://n8n.io/legal/nod-terms-of-use/`): the page is a JS-rendered n8n site and plain fetch returned only CSS. I did not open it in a browser, so its terms (liability, data, age) are unverified.
- **Waitlist vs open download**: the `/docs/waitlist/` page still exists, but the landing page and repo offer direct downloads. [INFERENCE] The waitlist came before the 2026-11-10 public release and is now vestigial.
- **Composio catalog size inside Nod**: the docs say "Gmail, Google Calendar, Drive, Sheets, Docs, Slack and many other apps" but give no count. It depends on the connection-service operator's Composio auth configs.
- **The "Jev" decision model (TypeSafe)** and the "GPT-6.x / Claude Opus 5.5" defaults are as the docs state them. I did not test them.
- **Benchmark reproducibility**: Nod itself says the 226-case snapshot can't be reproduced from current `main`, and it publishes no full per-case manifests ([benchmarks](https://nod.n8n.io/docs/how-it-works/benchmarks/)). The 98% and 0/678 figures are self-reported and single-model.
- **Nod's real cost per task**: the blog admits early testers found costs higher than expected and says later releases reduced them. No figures are given.
- **n8n node counts** (about 91 triggers, 275 app nodes, 323 credential pages) are counts of doc pages in `llms.txt`, not of shipped node types. The real number of integrations is likely higher (community nodes, the HTTP Request node).
- **n8n Agents in queue mode**: the docs say they are not supported yet. Timing unknown.
- **Atlas Crew items marked Missing** are judged from `docs/crew.md` only; I did not read hostd code. I did not read "What survives a restart" or the misfire behaviour of crew schedules in detail, so A4 "Durable runs" is marked **Verify**.
- **Live product behaviour**: I did not install Nod (research only, and no heavy jobs without the maintainer's go-ahead). Everything here comes from docs, the repo, the blog and issues.
