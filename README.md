<div align="center">

<img src="assets/logo.svg" alt="Atlas" width="80" />

# Atlas

**Your infrastructure, your rules. One command. Everything running.**

The open-source platform that turns any Linux server into a production-ready cluster.
No Kubernetes expertise needed. No vendor lock-in. Just `atlas deploy`.

[![Release](https://img.shields.io/github/v/release/codeatlasdev/atlas?style=flat-square&color=325CEB&labelColor=1a1a2e)](https://github.com/codeatlasdev/atlas/releases)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square&labelColor=1a1a2e)](LICENSE)

[Documentation](https://atlas.codeatlas.com.br) · [Quick Start](#quick-start) · [Why Atlas?](#why-atlas) · [Roadmap](#roadmap)

</div>

---

## Why Atlas?

Most teams waste weeks configuring infrastructure before shipping a single feature. Kubernetes, DNS, SSL, CI/CD — it's a lot.

Atlas abstracts all of that. Point it at any VPS and get a full production stack in minutes.

```bash
atlas deploy     # DNS, HTTPS, scaling — all automatic
```

**What you stop worrying about:**

- Container orchestration (K3s or Docker Swarm — you choose)
- DNS records and SSL certificates
- Secrets management
- Server provisioning
- Monitoring and logs

## Quick Start

```bash
# Install
curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash

# Authenticate
atlas login

# Provision a server (choose K3s or Swarm)
atlas infra setup --host root@your-vps

# Deploy
cd your-project && atlas deploy
```

That's it. Your app is live with HTTPS, monitoring, and logs.

## Choose Your Runtime

Atlas lets you pick the right tool for the job:

| | Docker Swarm | K3s (Kubernetes) |
|---|---|---|
| **Best for** | Small teams, 1-2GB RAM servers | Larger teams, 2GB+ RAM |
| **RAM overhead** | ~100MB | ~512MB |
| **Setup time** | ~2 min | ~5 min |
| **Docker Compose** | ✓ Native | ✗ |
| **Helm / ArgoCD** | ✗ | ✓ |
| **Auto-scaling (HPA)** | ✗ | ✓ |

Switch anytime with `atlas infra migrate --to k3s` or `--to swarm`.

## Commands

```
atlas login                     Authenticate with GitHub
atlas infra setup               Provision a server (K3s or Swarm)
atlas infra migrate             Switch runtimes (K3s ↔ Swarm)
atlas deploy                    Build → push → deploy
atlas status                    Cluster overview (TUI)
atlas logs [service] -f         Stream logs
atlas exec [service]            Shell into a container
atlas restart [service|all]     Rolling restart
atlas scale [service] -r N      Scale replicas
atlas env list|set|pull         Manage secrets
atlas preview start|stop|list   Preview environments per branch
atlas db migrate|psql|backup    Database management
atlas panel setup|status|config Control Panel
```

## Architecture

```
atlas/
├── apps/
│   ├── cli/              CLI (OpenTUI + citty)
│   ├── panel/            Control Panel API (Elysia + oRPC)
│   └── docs/             Documentation (Fumadocs)
├── packages/
│   ├── api/              oRPC router definitions
│   ├── auth/             JWT auth (sign/verify/guard)
│   ├── cloudflare/       Cloudflare API (DNS + Tunnels)
│   ├── config/           Shared tsconfig
│   ├── crypto/           AES-256-GCM encryption
│   ├── db/               Drizzle ORM + PostgreSQL
│   ├── env/              Type-safe env (zod)
│   ├── provisioner/      Server provisioning (K3s + Swarm)
│   ├── runtime/          Container runtime abstraction
│   └── ssh/              SSH client (ControlMaster)
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
| Encryption | AES-256-GCM (Web Crypto) |
| Runtime | K3s or Docker Swarm |
| Ingress | Traefik v3 |
| Certificates | cert-manager (K3s) / ACME (Swarm) |
| DNS | Cloudflare API |
| Monitoring | Prometheus + Grafana |
| Logs | Loki + Promtail |
| Registry | GitHub Container Registry |
| Lint | Biome |

## Development

```bash
git clone https://github.com/codeatlasdev/atlas.git
cd atlas && bun install

bun run db:start                # Start PostgreSQL
bun dev                         # Panel API + Docs

# CLI is used directly
bun run apps/cli/src/index.ts infra setup
```

## Contributing

PRs welcome. Please open an issue first to discuss what you'd like to change.

## License

[MIT](LICENSE)

---

<div align="center">

Built by [CodeAtlas](https://codeatlas.com.br)

</div>
