<div align="center">

<img src="assets/logo.svg" alt="Atlas" width="80" />

# Atlas

**Your own Heroku. Any server. One command. Everything running.**

An open-source Internal Developer Platform that turns any Linux server into a production-ready cluster. Developers write code — Atlas handles the rest.

[![Release](https://img.shields.io/github/v/release/codeatlasdev/atlas?style=flat-square&color=325CEB&labelColor=1a1a2e)](https://github.com/codeatlasdev/atlas/releases)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square&labelColor=1a1a2e)](LICENSE)

[Documentation](https://atlas.codeatlas.com.br) · [Install](#install) · [Quick Start](#quick-start) · [Roadmap](#roadmap)

</div>

---

## Install

```bash
curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash
```

## What is Atlas?

Atlas is an IDP (Internal Developer Platform) for teams that want the Heroku/Vercel experience on their own infrastructure. Point it at any VPS — Hetzner, DigitalOcean, AWS, bare metal — and Atlas provisions a full production stack automatically.

```bash
atlas deploy     # that's it. DNS, HTTPS, scaling — all automatic.
```

## Quick Start

```bash
atlas login                          # Authenticate with GitHub
atlas infra setup --host root@vps    # Provision a server
cd your-project && atlas deploy      # Deploy — done
```

## Architecture

```
atlas/
├── apps/
│   ├── cli/              CLI (OpenTUI + citty)
│   ├── panel/            Control Panel API (Elysia + oRPC)
│   └── docs/             Documentation (Fumadocs)
├── packages/
│   ├── api/              oRPC router definitions (shared types)
│   ├── auth/             JWT auth (sign/verify/guard)
│   ├── cloudflare/       Cloudflare API client (DNS + Tunnels)
│   ├── config/           Shared tsconfig
│   ├── crypto/           AES-256-GCM encryption
│   ├── db/               Drizzle ORM schema + migrations
│   ├── env/              Type-safe environment variables (zod)
│   ├── kubernetes/       kubectl abstraction via SSH
│   ├── provisioner/      Server provisioning phases
│   └── ssh/              SSH client with ControlMaster
├── turbo.json
└── package.json
```

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Monorepo | Bun + Turborepo |
| CLI | citty + OpenTUI |
| Panel API | Elysia + oRPC |
| Database | Drizzle ORM + PostgreSQL |
| Auth | JWT (HMAC-SHA256) + GitHub OAuth |
| Encryption | AES-256-GCM via Web Crypto API |
| Env | Zod-validated type-safe env |
| Container Runtime | K3s (lightweight Kubernetes) |
| Ingress | Traefik v3 |
| Certificates | cert-manager + Let's Encrypt |
| DNS | Cloudflare API |
| Monitoring | Prometheus + Grafana |
| Logs | Loki + Alloy |
| Registry | GitHub Container Registry |
| Lint | Biome |
| Git Hooks | lefthook |

## Commands

```
atlas login                     Authenticate with GitHub
atlas infra setup               Provision a fresh server
atlas deploy                    Build → push → deploy to cluster
atlas status                    Cluster overview (TUI dashboard)
atlas logs [service] -f         Stream logs in real-time
atlas env list|set|pull         Manage secrets
atlas exec [service]            Shell into a container
atlas restart [service|all]     Rolling restart
atlas scale [service] -r N      Scale replicas
atlas preview start|stop|list   Ephemeral preview environments
atlas db migrate|psql|backup    Database management
atlas panel setup|status|config Control Panel management
```

## Development

```bash
git clone https://github.com/codeatlasdev/atlas.git
cd atlas && bun install

# Run everything
bun run dev

# Or individually
bun run dev:cli                 # CLI in dev mode
bun run dev:panel               # Panel API on :3100
bun run dev:docs                # Docs site

# Database
bun run db:start                # Start PostgreSQL (Docker)
bun run db:push                 # Push schema
bun run db:studio               # Drizzle Studio
```

## License

[MIT](LICENSE)

---

<div align="center">

Built by [CodeAtlas](https://codeatlas.com.br)

</div>
