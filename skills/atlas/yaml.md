# atlas.yaml cheat sheet

Reference for the `atlas` skill. Condensed so you can edit safely; the complete, authoritative reference is `docs/configuration.md` in the Atlas repo (field semantics, tunnel, mobile, env_files, full example). Anything not listed here: read that file or run `atlas config schema`.

**Always validate after editing:** `atlas config check` (prints `OK: <path> is valid`, or every error and exit `1`; non-fatal `warning:` lines exit `0`). Unknown keys are errors with a "did you mean". The running `atlas dev` does not reload the file: the user must restart it.

## Shape

```yaml
name: shop                      # required
pre_command: ""                 # omit = auto-detect JS install; "" = never install
pre_command_when: always        # always (default with a pre_command) | lockfile; auto-detect is always lockfile-gated; output in .logs/setup.log
env_files: [.env]               # atlas.yaml only; later wins; missing files skipped
infra:
  compose_file: docker-compose.yml   # `docker compose -f <file> up -d` before apps; never `down`
services:
  database:                     # no command = externally managed (docker), only port-watched
    port: 24312                 # host port mapped in compose ("24312:5432"); block 24310-24319
    hide_logs: true             # no tab, not in the All tab
  migrate:
    command: bun run db:migrate # run as `sh -c`; cd, &&, pipes work
    oneshot: true               # exit 0 = success
    depends_on: { database: healthy }
  api:
    command: cd apps/api && air
    dir: apps/api               # working dir relative to project root
    port: 24310                 # project block 24310-24319 (see Ports); injected as PORT
    health: http://localhost:24310/health  # real GET; 2xx/3xx healthy; same number, no ${PORT}
    ready_log: 'listening on'   # regex; service stays `starting` until a line matches
    start_timeout: 60           # seconds; 0 disables; service is never killed for it
    autostart: true             # false = stays stopped until started (menu, palette, atlas_start_service)
    depends_on: { migrate: completed_successfully }
    restart: on_failure         # no | on_failure | always
    max_restarts: 5             # 0 = unlimited
    env: { LOG_LEVEL: debug }
    open_key: a
    critical: true
    critical_help: "Run `bun run db:up` and retry"
    lock_file: apps/api/.dev/dev.lock    # JSON {"pid": N}; stale owner cleaned before start
profiles:                       # groups for `atlas dev --profile NAME` (repeatable, union)
  backend: [api, migrate]       # members must be services; their depends_on come along
actions:
  - key: c
    label: Run check
    command: bun run check
  - key: R
    label: Reset local DB
    command: bun run db:reset
    confirm: "This wipes the local database. Continue?"
    pause_services: [api]       # suspended while it runs, restarted after
```

Order in the file does not matter; atlas sorts by `depends_on`.

## depends_on

List form `[database, redis]` means each `healthy`. Map form conditions: `started`, `healthy`, `completed_successfully` (target needs `command`), `log_ready` (target needs `ready_log`). If a dependency can never be satisfied the dependent becomes `failed` with `dependency X failed`.

## profiles and partial runs

`atlas dev [SERVICE...] [--profile NAME]... [--no-deps]` starts only the named services / profile members plus their transitive `depends_on` (a command-less infra service like `database` only when something selected needs it); `--no-deps` skips dependencies. Everything else shows `off` (state `disabled`, `"off": true` in `atlas_status` rows, `"deselected": true` in `.logs/.tui-status.json`) and is started by hand: `atlas_start_service` (or menu/palette). Unknown service/profile names fail fast with a "did you mean"; a profile member that is not a service is a config error. Without arguments everything starts. Do not treat an `off` service as broken. You still never run `atlas dev` yourself; ask the user.

## Actions

Required: `key` (one char), `label`, `command` (`sh -c` in project root). Optional: `confirm` (modal in the TUI; MCP refuses until `confirmed: true`), `pause_services`, `group`, `open_tunnel` (needs a `tunnel:` block), and session options `session`, `input_map`, `platform` (`ios`|`android`), `device_id`, `host_mode` (`auto`|`lan`|`simulator`). Session and `platform` actions need the TUI; MCP cannot start `platform` actions under `--headless`. Details: `docs/configuration.md#actions`.

## deploy (`atlas deploy`)

`deploy: { app?, build?, artifact?: [paths], targets: { NAME: { host (ssh alias|local), dir? (abs, default /opt/<app>), keep? (>=2, 5), link?: {rel: /abs}, install?, migrate?, hooks?: {pre_swap, post_swap, post_deploy, on_failure}, pause_marker?, health_timeout? (60), health_passes? (3), soak? (15), min_free_mb? (1024), services: [{name, kind: systemd|compose|command, health?: url, compose_file? (compose only), restart? (command only, required)}] } } }`. Services restart in list order; a failed health gate rolls back to the last healthy release. Migrations must be expand-only. Secrets go in host files referenced by `link`, never in the repo. Not overridable from atlas.local.yaml. Details: `docs/deploy.md`.

## Ports

Every dev runs several projects at once. Default ports (3000, 3001, 4000, 5000, 5173, 5432, 6379, 8000, 8080, 9000, 27017) collide across projects and with tools: never use them.

