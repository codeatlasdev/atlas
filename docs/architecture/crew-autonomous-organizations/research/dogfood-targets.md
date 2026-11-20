Raw evidence for docs/architecture/crew-autonomous-organizations/ (2026-11-20, main 72b47fc3).

The `/tmp/...` logs and clones cited below were local to the audit run and were not kept.

# Dogfood targets: Atlas, ac, zelvo, wi

Scope: static survey of the four repos, read-only. Nothing was edited, no service was started, no test was run, and `atlas dev` was not started or queried. Every claim cites the file it came from. [INFERENCE] marks conclusions I drew without running anything. Secret values were never read; only variable names from `.env.example` files were recorded. Test durations were not measured.

Paths: `atlas` is the main checkout; `ac`, `zelvo` and `wi` are its siblings (`../ac`, `../zelvo`, `../wi`).

---

## 1. Atlas (`atlas`)

**Product.** Developer platform: a Rust CLI and TUI that runs a project's dev services (`atlas dev`), an MCP server for agents, a Tauri desktop app (`apps/desktop`, Svelte), a Flutter phone app (`apps/mobile`), a website (`site/`), and "Atlas Crew": named AI agents on a Linux server through `atlas hostd` (`docs/crew.md`). Sources: `README` at root (not read), `AGENTS.md`, `docs/crew.md`.

**Stack and layout.** Cargo workspace in `crates/*` (`atlas-cli`, `atlas-tui`, `atlas-host`, `atlas-core`, `atlas-skills`, `atlas-term`, `atlas-desktop`, `atlas-setup`, `atlas-lsp`, `atlas-i18n`). Bun scripts in root `package.json`. Also `studio/` (Remotion/HyperFrames film studio), `services/push-relay` (Go), `i18n/` catalogs. Pinned toolchain in `rust-toolchain.toml` (`AGENTS.md`, "Which command checks what").

