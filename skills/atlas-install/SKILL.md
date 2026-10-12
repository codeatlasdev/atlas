---
name: atlas-install
description: Install, update, and wire up the atlas CLI on a developer machine, including connecting it to the OMP, Claude Code or Codex coding agents (MCP server, skills, hooks). Use when a new developer sets up atlas, when `atlas` is not found or outdated, when the atlas MCP tools or skills are missing in the agent, or when working on atlas itself and testing a local build with atlas link.
metadata:
  version: "4.1.0"
---

# atlas install and agent setup

## New developer, one pass

1. Install the binary (public GitHub release, sha256-verified, no login):
   ```bash
   curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash
   bash scripts/bootstrap.sh        # from a clone: same, plus offers `atlas setup` for detected agents
   ```
   It downloads the latest release and runs `atlas install`: binary to `~/.atlas/bin/atlas`, `PATH` entry for fish/zsh/bash, and the bundled skills into `~/.agents/skills/`. Open a new shell afterwards.
2. Preview, then wire atlas into your agent (`omp`, `claude-code` or `codex`; plain `atlas setup` shows which are installed):
   ```bash
   atlas setup claude-code            # dry run: prints what would change, writes nothing
   atlas setup claude-code --apply    # writes it
   ```
   Registers the `atlas` MCP server (`atlas mcp`), makes the skills available and adds hooks (`atlas hook <agent> …` for Claude Code and Codex). Existing MCP servers, skills, hooks and settings are preserved, and running `--apply` again reports `up to date`. If a different `atlas` MCP entry already exists it refuses; `--force` replaces it.
   **OMP is different: it gets one plugin.** `atlas setup omp --apply` writes `~/.atlas/omp-plugin/` (`package.json`, `.mcp.json`, `hooks/pre/atlas.ts` with header `managed by atlas setup`, `rules/`, `agents/`, `skills/`) and runs `omp plugin link ~/.atlas/omp-plugin`. If `omp` is not on PATH the files are still written and the plan prints the link command. It also removes an older atlas-owned `~/.omp/agent/hooks/pre/atlas.ts` and `mcpServers.atlas` entry so nothing loads twice.
3. Restart the agent. OMP: restart the session (the hook loads at start; `/reload-plugins` only refreshes skills and MCP), then `/mcp list` and `omp plugin list` (`atlas-omp`). Claude Code: `claude mcp get atlas` should say Connected. Codex: run `/hooks` once to trust the atlas hooks (they are skipped until reviewed), `codex mcp get atlas`.
4. In a project: `atlas init` creates `atlas.yaml`; `atlas config check` validates it.

`atlas setup <agent>` options: `--scope user|project` (user is the default: `$CLAUDE_CONFIG_DIR` else `~/.claude` plus `~/.claude.json`; `$CODEX_HOME` else `~/.codex`. Project writes `.mcp.json` + `.claude/`, or `.codex/` in the nearest directory with `atlas.yaml` so it can be committed. omp plugins are always user-level: `--scope project` for omp writes nothing and only removes legacy `.omp/` files), `--apply`, `--force`, `--uninstall`. Skills always go to the user scope: `~/.agents/skills/` for Codex (shared with the OMP fallback copy), `~/.claude/skills/` for Claude Code, and the plugin's own `skills/` for OMP.

## Verify

```bash
atlas --version          # atlas X.Y.Z
atlas version            # also prints OS and architecture
atlas skills status      # bundled vs installed skills
atlas setup              # per agent: "set up, up to date"
```

## Update

```bash
atlas update --check     # only report
atlas update             # new binary, then refreshes skills and the OMP / Claude Code / Codex setups it already made
```

Skills ship inside the binary, so updating atlas updates the skills and rewrites the omp plugin files (the link stays valid). After an update, run `/mcp reconnect` in OMP so the server process restarts on the new binary. Homebrew installs: `atlas update` tells you to run `brew upgrade atlas`. Atlas also checks at most once a day and prints a notice.

## Skills only

```bash
atlas skills list              # skills bundled in this binary
atlas skills install [names]   # all if no names; to ~/.agents/skills/
atlas skills install --local   # to .agents/ in the current directory instead
atlas skills status
```

## Working on atlas itself

```bash
atlas link             # cargo build -p atlas-cli, then symlink ~/.atlas/bin/atlas to target/debug/atlas
atlas link --no-build  # symlink the existing debug build
atlas unlink           # restore the release binary
```

`atlas link` backs up the release binary first; after it, `cargo build -p atlas-cli` is enough to refresh. Build `atlas-cli`, not `atlas-tui` (that is only the library). The embedded skills are compiled from `skills/` in the repo, so editing a skill needs a rebuild before `atlas skills install` sees it.

## Troubleshooting

| Symptom | Fix |
|---|---|
| `atlas: command not found` | Open a new shell, or `export PATH="$HOME/.atlas/bin:$PATH"` |
| MCP tools missing in the agent | `atlas setup <agent> --apply`, then restart it (OMP: `omp plugin list` must show `atlas-omp` enabled, then `/reload-plugins`); GUI-launched apps may need an absolute `command` path (`~/.atlas/bin/atlas`) |
| `atlas setup <agent>` refuses: foreign `atlas` entry | Inspect it; `--force` only if it is safe to replace |
| Download fails in bootstrap | Check the network and `https://github.com/codeatlasdev/atlas/releases`; the script needs `curl`, `tar` and `shasum`/`sha256sum` |
| Codex ignores the atlas hooks | `/hooks` in Codex: review and trust them once |
| Remove everything | `atlas setup <agent> --uninstall --apply` for each agent (omp: unlinks the plugin and deletes `~/.atlas/omp-plugin`), then `atlas uninstall` |
