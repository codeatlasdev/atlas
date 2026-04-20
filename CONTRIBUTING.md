# Contributing to Atlas

Thanks for your interest in contributing. This guide gets you from zero to productive.

## Prerequisites

- [Bun](https://bun.sh) >= 1.3
- [Docker](https://docs.docker.com/get-docker/) (for PostgreSQL in dev)
- Git

## Quick Start

```bash
git clone https://github.com/codeatlasdev/atlas.git
cd atlas
bun install
```

That's it. No build step needed — Bun runs TypeScript directly.

## Project Structure

```
atlas/
├── apps/
│   ├── cli/          # CLI (citty + OpenTUI)
│   ├── panel/        # Control Panel API (Elysia + oRPC)
│   └── docs/         # Documentation (Next.js + Fumadocs)
├── packages/
│   ├── api/          # Shared oRPC contract + types
│   ├── auth/         # JWT sign/verify (Web Crypto)
│   ├── cloudflare/   # Cloudflare DNS + Tunnels client
│   ├── config/       # Shared tsconfig
│   ├── crypto/       # AES-256-GCM encryption (Web Crypto)
│   ├── db/           # Drizzle schema + migrations
│   ├── env/          # Zod-validated env vars
│   ├── firecracker/  # Firecracker runtime + provisioner
│   ├── provisioner/  # Server provisioning phases
│   ├── runtime/      # Container runtime abstraction (K3s/Swarm/Firecracker)
│   └── ssh/          # SSH client with ControlMaster
├── atlas             # Dev CLI wrapper (./atlas <command>)
└── turbo.json
```

## Development

### Panel API + Docs

```bash
# Start PostgreSQL + Panel + Docs
bun run dev:all

# Or individually:
bun run db:start       # PostgreSQL on :5435
bun run dev:panel      # Panel API on :3100
bun run dev:docs       # Docs on :3000
```

Panel health check: `curl http://localhost:3100/health`

### CLI

Use the `./atlas` wrapper at the repo root:

```bash
./atlas --help
./atlas status --demo        # TUI with mock data (no server needed)
./atlas deploy --dry-run     # Animated deploy TUI (no Docker needed)
./atlas infra --help
```

The wrapper runs `apps/cli/src/index.ts` directly via Bun.

### Iterating on the TUI

The CLI has two modes for developing UI without infrastructure:

**`--demo` mode** (status command): renders the full status TUI with realistic mock data. No SSH, no server.

```bash
./atlas status --demo
```

**`--dry-run` mode** (deploy command): shows the animated deploy panel with simulated steps. No Docker, no GHCR, no server.

```bash
./atlas deploy --dry-run
```

Both work without any configuration — clone, install, run.

### TUI Component Testing

Components can be tested headlessly using OpenTUI's test renderer:

```ts
import { createTestRenderer } from "@opentui/core/testing";
import { Box } from "@opentui/core";
import { Header, DeployPanel } from "./ui";

const { renderer, renderOnce, captureCharFrame } = await createTestRenderer({
  width: 80,
  height: 24,
});

const root = Box({ width: "100%", flexDirection: "column" });
root.add(Header("atlas status", "demo"));
renderer.root.add(root);

await renderOnce();
const frame = captureCharFrame(); // plain text, no ANSI escapes
```

See `apps/cli/src/ui/index.test.ts` for examples.

## Testing

```bash
# Run all tests (parallel via Turborepo)
bun run test

# Run tests for a specific package
cd packages/auth && bun test
cd packages/crypto && bun test
cd apps/cli && bun test

# Full validation (types + lint + tests)
bun run validate
```

> **Note**: `bun run validate` currently fails on `cli#check-types` due to pre-existing type
> errors (OpenTUI API migration, PanelClient refactor pending). Tests and lint pass cleanly.
> Use `bun run test` as the primary validation command for now.

### Test Coverage

| Package | Tests | What's covered |
|---------|-------|----------------|
| `@atlas/crypto` | 10 | Encrypt/decrypt roundtrip, unicode, wrong key, env wrappers |
| `@atlas/auth` | 13 | JWT sign/verify, expiry, tampered tokens, requireAuth, assertRole |
| `@atlas/env` | 7 | Schema defaults, validation, coercion, error messages |
| `cli` | 16 | Status parser, mock data, TUI components (Header, StatusLine, Divider, DeployPanel) |

## Code Style

- **Formatter/Linter**: Biome (tabs, 100 char width)
- **Pre-commit hook**: Biome check + type check (via lefthook)
- **No `any`** — use `unknown` + type guard
- **No empty `catch`** — always handle or re-throw
- **No `process.env` directly** — use `@atlas/env` (except in low-level packages)
- **Secrets always encrypted** via `@atlas/crypto`

See `.kiro/steering/` for full conventions.

```bash
# Format + lint
bun run fix

# Check without fixing
bun run check
```

## Submitting Changes

1. Fork the repo and create a branch from `main`
2. Make your changes
3. Run `bun run test` — must pass
4. Open a PR with a clear description of what changed and why

Please open an issue first for large changes to discuss the approach.
