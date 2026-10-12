---
title: "Deploy"
description: "Ship releases with atlas deploy: atomic swap, health gate and automatic rollback."
---

# Deploy: `atlas deploy`

`atlas deploy` ships a release to a single host and swaps to it only after every service is healthy. Configure it in the `deploy:` section of `atlas.yaml`.

You build once, on your machine or in CI. Atlas packs the artifact, uploads it over ssh, and starts a **driver** on the host. The driver runs on its own, detached from ssh, and owns the whole transaction: lock, seal, install, migrate, swap, restart, health gate, and rollback. The local `atlas` process only tails the driver's log. Ctrl-C, a cancelled CI job, or a dropped connection cannot leave the host half-deployed.

## Guarantees

| # | Guarantee | How |
|---|---|---|
| 1 | The host finishes what it started | The driver runs under `systemd-run` when systemd is PID 1 and atlas runs as root, otherwise under `setsid nohup`. Killing ssh does not kill it. |
| 2 | One deploy at a time, and the lock never goes stale | The driver holds a kernel `flock` on `.atlas/lock` for its whole life. The lock dies with the process, so it never needs a TTL and nothing ever steals it. |
| 3 | Releases are immutable | Release ids are `YYYYMMDDHHMMSS-<sha7>[-dirty]`. The upload lands in `releases/.<id>.partial`, its sha256 is checked, then it is renamed. Ordering always goes by id, never by mtime. |
| 4 | The swap is atomic | `ln -sfn releases/<id> current.tmp.<id> && mv -Tf current.tmp.<id> current && sync -f`. |
| 5 | Rollback goes to the last *healthy* release | Each release records `sealed`, `healthy` or `failed` in `releases/<id>/.atlas-release`. Failed releases are skipped. |
| 6 | Healthy means it stays up | A service needs `health_passes` consecutive passing probes, then a `soak` window with no restart. Restarts are detected through systemd `NRestarts`/`ActiveEnterTimestampMonotonic` or docker `RestartCount`/`StartedAt`. |
| 7 | Every deploy is journalled | `.atlas/state.json` holds the current phase and is written atomically. `.atlas/deploys.jsonl` is an append-only audit log. |

If a deploy is interrupted after the swap (the host rebooted, or `kill -9` hit the driver), the next deploy or rollback sees the non-terminal phase in `state.json`. It logs `recovering interrupted deploy <id>`, marks that release failed, and restores the last healthy release before doing anything else.

## Configuration

```yaml
deploy:
  app: shop                 # [a-z0-9-]+, default: project name
  build: bun run build      # local `sh -c` in the project root (skip with --skip-build)
  artifact: [dist, package.json, bun.lock]   # omit to ship `git archive HEAD`
  targets:
    prod:
      host: prod            # ssh host or ~/.ssh/config alias; `local` runs on this machine
      dir: /opt/shop        # default /opt/<app>
      keep: 5               # releases kept (>= 2)
      link:                 # release-relative path -> absolute shared file (secrets live here)
        .env: /opt/shop/shared/.env.production
      install: bun install --frozen-lockfile   # in the new release, before the swap
      migrate: ./scripts/migrate.sh            # in the new release, before the swap (expand-only!)
      hooks:
        pre_swap: ./scripts/smoke.sh           # cwd = new release; non-zero aborts
        post_swap: ./scripts/sync-infra.sh     # cwd = current; re-run on rollback
        post_deploy: ./scripts/notify.sh       # warning only
        on_failure: ./scripts/page.sh          # warning only; gets ATLAS_DEPLOY_REASON
      pause_marker: /opt/shop/shared/.deploy-lock  # touched every 30s while the driver runs
      health_timeout: 60    # seconds per service
      health_passes: 3      # consecutive passing probes
      soak: 15              # seconds with no restart
      min_free_mb: 1024
      services:             # restarted strictly in this order
        - { name: shop-api, kind: systemd, health: "http://127.0.0.1:3000/health" }
        - { name: worker, kind: compose, compose_file: compose.yml }
        - { name: cron, kind: command, restart: "systemctl reload cron" }
```