**Rule: each project gets its own random block of 10 ports**, and every service of that project takes the next port in the block (api = base, web = base+1, db = base+2…). Example: `ac` uses 3070-3075 for apps, 54321/63790 for infra.

1. Pick a base once per project, between 10000 and 30000 (below macOS's ephemeral range 49152+), multiple of 10:
   `python3 -c 'import random; print(random.randrange(10000, 30000, 10))'`
2. Check the block is free (lookup only; never kill what you find):
   `lsof -nP -iTCP:<base>-<base+9> -sTCP:LISTEN` → no output = free. Also `grep -rn "port:" ../*/atlas.yaml` to avoid sibling projects.
3. Use those ports everywhere they appear, consistently:
   - `port:` in `atlas.yaml` (atlas injects it as `PORT`; the app must read `process.env.PORT` / `$PORT`, not hardcode).
   - `health:` URL: write the same number literally (atlas does not expand `${PORT}` there).
   - Docker infra: map host port → container port in compose (`"25432:5432"`), set the command-less service's `port:` to the **host** port, and update `DATABASE_URL` / `REDIS_URL` in `.env`.
   - Anything that calls another service (`VITE_API_URL`, CORS origins, OAuth redirect URIs) in `.env`.
4. Adding a service later: take the next free number in the same block; never reuse another project's.
5. One machine still clashes? Override only there in `atlas.local.yaml` (`services: { web: { port: 25475 } }`), don't change the committed block.

`atlas config check` rejects two enabled services on the same port, but it cannot see other projects: the block rule is what prevents those.

## Reserved keys

An action `key` or service `open_key` must not be one of these (nor repeat another action key or `open_key`); `atlas config check` rejects it:

`0-9 j k g G f v E M L P q r s S t m i ? : / n N W T V J z - D`

Free and good for actions: lowercase letters not listed (`a b c d e h l o p u w x y`) and uppercase not listed (`A B C F H I K O Q R U X Y Z`). Keys are case-sensitive. Source of truth: `BUILTIN_KEYS` in `crates/atlas-tui/src/config/validate.rs`; after choosing, run `atlas config check`.

## atlas.local.yaml

Gitignored per-machine overrides merged over `atlas.yaml`. Per-machine values (ports, `enabled: false`, `pre_command: ""`, mobile team id) go here, not in the committed file.

- `name`, `pre_command`, `pre_command_when`: replace (`""` disables `pre_command`).
- `services.<name>`: only `command`, `port`, `health`, `critical`, `depends_on`, `enabled`, `restart`, `max_restarts`, `ready_log`, `oneshot`, `start_timeout`, `autostart`. Other fields (`dir`, `env`, `open_key`, `open_url`, `hide_logs`, `lock_file`, `critical_help`) are rejected as unknown. Services not in `atlas.yaml` are ignored.
- `profiles`: merged by name (same name replaces that list, new name is added); members are validated against the merged services.
- `actions`: merged by `key` (replace or append). `tunnel`: field-wise, only if the base has one. `infra`: replaces the block. `mobile.ios` / `mobile.android`: each replaces.
- `env_files`: not overridable.

```yaml
pre_command: ""
services:
  web: { port: 25475 }          # this machine only
  redis: { enabled: false }
```

## Validation catches

Unknown fields; `depends_on` targets exist (no self, no cycles); `log_ready` needs target `ready_log`; `completed_successfully` needs target `command`; `ready_log` is a valid regex; unique ports among enabled services; unique action keys that do not clash with built-in keys or `open_key`s; `open_tunnel` needs `tunnel:`.

## Common mistakes

| Wrong | Right |
|---|---|
| `command: docker compose up redis` without `-d` | Add `-d`, or drop `command` and use `infra:`; a blocking command keeps the service `starting` |
| `open_key` on a service with no `port` and no `open_url` | Set `port` or `open_url` |
| `pre_command: bun install` when a lockfile exists | Omit it; auto-detected |
| `depends_on: { api: log_ready }` and `api` has no `ready_log` | Add `ready_log` to `api` |
| Migration as a long-running service | `oneshot: true`, depend on it with `completed_successfully` |
| `restart: always` on a one-shot task | `on_failure`; `always` relaunches after every success |
| Action key `s`, `r`, `v`, `f`, `D` ... | Pick a key outside the reserved list (see Reserved keys) |
| Two enabled services on the same port | Give each its own `port` in the project block; `atlas config check` flags it |
| `port: 3000` / `5173` / `5432` / `6379` / `8080` | A port from the project's random block (see Ports); defaults collide with other projects |
| App hardcodes its port while `atlas.yaml` says another | App reads `PORT`; keep `health:`, compose mapping and `.env` URLs on the same number |
| Killing whatever holds a port (`kill $(lsof -t -i:…)`) | Never; find the owner with `lsof -nP -iTCP:<port> -sTCP:LISTEN` and tell the user |
| `tunnel:` with an ssh key that needs a passphrase | Atlas uses `BatchMode=yes`; load the key in ssh-agent and accept the host key first |

Editor autocomplete: `atlas config schema > .atlas.schema.json`, then put `# yaml-language-server: $schema=.atlas.schema.json` on the first line of `atlas.yaml`.
