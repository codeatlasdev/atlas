<div align="center">

# ⚡ Atlas

**Your own Heroku. Any server. One command. Everything running.**

An open-source Internal Developer Platform that turns any Linux server into a production-ready cluster. Developers write code — Atlas handles the rest.

[![Release](https://img.shields.io/github/v/release/codeatlasdev/atlas?style=flat-square&color=6366f1&labelColor=1a1a2e)](https://github.com/codeatlasdev/atlas/releases)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square&labelColor=1a1a2e)](LICENSE)

[Install](#install) · [Quick Start](#quick-start) · [How It Works](#how-it-works) · [Commands](#commands) · [Roadmap](#roadmap)

</div>

---

## Install

```bash
curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash
```

Or build from source:

```bash
git clone https://github.com/codeatlasdev/atlas.git
cd atlas && bun install && bun run build
```

## What is Atlas?

Atlas is an IDP (Internal Developer Platform) for teams that want the Heroku/Vercel experience on their own infrastructure. Point it at any VPS — Hetzner, DigitalOcean, AWS, bare metal — and Atlas provisions a full production stack automatically.

**The developer experience:**

```bash
atlas deploy     # that's it. DNS, HTTPS, scaling — all automatic.
```

**What the developer never touches:**
- Kubernetes manifests
- Dockerfiles
- DNS records
- SSL certificates
- Secrets management
- Server provisioning
- CI/CD pipelines

## Quick Start

```bash
# 1. Authenticate
atlas login

# 2. Provision a server (any VPS, any provider)
atlas infra setup --host root@your-server.com --domain myapp.com

# 3. Deploy
cd your-project
atlas deploy

# 4. Done — your app is live at https://myapp.com
```

## How It Works

Atlas reads an `atlas.yaml` in your project root and handles everything:

```yaml
name: myapp
org: myorg
domain: myapp.com

services:
  server:
    type: api
    port: 3001
    domain: api.myapp.com
  web:
    type: web
    domain: myapp.com
  worker:
    type: worker

infra:
  postgres: true
  redis: true
```

From this single file, Atlas:

1. **Builds** Docker images for each service
2. **Pushes** to your container registry (GHCR)
3. **Deploys** to your Kubernetes cluster via the Control Panel
4. **Configures DNS** automatically via Cloudflare
5. **Provisions HTTPS** via Let's Encrypt
6. **Manages secrets** encrypted at rest, synced to the cluster

## What `atlas infra setup` installs

One command turns a fresh Linux server into a production cluster:

| Component | Purpose |
|-----------|---------|
| **K3s** | Lightweight Kubernetes |
| **Traefik v3** | Ingress + automatic HTTPS redirect |
| **cert-manager** | Let's Encrypt certificates |
| **Prometheus + Grafana** | Metrics + dashboards |
| **Loki + Alloy** | Centralized logs |
| **ArgoCD** | GitOps continuous deployment |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Developer                            │
│                                                             │
│   atlas deploy  ─────────►  Control Panel API               │
│   atlas logs                (the brain)                     │
│   atlas env set                  │                          │
│   atlas login                    │                          │
└──────────────────────────────────┼──────────────────────────┘
                                   │
                    ┌──────────────┼──────────────┐
                    ▼              ▼              ▼
              ┌──────────┐  ┌──────────┐  ┌──────────┐
              │ Server 1 │  │ Server 2 │  │ Server N │
              │   K3s    │  │   K3s    │  │   K3s    │
              │ Traefik  │  │ Traefik  │  │ Traefik  │
              │ cert-mgr │  │ cert-mgr │  │ cert-mgr │
              │ Your App │  │ Your App │  │ Your App │
              └──────────┘  └──────────┘  └──────────┘
                    │              │              │
                    └──────────────┼──────────────┘
                                   │
                            ┌──────┴──────┐
                            │ Cloudflare  │
                            │ DNS + Proxy │
                            └─────────────┘
```

The **Control Panel** is the central brain. The CLI and future Web UI are just clients. Every action flows through the Panel API — deploy, DNS, secrets, logs, provisioning.

## Commands

```
atlas login                     Authenticate with GitHub
atlas infra setup               Provision a fresh server
atlas deploy                    Build → push → deploy to cluster
atlas status                    Cluster overview
atlas logs [service] -f         Stream logs in real-time
atlas env list|set|pull         Manage secrets
atlas exec [service]            Shell into a container
atlas restart [service|all]     Rolling restart
atlas scale [service] -r N      Scale replicas
atlas preview start|stop|list   Ephemeral preview environments
atlas db migrate|psql|backup    Database management
```

### Control Panel

```
atlas panel setup               Connect CLI to Control Panel
atlas panel status              Show servers and projects
atlas panel config              Configure GitHub App, Cloudflare (interactive)
atlas panel server add          Add and provision a server
atlas panel server list         List servers with status
```

## Control Panel API

The Panel API powers everything. It's an Elysia server with PostgreSQL, deployed to your own cluster.

| Route | Description |
|-------|-------------|
| `POST /deploys/project/:id` | Trigger deploy (kubectl + DNS) |
| `GET /logs/project/:id` | Stream logs via SSE |
| `PUT /secrets/project/:id` | Set secrets (AES-256-GCM encrypted) |
| `GET /secrets/project/:id/values` | Pull decrypted secrets |
| `POST /servers` | Add server (with optional provisioning) |
| `PATCH /org/settings` | Configure Cloudflare, GitHub App |
| `GET /auth/github` | OAuth login flow |

## Tech Stack

| Layer | Technology |
|-------|-----------|
| CLI | Bun + citty + @clack/prompts |
| Panel API | Elysia + Drizzle ORM + PostgreSQL |
| Encryption | AES-256-GCM via Web Crypto API |
| Auth | JWT (HMAC-SHA256) + GitHub OAuth |
| Container Runtime | K3s (lightweight Kubernetes) |
| Ingress | Traefik v3 |
| Certificates | cert-manager + Let's Encrypt |
| DNS | Cloudflare API |
| Monitoring | Prometheus + Grafana |
| Logs | Loki + Alloy |
| Registry | GitHub Container Registry |

## Roadmap

- [x] **Control Panel API** — deploy, DNS, secrets, logs, provisioning
- [x] **CLI** — deploy, panel, login, env commands
- [x] **DNS automation** — Cloudflare integration
- [x] **Secrets management** — encrypted at rest + K8s sync
- [x] **Server provisioner** — K3s + full monitoring stack
- [x] **Logs streaming** — SSE via kubectl
- [ ] **@atlas/env** — Type-safe environment variables from `atlas.yaml`
- [ ] **atlas dev** — Local development (reads atlas.yaml → docker-compose)
- [ ] **SDK packages** — @atlas/db, @atlas/cache, @atlas/log
- [ ] **Web UI** — Control Panel dashboard
- [ ] **atlas create** — Full project scaffolding + first deploy

## Development

```bash
# Clone
git clone https://github.com/codeatlasdev/atlas.git
cd atlas

# Install dependencies
bun install

# Run CLI in dev mode
bun run dev -- deploy

# Run Control Panel API
cd panel && docker compose up -d    # PostgreSQL
cd panel/api && bun run src/seed.ts # Create org + admin
bun run dev:panel                   # Start API on :3100
```

## License

[MIT](LICENSE)

---

<div align="center">

Built by [CodeAtlas](https://codeatlas.com.br)

</div>