| Kind | Restart | Liveness, when `health` is unset | Restart fingerprint |
|---|---|---|---|
| `systemd` | `systemctl reset-failed` + `systemctl restart <name>` | `systemctl is-active` | `NRestarts`, `ActiveEnterTimestampMonotonic` |
| `compose` | `docker compose -p <app> -f current/<compose_file> up -d --no-deps --build --remove-orphans <name>` | a running container | `RestartCount`, `StartedAt` |
| `command` | `restart` (required), cwd `current` | always up | none |

`health` is an http(s) URL that is curled **on the host**. A probe passes on a 2xx/3xx response within 5s.

Every hook and command receives `ATLAS_RELEASE`, `ATLAS_RELEASE_DIR`, `ATLAS_PREVIOUS`, `ATLAS_SHA`, `ATLAS_TARGET`, `ATLAS_APP` and `ATLAS_DIR`.

`atlas config check` validates the section. It rejects:
- an empty `targets` or `services` list
- `keep < 2`
- `command` without `restart`
- `restart` or `compose_file` on the wrong kind
- duplicate service names
- a `link` key that is absolute or contains `..`
- a relative `link` value, `dir` or `pause_marker`

The `deploy:` section cannot be overridden from `atlas.local.yaml`.

**Secrets never travel with atlas.** Keep them in host-side shared files and reference them through `link`. Atlas never ships, reads or prints env files.

## Commands

```text
atlas deploy [TARGET] [--dry-run] [--json] [--allow-dirty] [--skip-build] [--force]
                      [--lock-wait SECS] [--no-follow] [-d DIR]
atlas deploy rollback [TARGET] [--to RELEASE_ID] [--json]
atlas deploy status   [TARGET] [--json]
atlas deploy logs     [TARGET] [RELEASE] [-f]
atlas deploy history  [TARGET] [-n 20] [--json]
```

You can omit `TARGET` when there is exactly one target.

- `--dry-run` builds and packs, then prints the release id, the sha256 and the rendered `plan.sh`. It touches nothing remote.
- A dirty working tree is refused unless you pass `--allow-dirty`, in which case the id ends in `-dirty`.
- If HEAD's sha is already current and healthy, the deploy is a `noop`. Use `--force` to redeploy anyway.
- `--no-follow` prints the release id once the driver has started and exits 0.
- Ctrl-C while following prints `detached; deploy continues on host` and exits 130. The host carries on.
- `rollback` swaps to the newest healthy release before `current`, or to `--to <id>`, then restarts and gates every service. A release marked `failed` is refused.
- `status` exits 0 only if the last phase is terminal, the lock is free and every service is healthy. A non-terminal phase with a free lock shows `INTERRUPTED`.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | deployed, noop, or rollback succeeded |
| 1 | failed, and `current` was restored or never changed (`rolled_back: true` when it was restored) |
| 2 | **critical**: the rollback or recovery itself is unhealthy and needs a human |
| 3 | locked by another deploy |
| 4 | cannot reach the host, or lost contact while following (the deploy continues on the host) |
| 130 | detached with Ctrl-C (the deploy continues on the host) |

`--json` prints one object on stdout, and the host log goes to stderr:

```json
{"ok":true,"target":"prod","host":"prod","release":"20250101120000-abcdef0","sha":"abcdef0…","previous":"20241231090000-1234567","phase":"healthy","rolled_back":false,"reason":null,"duration_s":74}
```

## Remote layout

```text
/opt/<app>/
  current -> releases/<id>
  releases/<id>/                 # immutable; .atlas-release = {"id","sha","ts","user","status"}
  releases/.<id>.partial/        # upload in progress
  .atlas/lock                    # flock, held by the running driver
  .atlas/lock.owner              # user@host pid run ts
  .atlas/state.json              # {"release","previous","phase","pid","ts","reason","run"}
  .atlas/deploys.jsonl           # audit trail, one line per finished deploy/rollback
  .atlas/run/<id>/{driver.sh,plan.sh,log,exit}
```

