# Tech Stack — Atlas

Fonte de verdade para versões e convenções técnicas.

## Runtime & Tooling

| Tool | Version | Purpose |
|------|---------|---------|
| Bun | >=1.3 | Runtime, package manager, bundler |
| Turborepo | ^2.9 | Monorepo task orchestration |
| TypeScript | ^5.7 | Type system |
| Biome | ^1.9 | Lint + format (tabs, 100 chars) |
| lefthook | latest | Git hooks (pre-commit) |

## Backend (apps/panel)

| Lib | Version | Purpose |
|-----|---------|---------|
| Elysia | ^1.3 | HTTP framework |
| oRPC | ^1.13 | Type-safe RPC (server + client) |
| Drizzle ORM | ^0.44 | Database ORM |
| PostgreSQL | 17 | Database |
| zod | ^3.24 | Schema validation |

## CLI (apps/cli)

| Lib | Version | Purpose |
|-----|---------|---------|
| citty | ^0.1.6 | Command routing |
| OpenTUI | ^0.1 | Terminal UI (interactive mode) |
| @clack/prompts | ^0.10 | Prompts (legacy, migrating to OpenTUI) |
| picocolors | ^1.1 | Terminal colors (non-interactive) |

## Packages compartilhados

| Package | Responsabilidade |
|---------|-----------------|
| `@atlas/api` | oRPC router definitions (shared types CLI↔Panel) |
| `@atlas/auth` | JWT sign/verify/guard |
| `@atlas/cloudflare` | Cloudflare API (DNS + Tunnels) |
| `@atlas/config` | tsconfig base |
| `@atlas/crypto` | AES-256-GCM encryption |
| `@atlas/db` | Drizzle schema + migrations |
| `@atlas/env` | Zod-validated env vars |
| `@atlas/kubernetes` | ⚠️ Deprecated — re-exports @atlas/runtime |
| `@atlas/provisioner` | Server provisioning phases (K3s + Swarm) |
| `@atlas/runtime` | Container runtime abstraction (K3s + Swarm) |
| `@atlas/ssh` | SSH client com ControlMaster |

## Convenções

### Monorepo

- Workspaces: `apps/*` e `packages/*`
- Versões centralizadas via `catalog:` no root package.json
- Deps internas: `workspace:*`
- Cada package tem `exports` field no package.json
- tsconfig extends `packages/config/tsconfig.base.json`

### Código

- Indent: tabs
- Line width: 100
- Imports: organizados pelo Biome
- Nomes de arquivo: kebab-case
- Tipos: PascalCase
- Funções: camelCase
- Sem `any` — usar `unknown` + type guard
- Sem `catch {}` vazio — sempre tratar ou re-throw
- Sem `process.env` direto — usar `@atlas/env`

### CLI

- Todo comando suporta `--yes` para modo não-interativo (CI)
- Modo interativo: OpenTUI (novos) ou @clack/prompts (legado)
- Modo CI: console.log com prefixos (→, ✓, ✗)
- Spinner pattern: `createSpinner(auto)` do `ui/`

### Panel API

- CRUD via oRPC em `/rpc/*`
- Auth OAuth e SSE streaming via rotas Elysia custom
- Auth middleware: `requireAuth()` do `@atlas/auth`
- Secrets: sempre encriptados via `@atlas/crypto`
- Audit log: toda ação administrativa logada
- Multi-tenant: tudo scoped por `orgId`