**atlas.yaml (services, ports, docker, pre_command).** `atlas.yaml`:
- No `pre_command` (`""`, the setup script is called by the commands that need it).
- Services: `lab` (port 13450, `scripts/desktop-lab.sh`, `autostart: false`, open key `l`), `desktop` (port 1420, `bun run desktop:dev`, real window on the user's screen, `autostart: false`), `site` (port 13451, Astro, `autostart: false`).
- No docker, no database, no infra block.
- Actions: gate `c` = `scripts/heavy.sh bun run check`; `u` changed-checks; `F` format; `d` desktop tests; `b` debug app build; `A` agent instance; benches `x`, `B`, `H`, `Z`, `K`, `S`; `O` mobile lab; `I` icons; `Y` release dry run.
- Ports block 13450-13459 (`atlas.yaml`, header comments).

**Gate.** `bun run check` → `scripts/check.ts` (`package.json`). Mapped by `scripts/checks/map.ts` (`docs/contributing.md`, "The gate"). Steps listed there: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo nextest run --workspace --locked`, doctests, `cargo deny`, skill check, workflow lint (actionlint, zizmor), desktop svelte-check and frontend tests, i18n check, mobile check (Flutter), push-relay check, `release:test`, `dup` (jscpd ≤1.5%), site check and build. `bun run check:changed` runs only what the diff touches (`docs/contributing.md`).

**Heavy work.** Cargo, the gate and benches must go through `atlas run --heavy -- <cmd>`, a machine-wide queue (`AGENTS.md` hard rules; `docs/contributing.md`, `scripts/heavy.sh` load gate, max `min(cores/2, GB/8)` jobs).

**Tests.** Unit and integration in Rust (`cargo nextest`, `atlas-cli --test mcp` needs python3). Desktop frontend tests via `bun test --conditions browser` in `apps/desktop`. Benches are opt-in (`--ignored`). Windows is checked on CI; locally only via cargo-xwin or zig cross-clippy (`docs/contributing.md`). Duration: not measured; CI has a 60 min job cap (`.github/workflows/ci.yml`).

**CI.** `.github/workflows/ci.yml`: plan job (`scripts/ci-plan.ts`) decides which areas run; Rust on macOS, Linux and Windows (60 min caps), desktop launch check on Linux under headless sway, arm64 CLI build, mobile, relay, docs. Also `release.yml`, `mobile-release.yml`, `closes.yml`, `crew-e2e.yml`, `lsp-real.yml`, `ci-rerun-lost.yml`, `pr.yml` (names from the directory listing; contents not read).

**Deploy path.** Releases: `bun run release alpha` from `main` only (`AGENTS.md`, "Releases"; `docs/release.md`). Website: `bun run site:deploy` (`scripts/deploy-site.sh`). Push relay: `bun run push:deploy`. Mobile: fastlane in `apps/mobile` (`mobile:beta`, `mobile:release`). Crew servers: `atlas crew install|update` over SSH (`docs/crew.md`).

**External systems and secrets (dev).** No database. Mobile lab pairs a phone (`bun run mobile:lab`). Desktop agent instance needs macOS (ScreenCaptureKit) or Linux (`sway`, `grim`, `dbus`) per `docs/contributing.md` table. Signing identity for debug builds via `scripts/dev-signing-identity.sh`. Release and push need GitHub credentials and the maintainer identity (`AGENTS.md`, "Commits"). Atlas Crew servers need SSH keys and the agents' own sign-ins (`docs/crew.md`, "Sign agents in").

**Language.** Team docs are English with a full pt-BR mirror under `docs/pt-BR/` (directory listing). i18n is enforced: user-visible text in `i18n/messages/<area>/` for en and pt-BR (`AGENTS.md`, hard rules). Chat with the maintainer is pt-BR (memory summary).

**Agent rules (already written).** `AGENTS.md` (under 8 KB, enforced by `scripts/agents-md.test.ts`): never merge, never push to `main`, never use the ruleset bypass, unless a maintainer runs the agent and says so for that change; branch + PR otherwise. Guards in `.claude/settings.json`, `.codex/hooks.json`, `.omp/extensions/agent-guard.ts`, and a pre-push hook refuse writes outside the worktree, pushes to `main`/`release/*`, force-push, merges (`bun run pr:merge`), `git stash`, branch/tag deletes. Skills in `.agents/skills/` (`atlas-workflow`, `real-tests`, `i18n`, `desktop-debug`, others). Tests-first rule for bugs. DCO sign-off and signed commits required (`CONTRIBUTING.md`). Merging is `bun run pr:merge <pr>`, maintainer-run only.

**Risk areas.** Release channel (alpha is "Latest"; the release line is a maintainer decision). Orphan-process killing (never `lsof` port-kill; see `AGENTS.md` hard rules). Container runtime restart (forbidden). Crew servers execute agents with shell access (approval modes, guards and protected paths in `docs/crew.md`, "Permission modes"). No production data store of its own.

**Readiness for an autonomous crew: HIGH for branch/PR/test work, BLOCKED for merge/release by design.**
- What the crew needs: a Mac or Linux box with Rust (pinned toolchain), bun, `atlas run --heavy` queue, `scripts/setup.sh` run once, a desktop agent instance for UI proof (macOS or Linux only; Windows has none).
- Services to boot: none required for the gate. `lab` only for UI work, through `atlas start_service lab` (MCP) or `bun run desktop:lab`, never the user's `desktop`.
- Data and seeds: none.
- Credentials: GitHub token with PR scope; DCO and signing config (`commit.gpgsign`); no maintainer bypass.
- Human approval: every merge (`pr:merge`), every release (`bun run release alpha`), every change to `desktop` (the user's own window), any action that touches the maintainer identity.
- "Done" = `bun run check` rc 0 (plus `check:changed` evidence), CHANGELOG line under `## [Unreleased]`, `bun scripts/changelog-lint.ts` clean, and a PR green in CI with the DCO check.
- Blockers today: (a) the gate is heavy and needs a warm Rust cache; a crew server must share `atlas run --heavy` with the maintainer's own jobs. (b) Local Windows verification needs zig/cargo-xwin setup (`docs/contributing.md`, Windows lint). (c) `atlas dev` is not running on this Mac right now (per the task), so the MCP tools have no live status here; I did not query it. (d) the crew server model assumes a Linux host with systemd; a crew working on Atlas itself needs a second machine or a Mac agent instance.

---

## 2. ac (Automação Curitiba) (`ac`)

**Product.** Operating system for Automação Curitiba, a flexographic label printer in Curitiba. Complements the ERP (Tiny) rather than replacing it. Scope: products and stock synced from ERP, price tables, production orders (knives, label papers), commercial analytics (ABC, RFM, CLV, churn, and more), commissions and goals, plus an online store (`ac/README.md`, `ac/AGENTS.md`, "O que é o AC").

**Stack and layout.** Bun + Turborepo monorepo (`ac/package.json`, workspaces `apps/*`, `packages/*`). API is Elysia + oRPC (`@orpc/server`), auth Better Auth, DB Drizzle over Postgres, queue BullMQ over Redis, React 19 + TanStack Router/Query/Start, Tailwind 4 + shadcn UI, Biome via Ultracite. Apps: `web`, `seller`, `server`, `worker`, `ai` (Mastra), `store` (Astro), `store-api` (Go, `air`), `studio` (Remotion), `operator` and `printer` (Flutter), `videowall`. Services in `services/ac-agent`, `services/ac-print`; Rust crates in `crates/` (`ac-rpc`, `print-bridge`, `zpl-compiler`). `packages/*` holds api, db, auth, ui, social-ui, egress, email, catalog, etc. (directory listing).

**atlas.yaml.** `ac/atlas.yaml`:
- `pre_command: bun install`; `env_files: [.env]`.
- Tunnel to prod DB: `enabled: false`, local 54320, `ssh_host: prod-3-codeatlas`.
- Services and ports: `database` 54321 (Postgres in `packages/infra/dev/compose.yml`, hide_logs), `redis` 63790 (critical), `server` 3070 (`bun run dev:server`, health `/health/live`), `web` 3071, `seller` 3072, `worker` (no port), `ai` 3075 (`lock_file: apps/ai/.mastra/dev.lock`), `store-api` 3073 (Go), `store` 3074, `studio` 3076 (dev only).
- Actions: `c` check; `d` db push; `u` DB refresh local (replaces local Postgres with a sanitized prod snapshot, `pause_services` server/worker/ai/store-api, `open_tunnel: true`, `confirm` message); `b` drizzle studio.
- `.env.example` names of note: `DATABASE_URL` (localhost 54321), `REDIS_URL` (63790), `BETTER_AUTH_SECRET`, `BETTER_AUTH_URL`, `CORS_ORIGIN`, `SOCIAL_CURSOR_SECRET`, `RELEASES_PUBLISH_TOKEN`, `AGENT_MINT_KEY`, `VAPID_KEYS`, `APP_ENCRYPTION_KEY`, `GOOGLE_CLIENT_ID/SECRET`, `ALLOWED_EMAIL_DOMAINS`, `TINY_CLIENT_ID/SECRET`, `TINY_LEGACY_TOKEN`, `KOMMO_*`, `CLICKUP_API_TOKEN`, `EMAIL_PROVIDERS`, `RESEND_API_KEY`, `AWS_SES_REGION`, `BEDROCK_*`, `S3_*`, `WI_OAUTH_CLIENT_ID/SECRET`, `OMNI_API_KEY`, `PRINT_SERVER_URL`, `PRINTER_API_KEY`, `OPERATOR_PROVISIONING_TOKEN`, `STORE_API_INTERNAL_KEY`, `INDEXNOW_KEY`, `CLOUDFLARE_API_TOKEN`, `ASAAS_WEBHOOK_TOKEN`, `VITE_*`. Source: `ac/.env.example` (names only).

**Gate.** `bun run check` = `ultracite check && turbo check-types && bun run dx:gate && bun run money:gate && bun run soft-delete:gate` (`ac/package.json`). Also `bun run ui:viewport-contract`, `bun run ui:audit-mobile` (manual). Lefthook at root (`lefthook.yml`).

**Tests.** `bun run test` → `scripts/test-ci.ts`: runs `bun test` once per workspace with a `bunfig.toml` preload, sequential, with 15 s per-test timeout. Parallel only for a list of workspaces. DB suites run against the local Postgres (`ac/AGENTS.md`, "Testes"). Playwright e2e exists under `apps/web` (`e2e` skipped by test-ci; not verified). Duration: not measured; `test-ci.ts` is serial, so it is slow by design [INFERENCE].

**CI.** `.github/workflows/ci.yml` is `workflow_dispatch` only: runs `bun run check` on the self-hosted `codeatlas` runner, then `deploy-tool` (go vet, race tests, linux build of `tools/deploy`), and `flutter-check`. Other workflows: `deploy.yml` (push to `main` only, self-hosted `deploy` runner), `rollback.yml`, `backup.yml`, `printer.yml`, `videowall.yml`, `ac-agent.yml`, `cast-bench.yml`, `publish-releases.yml`, notification workflows. No pull_request CI trigger exists in this repo (from the files listed) [INFERENCE: only the `ci.yml` header was read].

**Deploy path.** Production = push to `main` triggers `deploy.yml`, which runs `tools/deploy` (Go): plan, build changed services, ship artifacts over SSH to `prod-3-codeatlas`, activate with smoke and per-service rollback (`ac/.agents/skills/ac-production/SKILL.md`). Host layout `/opt/ac`, systemd units, Caddy, Docker Postgres and Redis (read-only for agents). Schema changes go through `schema-sync`, not migrations; the deploy takes a `pg_dump` first (`ac/AGENTS.md`, "Mudanças de schema"; SKILL.md "Deploy").

**External systems and secrets.** Tiny ERP (OAuth), Kommo CRM, Asaas payments (`ASAAS_WEBHOOK_TOKEN`), ClickUp, WhatsApp, Resend and AWS SES e-mail, Bedrock and an AI router, S3, Cloudflare, Google OAuth, WI API (`WI_OAUTH_*`, a sister product in `codeatlasdev/wi`). Egress is wrapped by `packages/egress`. Local secret-bearing files exist in the repo working tree (`.env`, `.secrets`, `.auth/`, `.agents/local-credentials.json`). I did not open them. The admin Mac holds `AGENT_MINT_KEY` in Keychain (`AGENTS.md`, "Testes em produção por agentes").

**Language.** Team and UI are pt-BR; code, identifiers and technical comments in English (`AGENTS.md`, "Idioma"). Commit messages not checked.

**Agent rules (already written).** `ac/AGENTS.md` (421 lines) is the authority. Key hard limits (its "Limites de autorização"): no commit, push or merge to `main` without an explicit request in the conversation (push to `main` = production deploy); no deploy of any kind; no production writes (SSH is read-only; no `psql` writes, no `systemctl restart`, no `schema-sync --target production`); no `--accept-data-loss`, no `db:refresh-local`, no destructive SQL; no changes to external systems (Tiny, Kommo, ClickUp, Google Ads, WhatsApp, e-mail). Agents never run `bun run dev`, `atlas dev` or `dev:<app>`; they check with `bun run dev:status` (exit 3 = stop and ask the dev). Production test accounts: `bun run agent:login reader` (read-only profile; writes refused). Git shared across agents: never `git stash`, `reset`, `checkout --`, `restore`, `clean`; never `bun install` or touch `bun.lock` without a request. `$ac-*` skills (about 30, in `.agents/skills/`). Design system (`packages/ui`) is law; a lint guardrail (`lint:ui`) and `kpi-grid-contract.test.ts` enforce parts of it.

**Risk areas.** Production data (customers, orders, payments, Asaas); payments (Asaas, boletos, Pix, cards); money in integer cents (`money:gate`); soft deletes (`soft-delete:gate`); production DB via SSH tunnel (`ac atlas.yaml`, action `u` replaces local DB); `bun db:push` writes to a fixed local DB only (`localhost:54321`); deploy on every push to `main`.

**Readiness for an autonomous crew: LOW today, for reasons below.**
- What the crew needs: a checkout of `ac` on a machine with Bun 1.4.3, Docker (Postgres 54321, Redis 63790 from `packages/infra/dev/compose.yml`), Go 1.x (store-api), Flutter (operator, printer), SSH to `prod-3-codeatlas` read-only only, the `.env` for local services (a local copy, never shared), and `gh` with `codeatlasdev` access.
- Services it must boot: `database`, `redis`, `server` (3070), `web` (3071), `worker`; `seller`, `ai`, `store`, `store-api` only for their areas. `atlas dev` is forbidden to agents (they must ask the dev to start it: exit 3 means stop).
- Data and seeds: local DB is empty by default; `bun db:refresh-local` (sanitized prod snapshot) is forbidden to agents and needs a dev's explicit confirm. A seed script exists (`scripts/seed-production.sql` is at repo root, content not read) [INFERENCE: it is not the same as a refresh].
- Credentials: none for the crew by default. Test-account login only through `agent:login reader`; mint key from the admin Mac only.
- Human approval: every commit (per request), push to any branch with `git branch --show-current` check, every merge, every deploy and every rollback, any `schema-sync` that loses data (exit 2), every production read beyond `SELECT`, every external integration write.
- "Done" = `bun run check` clean, `bun run test` clean for the touched workspace, `releases/<date>-<slug>.md` for user-visible change (`bun run releases:check`), i18n pt-BR text, design-system components used, and a visual check in desktop and mobile (per `ac/AGENTS.md`). E2E (`apps/web`) for flows, with a note if not run.
- Blockers today: (a) agents are forbidden to start the dev stack; a crew cannot run `server` or `web` unless a human starts `bun run dev` (exit 3 otherwise). This is the largest block. (b) Tests need local Postgres and Redis, which are created by `packages/infra` only through the dev stack. (c) Every deploy is a push to `main`, and the policy forbids pushing to `main` without a request in the conversation: an autonomous push to `main` is refused by design. (d) Dependencies: `bun install` forbidden without a request. (e) The `db:push` path is fine locally, but there is no migration history (schema-sync diffs only). (f) The test runner is serial, so a full `bun run test` is long [INFERENCE].

---

## 3. zelvo (`zelvo`)

**Product.** CRM for external salespeople in Brazil: find local companies on a map (scraped Google Maps via `gosom/google-maps-scraper`), qualify, cadence, plan visit routes, record results. Invite-only (waitlist). Mobile-first. Pt-BR first, with en and es catalogs (`zelvo/README.md`, `zelvo/PLANNING.md`).

**Stack and layout.** Bun + Turborepo (`zelvo/package.json`). Web: SvelteKit 2 with Svelte 5 runes and adapter-node, TanStack Svelte Query, `@codeatlasdev/ui` (no Tailwind), Mapbox via `@codeatlasdev/ui/map` and deck.gl. API: Elysia + oRPC at `/rpc`, Better Auth (email/password, cookie, organization plugin). Worker: BullMQ on Redis (queues `search` and `email`), with a Docker container per search. DB: Drizzle + Postgres 18 (prod on the server's own Postgres container; dev on Neon). Email: React Email + Resend. Anti-bot: Cloudflare Turnstile. Lint: Biome. Layout: `apps/web`, `apps/server`, `apps/worker`, `packages/{api,auth,db,queue,email,env,i18n,config,receita}`, `deploy/` (`deploy.sh`, nginx, systemd, remote scripts), `docs/` (`arquitetura.md`, `backend.md`, `frontend.md`, `operacao.md`, `deploy.md`, `i18n.md`, `emails.md`).

**atlas.yaml.** `zelvo/atlas.yaml`:
- `infra.compose_file: docker-compose.yml` (only `redis` is defined there).
- Services: `redis` 36550 (critical, hide_logs), `vroom` 36556 (a `docker run` of `ghcr.io/vroom-project/vroom-docker:v1.14.0`, route planner; optional, the app falls back to its own solver without `VROOM_URL`), `server` 36551 (`bun run --hot src/index.ts`, critical, depends on redis, open key `a`), `web` 36552 (`vite dev`, depends on server, open key `w`), `worker` (depends on redis; no port).
- No `pre_command`.
- Actions: `c` check; `y` types; `u` tests; `d` DB push (pauses server, worker); `x` DB generate migration; `b` DB studio; `e` DB seed (pauses server, worker).
- Env: one `.env` at root (`.env.example`). Names: `DATABASE_URL` (Neon in dev), `BETTER_AUTH_SECRET`, `BETTER_AUTH_URL`, `CORS_ORIGIN`, `VITE_SERVER_URL`, `VITE_MAPBOX_TOKEN`, `REDIS_URL`, `PORT`, `NODE_ENV`; production adds `TURNSTILE_SECRET_KEY`, `RESEND_API_KEY`, `VITE_TURNSTILE_SITE_KEY`, `RECEITA_DATABASE_URL`, `ASAAS_*`, `AI_*`, `PLATFORM_ADMIN_EMAILS`, `SIGNUP_MODE` (from `docs/deploy.md` and `docs/operacao.md` references).

**Gate.** `bun run check` = `biome check --write . && lint:ui && lint:i18n && i18n:check` (rewrites files). Read-only gate in CI: `bun run lint`. Plus `bun run check-types` and `bun run test` (both listed in the AGENTS "Antes de commitar" line). Docs require updating docs in the same commit when behavior changes.

**Tests.** `bun run test` runs unit suites in `packages/{ai,api,auth,queue,db}/src`, `apps/{worker,server}/src`, `packages/{i18n,env,receita}/src`, `apps/web/src/{lib,routes}` and `packages/email/src`, all with `bun test`. Web e2e: Playwright in `apps/web/e2e` (`bun run test:e2e`), needs `atlas dev` running and QA seed (`db:seed:qa`), per `docs/deploy.md` (smoke). Not part of `bun run test`. Duration: not measured.

**CI.** `.github/workflows/ci.yml`: one job on `ubuntu-latest` with Postgres 18 and Redis 7 service containers (`zelvo_test` database, the name the dev guard accepts). Steps: `check-types`, `biome check`, `lint:ui`, drizzle migrate, `bun run test`. A `build` job depends on it (`bun run build`). E2E is not in CI (not in the file read).

**Deploy path.** `bun run deploy` (`deploy/deploy.sh release`): refuses unless on branch `main`, clean tree, HEAD on origin; asks for `sim`; `git archive` to the server; `deploy/remote/release.sh` installs deps, runs `drizzle-kit migrate`, pulls the scraper image, builds, swaps the `current` symlink atomically, restarts `zelvo-api`, `zelvo-worker`, `zelvo-web`, health-checks, auto-rolls back on failure (`docs/deploy.md` steps 1-8). Target server is an alias `awesome` in `~/.ssh/config` (shared server). Migrations expand-only, idempotent. Backups to S3 daily (`docs/deploy.md`, "Backups e restore"). Rollback: `bun run deploy:rollback`. Agents do not run `bun run deploy` without an explicit user request (`zelvo/AGENTS.md`, "Comandos").

**External systems and secrets.** Neon (dev Postgres), Mapbox (public token for the map), Resend (email), Cloudflare Turnstile, Redis, Google Maps scraping container, AWS S3 backups (profile `zelvo-backup`), Receita Federal CNPJ data (`packages/receita`, ~10 GB Postgres DB on the prod server), Asaas (billing, in `docs/operacao.md`), VROOM (local container), ElevenLabs (only for help-video narration; never on prod, per `docs/deploy.md`). Secrets live only on the server (`/opt/zelvo/.env.production`, `postgres.env`, `backup.env`) and in the local `.env` (gitignored).

**Language.** pt-BR is the source locale for product text; en and es catalogs exist (`docs/i18n.md`). Docs are in pt-BR. Code identifiers mixed. Commits in Conventional Commits.

**Agent rules (already written).** `zelvo/AGENTS.md`: stack, commands, padrões (router thin, service thick, `can({...})` permission, `orgId` never from input, leads through `leadScopeCondition`), and "antes de commitar" checks. `docs/deploy.md` "Nunca faça" section and the agent rule in `AGENTS.md` (no `bun run deploy` without an explicit user request). Design system via `@codeatlasdev/ui` with `codeatlas-ui` and `codeatlas-dx` skills in `.agents/skills/`. Tests first for bugs (`AGENTS.md`, "Teste primeiro"). No `.claude/`-style guard files were found at the root listing [INFERENCE: not verified].

**Risk areas.** Production DB (Postgres 18 on a shared server, "Nunca faça" in `docs/deploy.md`); migrations that can lose data (expand-contract rule); `db:push` against Neon dev (`db:reset` deletes the dev DB); the Receita CNPJ data; billing (Asaas) and the webhook; Mapbox token exposed by design (`VITE_` is public); auth secret rotation drops all sessions; lead data (personal data of companies' contacts: PF/PJ). Shared server runs other apps (`deploy/remote/setup.sh:3`), so bad deploys affect neighbours.

**Readiness for an autonomous crew: MEDIUM. Safe for code and tests on dev with Neon; blocked for deploy and prod-touching work.**
- What the crew needs: Bun 1.3.13, Docker (Redis 36550, VROOM 36556 optional, scraper image for search), a `.env` with a Neon dev `DATABASE_URL` (a credential the crew would hold: Neon dev is shared with humans), Mapbox dev token, a Resend key or the dev preview (`email:dev`, port 36560), Turnstile test keys.
- Services it must boot for web work: `redis`, `server` 36551, `web` 36552, `worker`. For UI proof: Playwright with `atlas dev` running (`docs/deploy.md`).
- Data and seeds: `bun run db:seed` (demo) and `bun run db:seed:qa` (the QA company that e2e specs expect). `db:reset -- --host <host>` wipes the dev DB; restricted to Neon dev hosts by the dev guard.
- Credentials: Neon dev URL, Mapbox public token, Resend key (only if email is tested), Turnstile test keys. None of production.
- Human approval: `bun run deploy` (always, needs `sim`), any `db:push` or `db:generate` change accepted for production, rollback, backup restore, DNS/Cloudflare, SSH to the shared server, any e-mail to real people, any Asaas or CNPJ-API change, `db:reset` on any host.
- "Done" = `bun run check-types`, `bun run test`, `bun run check` clean; doc updated in the same commit if behavior/env/port changed (the repo rule); e2e run for screen or auth changes (with `atlas dev` up); migration reviewed and idempotent; visual check at 390 px and desktop (`AGENTS.md`).
- Blockers today: (a) the dev DB is a shared Neon instance, not a local container; a crew run writes to a database humans use unless it gets its own Neon branch or a local Postgres. (b) Local Postgres is not in `docker-compose.yml` (only Redis is); the CI service container is the only local-Postgres recipe, so a crew would need to reproduce it. (c) `atlas dev` gives 36551-36552 and the worker, but e2e needs a running stack and a QA seed; the crew must be told which of those is its own. (d) The Neon URL and Mapbox token are in `.env`; a crew needs a dedicated key set. (e) Integration with CRMs (Kommo, Pipedrive, HubSpot) is mock only (`AGENTS.md`, "Fora do escopo funcional"); a task touching them is not testable against a real system.

---

## 4. wi (`wi`)

**Product.** wi (in code: "wa-api"): a WhatsApp API platform. Companies send and receive WhatsApp messages over HTTP with zero message loss and per-session isolation. Three layers: API master (Elysia/Bun, REST, auth, webhooks), engines (Go, whatsmeow, one container per WhatsApp session), and a NATS JetStream broker in between. Also: MCP server for AI clients, voice calls (voice-worker, voice-agent), billing via Asaas prepaid (`wi/README.md`, `wi/AGENTS.md`, `wi/.agents/AGENTS.md`, "Billing").

**Stack and layout.** Bun workspaces + Turborepo (`wi/package.json`) and Go workspace in `engines/` (`go.work`, vendored in `engines/vendor`). Apps: `apps/server` (Elysia), `apps/web` (SvelteKit, `@codeatlasdev/ui`), `apps/docs` (SvelteKit static), `apps/voice-agent`. Packages: `db` (Drizzle + Postgres), `api`, `auth` (Better Auth), `mcp`, `orchestrator` (Docker lifecycle), `events`, `speech`, `brand`, `config`, `email`. Engines: `whatsmeow`, `voice-worker`, `sip-gateway`, `shared`. Deploy: `tools/deploy` (Go). SDKs in `sdks/` (node, go, php, rust).

**atlas.yaml.** `wi/atlas.yaml`:
- `infra.compose_file: docker-compose.dev.yml` (Postgres and NATS in Docker, per `wi/package.json` `dev:infra`).
- Services: `postgres` 7232 (critical), `nats` 7222 (critical, health on 7282), `engine` 7200 (`cd engines/whatsmeow && air`, Go hot-reload), `voice-worker` 7210 (air; env `HEALTH_PORT`, `HTTP_PORT` 7211, `NATS_URL`), `server` 7230 (`bun --watch`, env with `CORS_ORIGIN`, `BETTER_AUTH_URL`, `WEB_URL`, `VOICE_WORKER_URL`, and a literal `VOICE_AGENT_TOKEN: dev-voice-agent-token`), `voice-agent` 7240 (`bun --watch`), `web` 7231 (`bun run dev`, open key `w`), `docs` 7221 (open key `o`).
- Actions: `c` check; `y` type check; `e` tests (`bun run test:affected`); `d` DB push (pauses server); `b` DB studio.
- Env names (`wi/.env.example`): `PORT`, `API_KEY` (dev value `dev-api-key-12345`), `ENGINE_URL`, `NATS_URL`, `VOICE_WORKER_URL`, `DATABASE_URL` (dev `waapi:waapi`), `API_PUBLIC_URL`, `NODE_ENV`, `WEBHOOK_ALLOW_PRIVATE` (dev `1`). Production (`tools/deploy/AGENTS.md`, "Secrets"): `RESEND_API_KEY`, `EMAIL_FROM`, `WEB_URL`, `PLATFORM_ADMIN_ORGS`, `PLATFORM_ADMIN_EMAILS`, `CREDENTIALS_ENC_KEY`, `ASAAS_ENV`, `ASAAS_API_KEY`, `ASAAS_WEBHOOK_TOKEN`, `VOICE_AGENT_TOKEN`, `PUBLIC_IP`, `POSTGRES_PASSWORD`, `ENGINE_TOKEN_SECRET`, `NATS_PASSWORD_*`.

**Gate.** `bun run check` = `bun --bun biome check . && bun --bun turbo check-types`. Go vet via each package's `check-types`. Per `CONTRIBUTING.md`: `check-types`, `test`, and the issue's acceptance checks; `test:integration` for DB suites, `test:engine` (Go with -race). Pre-push hook: Biome on pushed files plus `check-types` and `test` with `--affected` (`lefthook.yml`, `CONTRIBUTING.md`).

**Tests.** `bun run test` via turbo, which runs the Go engines too. Real-Postgres suites skip unless `INTEGRATION=1`; `bun run test:integration` (`scripts/test-integration.ts`) needs `docker compose -f docker-compose.test.yml up -d` (Postgres 17232, NATS 17222). `bun run test:e2e` (Playwright, `apps/web`) against a stub API. `bun run test:live` needs a paired real session (ENGINE_URL, API_URL, API_KEY, TEST_PHONE). `test:engine:live` needs a real paired WhatsApp session. Duration: not measured.

**CI.** `.github/workflows/ci.yml` (on push to `main` and PRs to `main`): job `check` (Biome, `schema:verify` for the Better Auth schema drift, type check), job `test` (Postgres 18.7 service, `bun run test`, `test:integration`, `test:engine`, Go tests for `engines/shared` and `tools/deploy`), job `docs` (Playwright Chromium install, `apps/docs` build, layout audit, docs contract tests). Playwright e2e for `apps/web` is not in CI [INFERENCE: not in the file read].

**Deploy path.** `bun run deploy` (Go CLI in `tools/deploy`, `deploy.toml` production environment with host alias `wi`). Pipeline: SSH check, deploy lock, preflight (git clean and pushed), infra (apply `tools/deploy/host/*` files), build, sync, install, migrate BEFORE the new server starts, swap server with auto-rollback, roll engines with `docker-rollout`, verify with HTTP health, broker auth enforce (`tools/deploy/AGENTS.md`). Migrations are expand-only; indexes on `messages` go through `CONCURRENT_INDEXES` (`wi/.agents/AGENTS.md`, "Deploy"). The production Asaas account is shared with other products (`wi/.agents/AGENTS.md`, Billing: "never create charges on the production account from scripts or tests").

**External systems and secrets.** WhatsApp (via whatsmeow: unofficial protocol; risk of account bans is a product risk, not in repo docs [INFERENCE]), Asaas payments (prod key single-quoted in `.env`), Resend (waitlist and billing e-mails), ElevenLabs/OpenAI (speech, per org keys sealed with `CREDENTIALS_ENC_KEY`), NATS, Postgres, a Cloudflare DNS and Let's Encrypt on the host (`tools/deploy/AGENTS.md`). The docs site and web use `@codeatlasdev/ui`.

**Language.** Team and product docs in pt-BR in places (`PROJECT.md`, `.agents/AGENTS.md`, `CONTRIBUTING.md` resolution template) and English in code and the root `AGENTS.md`. Agent-facing text is English. User-facing product strings: pt-BR in the web app (inferred from docs) [INFERENCE].

**Agent rules (already written).** `wi/AGENTS.md`, mandatory issue workflow: claim with `in progress` label and assignee, comment the plan, branch `fix/N-slug` or `feat/N-slug`, Conventional Commits with `fixes #N`, verify (`check-types`, `test`, acceptance), report with the resolution template, close only after merge to `main`, `blocked` label with exact cause. `CONTRIBUTING.md` "Rules for AI agents": read the issue with comments, no silent scope shrink, never invent user-facing facts, never commit secrets, parallel agents own one file each, migrations generated once by the integrator. `.agents/AGENTS.md` lists steering files (`philosophy`, `whatsapp-api`, `execution`, `server`, `testing`, `design-system`, `dates`, `docs`, `compat`). `wi/AGENTS.md` says the agent must not use `--no-verify`.

**Risk areas.** Message integrity (zero loss invariant, idempotency, DLQ replay `dlq:replay`); WhatsApp session credentials and their encryption (`CREDENTIALS_ENC_KEY` rotation breaks stored keys); billing (prepaid, Asaas, lapsed orgs get 402); the shared Asaas production account; production deploy (single host, expand-only migrations, broker auth enforcement can lock engines out if misordered: rollback steps in `tools/deploy/AGENTS.md`); voice calls (real phone calls, a Telegram-style external effect); personal data of end users' messages.

**Readiness for an autonomous crew: MEDIUM-HIGH for issue work with a clear acceptance list; LOW for anything needing real WhatsApp or Asaas.**
- What the crew needs: Bun 1.4.4, Go 1.27 (per `CONTRIBUTING.md`), `air` (`go install github.com/air-verse/air@latest`), Docker, Playwright Chromium, `gh` with `codeatlasdev` access, `atlas` CLI.
- Services to boot: `postgres` (7232), `nats` (7222), `server` (7230), `web` (7231) for UI, `engine` (7200) only for WhatsApp-path work, `voice-worker`/`voice-agent` only for voice. For tests alone: `docker-compose.test.yml` (ports 17232, 17222), no dev stack.
- Data and seeds: migrations via `DATABASE_URL=... bun run db:migrate`. No seed script was found in the root listing [INFERENCE]. Test DB is disposable.
- Credentials: dev values in `.env.example` are safe. The `atlas.yaml` literal `VOICE_AGENT_TOKEN` is dev-only. No production keys. Real WhatsApp pairing needs a real phone (human).
- Human approval: `bun run deploy` (production), any change to `tools/deploy/host/*` (infra), broker auth enforcement, any message sent to a real number, any Asaas charge or webhook change, DNS, merge and close of issues that claim "deployed".
- "Done" = `check-types`, `test`, `test:integration` for DB work, `test:engine` (-race) for engine work, the issue's acceptance items each with evidence, resolution comment in the template. For a deployed fix: `bun run deploy` by a human, then the SHA on the issue.
- Blockers today: (a) no e2e against a real WhatsApp session in the crew's reach (needs a paired phone); `test:live` and `test:engine:live` need `TEST_PHONE`. (b) `atlas dev` needs Go and `air`; without them the engine cannot start. (c) The production deploy tool is not the crew's to run; a crew can prepare, not ship. (d) The Asaas sandbox is documented as returning 400 on Pix QR creation (`wi/.agents/AGENTS.md`, Billing), so billing tests are partly blocked by the vendor. (e) WhatsApp account risk means test sessions must not run on a production number [INFERENCE].

---

## 5. Cross-repo table

| | Atlas | ac | zelvo | wi |
|---|---|---|---|---|
| Stack | Rust workspace, Tauri+Svelte, Flutter, Astro, Go (relay) | Bun, Turborepo, Elysia+oRPC, React 19 + TanStack, Drizzle, BullMQ, Flutter, Go, Rust | Bun, Turborepo, SvelteKit 2 + Svelte 5, Elysia+oRPC, Drizzle, BullMQ | Bun, Turborepo, Elysia, SvelteKit, Drizzle, Go engines (whatsmeow), NATS |
| Dev services (atlas.yaml) | `lab` 13450, `desktop` 1420 (user's screen), `site` 13451 | `database` 54321, `redis` 63790, `server` 3070, `web` 3071, `seller` 3072, `ai` 3075, `store-api` 3073, `store` 3074, `studio` 3076, `worker` | `redis` 36550, `vroom` 36556, `server` 36551, `web` 36552, `worker` | `postgres` 7232, `nats` 7222, `engine` 7200, `voice-worker` 7210, `server` 7230, `voice-agent` 7240, `web` 7231, `docs` 7221 |
| Docker deps | none | Postgres, Redis (`packages/infra/dev/compose.yml`) | Redis (`docker-compose.yml`), VROOM (`docker run`), scraper container | Postgres, NATS (`docker-compose.dev.yml`), engine containers |
| pre_command | none | `bun install` | none | none |
| Gate | `bun run check` (heavy, Rust, 3 OS in CI) | `bun run check` (ultracite + turbo types + 3 gates); `bun run test` serial per workspace | `bun run check-types && bun run test && bun run check` | `bun run check` + `test` + acceptance on issue; `test:integration`, `test:engine` |
| CI trigger | push and PR (`ci.yml`) | `ci.yml` workflow_dispatch only | push main, PR main | push main, PR main |
| Deploy | `bun run release alpha` (maintainer), `site:deploy`, relay deploy, crew install/update | push to `main` → `deploy.yml` (tools/deploy, per-service, rollback) | `bun run deploy` (needs `sim`), shared server `awesome` | `bun run deploy` (Go CLI, host `wi`), expand-only migrations |
| Secrets | GitHub, maintainer identity, signing, crew SSH keys | Many (Tiny, Kommo, Asaas, ClickUp, WI, S3, SES, Bedrock, Cloudflare); local `.env`, `.secrets`, `.auth/` | Neon dev URL, Mapbox, Resend, Turnstile, Receita DB, S3 backup, Asaas, AI router; prod only on server | WhatsApp engine tokens, Asaas (shared prod account), Resend, speech keys, NATS creds; prod only on host |
| Risk | Release channel; orphan kills; merge policy | Production data, payments, money, Tiny/Kommo writes; deploy on push | Shared prod Postgres; CNPJ data; billing; neighbours on shared server | Message integrity, WhatsApp accounts, Asaas shared account, broker auth lockout |
| Team language | pt-BR chat; EN docs with pt-BR mirror; i18n enforced | pt-BR product and UI; EN code | pt-BR product and docs; en/es catalogs | Mixed pt-BR and EN docs; EN agent rules |
| Agent rules | `AGENTS.md` + guards + skills; never merge/push main | `AGENTS.md` 421 lines; no push to main without request; no deploy; no prod writes; `$ac-*` skills | `AGENTS.md`; deploy only on explicit user request; `codeatlas-*` skills | `AGENTS.md` issue lifecycle; `.agents/AGENTS.md`; steering files |
| Readiness for autonomous crew | HIGH for code+PR, merge/release blocked by design | LOW: dev stack is human-started; no autonomous push to main | MEDIUM: dev DB is shared Neon; deploy human-only | MEDIUM-HIGH for issue work; LOW for WhatsApp/Asaas-touching work |

---

## 6. Common requirements for Atlas Crew

Each item is needed by at least two of the four repos. Status is what I observed, not what I tried.

1. **Repo-scoped working copy per crewmate, with a clean and pushed base.** All four repos assume a git checkout and forbid destructive git (`ac`: no stash, reset, checkout --, restore, clean; `atlas`: guards refuse stash). Crew work must run in its own worktree or clone; a shared worktree breaks ac's rule.
2. **Branch + PR only; merge and deploy are human steps.** Atlas (merge = `pr:merge`, maintainer-run), ac (push to main = deploy), zelvo (`bun run deploy` only on request), wi (deploy is the Go CLI). The crew must treat "merged" and "deployed" as approval gates, not as done. Current state: no approval gate exists in Atlas Crew for PR merge; the crew can open PRs.
3. **A gate command and an explicit "done" definition per repo.** Use the repo's own gate, read the exit code, and record evidence on the PR or issue. Already in each `AGENTS.md`.
4. **Dev stack lifecycle owned by a human (or by an explicit start).** ac forbids agents from starting `bun run dev`; Atlas uses `atlas dev` with MCP tools for status and restart; zelvo and wi use `atlas dev` too. A crew needs: start a named service set on request, read status, read logs, stop. The MCP `atlas_*` tools already cover status, logs, restart, start, stop, run_action, wait_ready (`mcp__atlas_*` routes). Gap: Atlas Crew does not yet run `atlas dev` on the crew server, and ac's rule forbids agents from starting it at all.
5. **Heavy-job queue shared across crewmates and the human.** Atlas enforces `atlas run --heavy` (machine-wide queue). Required wherever a build or the full gate runs.
6. **Per-crewmate secrets with a dedicated dev credential set.** zelvo's dev DB is shared Neon; ac's `.env` holds real third-party keys; wi's dev values are literal. The crew must have its own keys (or a Neon branch), never the maintainer's, and never production keys. Atlas Crew stores per-crewmate `secrets.json` (`docs/crew.md`, "Security model"); the export never carries it by default, which fits.
7. **Read-only production access.** ac documents SSH read-only, `SELECT`-only SQL, and read-only deploy status; zelvo and wi document the same for logs. The crew needs a read-only profile on the host (no `systemctl restart`, no `psql` writes). Atlas Crew's approval model (`ask`, `edits`, `bypass`) and protected paths (`docs/crew.md`) do not block network or SSH commands by themselves: a `bypass` crewmate with an SSH key can write to production. Required: a deny rule for the production hosts (`prod-3-codeatlas`, `awesome`, `wi`) for crewmates.
8. **Human approval for external effects.** Tiny, Kommo, ClickUp, Asaas, WhatsApp, e-mail (ac and wi), and real phone calls (wi voice). Atlas Crew's approval prompts cover tool calls, not the content of an external API call made by a script. Required: a denylist of commands that reach these systems, or a one-shot approval for each run.
9. **Destructive DB actions are human-only.** ac's `db:refresh-local` and `db:push` with data loss (exit 2); zelvo's `db:reset`; wi's migrations that are not expand-only. The crew needs a deny rule on these actions in every repo (they exist as atlas.yaml actions and as `package.json` scripts).
10. **Dependency installs need a request.** ac forbids `bun install` and `bun.lock` changes without a request; wi and atlas pin toolchains. The crew needs a rule: install from the lockfile only (`bun install --frozen-lockfile`), no lockfile edits.
11. **Language per output.** pt-BR for product text and team-facing reports (ac, zelvo, Atlas chat); EN for code. The crew writes user-visible text through each repo's i18n path (atlas `bun run i18n:check`, zelvo `lint:i18n`, ac pt-BR rules), and the report language follows the maintainer's request.
12. **Evidence in the artifact.** Each repo asks for proof (CHANGELOG line, release note file, resolution comment with SHA, screenshot for UI). A crew run must attach the gate output and UI captures; Atlas already has `ui` tasks with proof (`docs/crew.md`, "UI tasks and proof"). The same proof model should apply to ac and zelvo UI changes.
13. **Machine capabilities per repo.** Atlas needs a macOS or Linux agent instance for UI work; ac needs Flutter for operator and printer; wi needs Go and `air`; zelvo needs Docker for VROOM and the scraper. A crew server (Linux, systemd) covers the Go and Bun parts; UI and Flutter work need a different host. Required: a per-repo capability check that refuses a task early when its toolchain is missing.
14. **Stop conditions on the dev stack.** ac's `dev:status` exit 3 means "stop and ask". The crew must treat a down stack as a blocker to report, not a thing to fix by starting it.

## 7. Open questions for the maintainer

1. Should an Atlas Crew crewmate be allowed to start `atlas dev` in ac, zelvo or wi (on the crew host), or only the human starts it? ac's rule says never.
2. Which Neon branch or local Postgres should a zelvo crewmate use? The shared dev Neon is written by humans.
3. Is a crewmate allowed to run `bun run check` on the maintainer's Mac through `atlas run --heavy`, or only on a crew server?
4. For wi, is `bun run deploy` ever allowed for an agent with an explicit request in the conversation, or is it human-only by policy? `tools/deploy/AGENTS.md` says "the only way to deploy" but does not name who.
5. For ac, is a PR-only flow for `feat/*` or `fix/*` branches acceptable without a request in the conversation? The AGENTS rule says branch push is free but commit needs a request; this needs a single answer for the crew.
6. Atlas Desktop's `desktop` service runs on the user's screen. Is there a crew mode that uses only `lab` and the mirror, as `AGENTS.md` requires for agents?
7. The atlas.yaml of wi hard-codes `VOICE_AGENT_TOKEN: dev-voice-agent-token`. Is that acceptable for the crew's dev stack, or should it come from a crew-scoped secret?
8. Atlas's `ci.yml` runs on Windows, ac's CI is manual-only. Who decides when the crew's PRs trigger CI in ac?

## 8. Not verified in this pass (honest gaps)

- `atlas dev --status` / MCP `atlas_status` were not queried (the task said the stack is not running for atlas; I did not check ac, zelvo, wi stacks either).
- No test was run; no durations were measured.
- Secret-bearing local files (`.env`, `.secrets`, `.auth/`, `.agents/local-credentials.json`) were not opened on purpose.
- Only the first ~300 lines of several long docs were read (ac `AGENTS.md`, zelvo `docs/deploy.md`, wi `.agents/AGENTS.md`, atlas `docs/contributing.md`, `docs/crew.md`). Later sections may add rules.
- Not read: `ac` `packages/infra/dev/compose.yml`, `zelvo` `docker-compose.yml` contents, `wi` `docker-compose.dev.yml` contents, `ac/.agents/skills/ac-dev-tui`, `ac/.agents/skills/ac-database`, `zelvo/docs/operacao.md`, `wi/.agents/steering/*`, `atlas/.agents/skills/atlas-workflow`. Each would firm up the readiness rating.
- CI workflows of ac (`deploy.yml`, `rollback.yml`) and wi (`ci.yml` beyond the first 300 lines), and the zelvo `ci.yml` were read only at the top level.