Phases, in order: `preflight`, `uploaded`, `sealed`, `installing`, `migrating`, `pre_swap`, `swapped`, `post_swap`, `restarting`, `healthy`.

A failure up to and including `pre_swap` deletes the new release and leaves `current` untouched. A failure from `swapped` onwards goes through `rolling_back` and ends in `rolled_back` (exit 1) or `failed` (exit 2).

## Host requirements

The host needs Linux with bash, GNU coreutils, util-linux `flock`, `tar`, `curl` and `sha256sum`. Depending on the service kinds you also need `systemctl` (with the ssh user allowed to restart the units) or `docker compose`.

The ssh user needs write access to `dir`. Atlas uses the system `ssh` with `BatchMode=yes` and a 120s `ControlPersist` socket at `~/.ssh/atlas-%C`.

## Migrations

`migrate` runs before the swap while the old release is still serving traffic, and a rollback **never** reverts the schema. Every migration must therefore be expand-only: add columns or tables, backfill, and keep both releases working. Drop columns in a later deploy, once no release that reads them remains in `keep`. This is Fowler's expand/contract pattern.

## GitHub Actions

```yaml
name: deploy
on: { push: { branches: [main] } }
concurrency: { group: deploy-prod, cancel-in-progress: false }   # never cancel a running deploy
jobs:
  deploy:
    runs-on: ubuntu-latest
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v4
      - name: ssh
        run: |
          mkdir -p ~/.ssh && chmod 700 ~/.ssh
          printf '%s\n' "${{ secrets.DEPLOY_SSH_KEY }}" > ~/.ssh/id_ed25519 && chmod 600 ~/.ssh/id_ed25519
          printf '%s\n' "${{ secrets.SSH_KNOWN_HOSTS }}" > ~/.ssh/known_hosts   # pinned; never ssh-keyscan
          printf 'Host prod\n  HostName %s\n  User deploy\n' "${{ secrets.DEPLOY_HOST }}" > ~/.ssh/config
      - run: curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh   # prebuilt releases are macOS; on a Linux runner build atlas from source instead
      - run: ~/.atlas/bin/atlas deploy prod --json
```

Even if the job is cancelled or times out, the deploy still finishes on the host. Check it with `atlas deploy status prod`.

## Example: web app, worker and reverse proxy

A realistic single-host setup: a build step, shared secrets linked from the host, an expand-only migration, a proxy config validated and reloaded after the swap, and three systemd services restarted in order.

```yaml
deploy:
  app: shop
  build: bun run build
  targets:
    prod:
      host: prod
      dir: /opt/shop
      link:
        .env: /opt/shop/shared/.env.production
      install: bun install --frozen-lockfile
      migrate: ./scripts/migrate.sh        # expand-only; exit non-zero aborts before the swap
      hooks:
        post_swap: >-
          caddy validate --config deploy/Caddyfile &&
          sudo cp deploy/Caddyfile /etc/caddy/Caddyfile &&
          sudo systemctl reload caddy
      pause_marker: /opt/shop/shared/.deploy-lock
      services:
        - { name: shop-api,    kind: systemd, health: "http://127.0.0.1:3000/health" }
        - { name: shop-web,    kind: systemd, health: "http://127.0.0.1:3001/" }
        - { name: shop-worker, kind: systemd }
```

## Testing

`cargo test -p atlas-cli --test deploy` exercises the whole lifecycle against `host: local`: deploy, noop, force, crash rollback, manual rollback, lock, interrupted recovery, dirty tree, and `--no-follow`. It needs Linux. On macOS, run it in docker:

```sh
docker run --rm -v "$PWD":/w -v atlas-target:/w/target -w /w rust:1 \
  bash -c 'git config --global --add safe.directory "*"; cargo test -p atlas-cli --test deploy'
```
