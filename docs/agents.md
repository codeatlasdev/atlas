---
title: "Agent setup"
description: "Wire Atlas into omp, Claude Code and Codex with one command: MCP server, skills and hooks."
---

# Agent setup

`atlas setup` wires Atlas into your AI coding agent in one command. Three agents are supported: [OMP](https://github.com/can1357/oh-my-pi), [Claude Code](https://code.claude.com) and [Codex](https://developers.openai.com/codex). They share the same plan/apply machinery, flags and ownership rules.

```bash
atlas setup                          # detect omp / claude / codex on PATH, show status and the commands to run
atlas setup claude-code              # DRY RUN: print the plan, change nothing
atlas setup claude-code --apply      # do it
atlas setup claude-code --apply      # again: everything reports "up to date"
```

Replace `claude-code` with `omp` or `codex`. The default is always a dry run. Each line of the plan shows the artifact, its path, the action (`create`, `update`, `up to date`, `CONFLICT`, `remove`, `skipped`) and a short summary of the change.

| Artifact | OMP | Claude Code | Codex |
|----------|-----|-------------|-------|
| MCP server `atlas` | plugin `~/.atlas/omp-plugin/.mcp.json` | `~/.claude.json` / `.mcp.json` | `config.toml` `[mcp_servers.atlas]` |
| Skills | plugin `skills/` (+ `~/.agents/skills` for other agents) | `~/.claude/skills` (Claude Code does not read `~/.agents/skills`) | **shared** `~/.agents/skills` (Codex reads it; same copy as OMP's) |
| Hooks | plugin `hooks/pre/atlas.ts` | `settings.json` → `atlas hook claude-code …` | `hooks.json` → `atlas hook codex …` (trust once in `/hooks`) |
| `--scope project` | none: omp plugins are user-level | `<project>/.mcp.json` + `<project>/.claude/settings.json` | `<project>/.codex/` (trusted projects only) |

Common flags (all three): `--scope user|project`, `--apply`, `--force`, `--uninstall`.

| Flag | Meaning |
|------|---------|
| `--scope user` (default) | Write the MCP entry and hooks for every project (the agent's user config). |
| `--scope project` | Write them into the project instead. The project root is the nearest ancestor of the current directory that has an `atlas.yaml`; it is an error if there is none. Skills always stay at user scope. |
| `--apply` | Perform the plan. Without it nothing is written. |
| `--force` | Replace an `atlas` MCP entry or hook file that Atlas did not create. |
| `--uninstall` | Remove only what Atlas owns (dry run unless `--apply`). |

## What the hooks do

OMP runs a small extension (inside the Atlas plugin); Claude Code and Codex run the built-in `atlas hook <agent> <event>` subcommand (event JSON on stdin, the agent's JSON response on stdout). All three behave the same and only read local files:

1. **Session start** inside an Atlas project (an `atlas.yaml` in the working directory or a parent): inject the output of `atlas dev --status`. If the stack is not running, tell the agent to ask you to start it and never to run `atlas dev` itself.
2. **Before a shell command**: deny commands that would start the interactive TUI. That is `atlas dev` without `--status`, `--ci` or `--headless`, and `bun run dev` / `npm run dev` / `pnpm dev` / `yarn dev` in a project whose `package.json` `dev` script is `atlas dev`. The reason points the agent at the MCP tools and `atlas dev --status`.

3. **Agent state for Atlas Desktop**: when the agent runs in an Atlas Desktop terminal (`ATLAS_HOOK_ENDPOINT` set), prompt, tool, stop and approval events are posted to the app's loopback server (127.0.0.1 only, 1 s timeout) so it can show working / needs you / done. omp uses the `atlas-status` extension; Claude Code and Codex get extra hooks for `UserPromptSubmit`, `PostToolUse`, `Stop`, plus `Notification`, `StopFailure`, `SessionEnd` (Claude) or `PermissionRequest` (Codex). Outside Atlas Desktop these do nothing. Codex runs hooks in its shared background daemon, without the terminal's environment; for those events `atlas hook` uses `~/.atlas/desktop/hook-endpoint.env` and sends the `cwd`, and the app matches it to the Codex tab it launched in that worktree (events from other directories are ignored).

`atlas hook` is hidden from `--help` and is run by the agent, not by you. Any problem (bad input, not an Atlas project, unknown tool) produces no output and exit code 0, i.e. "no opinion". The hook command registered with Claude Code and Codex is `atlas hook <agent> <event> 2>/dev/null || true`: both agents treat exit code 2 as a *blocking* error, and the `|| true` guarantees that an older `atlas` on `PATH` without the `hook` subcommand, or none at all, can never block your tool calls.

Dry-run it by hand:

```bash
echo '{"tool_name":"Bash","tool_input":{"command":"atlas dev"},"cwd":"'"$PWD"'"}' | atlas hook claude-code pre-tool-use
echo '{"cwd":"'"$PWD"'"}' | atlas hook codex session-start
```

## `atlas setup omp`

```text
atlas setup omp [--scope user|project] [--apply] [--force] [--uninstall]
```

Everything for omp is one plugin, `~/.atlas/omp-plugin/`, linked into omp with `omp plugin link`. omp plugins are user-level, so `--scope project` writes nothing for omp: it only removes legacy project files (the `<project>/.omp/mcp.json` entry and `<project>/.omp/hooks/pre/atlas.ts`) left by older Atlas versions, and prints a warning.

### What it installs

| Artifact | Path |
|----------|------|
| Plugin manifest | `~/.atlas/omp-plugin/package.json` (`name: atlas-omp`) |
| MCP server `atlas` | `~/.atlas/omp-plugin/.mcp.json` |
| Hook | `~/.atlas/omp-plugin/hooks/pre/atlas.ts` |
| Rule (TTSR) | `~/.atlas/omp-plugin/rules/atlas-no-dev-tui.md` |
| Subagent `atlas-ops` | `~/.atlas/omp-plugin/agents/atlas-ops.md` |
| Skills | `~/.atlas/omp-plugin/skills/<name>/` and `~/.agents/skills/<name>/` |
| Link | `omp plugin link ~/.atlas/omp-plugin` (state shown by `omp plugin list`) |

**Plugin.** Atlas owns the plugin directory and rewrites it on every `--apply` and `atlas update`; never edit files in it. The `Plugin link` step runs `omp plugin link` when the plugin is not registered, `omp plugin enable atlas-omp` when it is registered but disabled, and nothing when it is already enabled. If `omp` is not on `PATH` the link step is `skipped`, the files are still written, and the plan prints the command to run yourself.

**MCP server.** `.mcp.json` declares `{"type":"stdio","command":"atlas","args":["mcp"]}` under `mcpServers.atlas`. If `atlas` is not on `PATH`, the absolute path of the running binary is written instead and the plan says so. See [MCP](mcp.md) for the tools, resources and prompts (`/atlas:status`, `/atlas:diagnose <service>`, `/atlas:add-service`).

**Skills.** The `atlas`, `atlas-desktop` and `atlas-install` skills are compiled into the binary, so they always match the version you run; nothing is fetched from GitHub. omp loads them from the plugin (and prefers that copy); the same set is kept in `~/.agents/skills/` for Claude Code and Codex. Ownership of the shared copy is tracked in `~/.atlas/.atlas-skills.json`. `atlas skills list|status|install|update` work on the same bundled copy.

**Rule.** `atlas-no-dev-tui` is a TTSR rule: when the model starts generating a bash command that runs `atlas dev` (other than `--status`, `--ci`, `--headless`, `--help`), omp interrupts the generation and shows the reminder to use the MCP tools instead.

**Subagent.** `atlas-ops` diagnoses and fixes one unhealthy service through the MCP tools (status, logs, fix, restart, wait for readiness) and never starts or stops the whole stack.

**Hook.** A small OMP extension that never throws, never touches the network, and gives every command 2 seconds:

1. On the first agent turn of a session inside an Atlas project (an `atlas.yaml` in the working directory or a parent) it injects the output of `atlas dev --status`. If the stack is not running it tells the agent to ask you to start it and never to run `atlas dev` itself. This happens once per session.
2. It blocks tool calls that would start the interactive TUI: bash commands running `atlas dev` without `--status`, `--ci` or `--headless`, `bun run dev` / `npm run dev` / `pnpm dev` / `yarn dev` in a project whose `package.json` `dev` script is `atlas dev`, and the same commands started through the `hub` tool (`op: start`, `application` + `args`). The block reason points the agent at the MCP tools and `atlas dev --status`.

The first line of the hook is a comment containing `managed by atlas setup` and the Atlas version. Don't edit the file: `atlas setup omp --apply` rewrites it when the content differs, and `atlas update` refreshes it for you. Restart omp sessions to load a new hook (`/reload-plugins` refreshes skills and MCP only).

### Ownership and conflicts

Atlas only touches what it created:

- An `atlas` MCP entry is "owned" when it is a stdio server running `atlas mcp` (bare `atlas` or any path ending in `/atlas`). The plugin directory is entirely Atlas-owned and never conflicts.
- Migration from older versions: an owned `mcpServers.atlas` entry in `~/.omp/agent/mcp.json` and an owned hook `~/.omp/agent/hooks/pre/atlas.ts` (header `managed by atlas setup`) are removed on `--apply`, so the hook and server do not load twice. Foreign `atlas` entries or hook files are left alone.
- `--uninstall` skips foreign entries and files, and never deletes skills that are not in the Atlas manifest or your other MCP servers. A foreign legacy entry is reported as a conflict (`--force` removes it).
- Invalid JSON in an existing legacy `mcp.json` is an error for `--uninstall`; Atlas never overwrites a file it cannot parse.

### After applying

1. Restart omp: the hook loads at session start. In a running session `/reload-plugins` refreshes skills and MCP only.
2. Run `/mcp list`: `atlas` should be listed (from the plugin). `omp plugin list` shows `atlas-omp`.
3. Open omp inside an Atlas project and ask: *"why is api failing?"*, or run `/atlas:diagnose api`.

### Uninstall

```bash
atlas setup omp --uninstall           # preview
atlas setup omp --uninstall --apply   # unlink the plugin, delete ~/.atlas/omp-plugin, remove bundled skills and any legacy atlas files
atlas setup omp --scope project --uninstall --apply   # same, plus legacy files under <project>/.omp/
```

### Environment

| Variable | Effect |
|----------|--------|
| `PI_CODING_AGENT_DIR` | OMP's agent directory; only used to find and migrate the legacy `mcp.json` and hook (same variable OMP uses). |
| `CLAUDE_CONFIG_DIR` | Claude Code's config directory; replaces `~/.claude` (and holds `.claude.json`). |
| `CODEX_HOME` | Codex's home; replaces `~/.codex`. |
| `HOME` | Base for `~/.atlas/omp-plugin`, `~/.agents/skills`, `~/.claude.json` and `~/.atlas/.atlas-skills.json` (omp keeps its plugin registry under `~/.omp/plugins`). |

To try any of them without touching your real setup, point the variables at a temp directory:

```bash
T=$(mktemp -d)
HOME=$T/home PI_CODING_AGENT_DIR=$T/omp CLAUDE_CONFIG_DIR=$T/claude CODEX_HOME=$T/codex atlas setup claude-code --apply
```

`omp` itself reads credentials from your real `HOME`/agent directory, so to run a real `omp` against such a sandbox, keep `HOME` real and override only `PI_CODING_AGENT_DIR` (copy `agent.db` and `config.yml` into it for auth).

## `atlas setup claude-code`

```text
atlas setup claude-code [--scope user|project] [--apply] [--force] [--uninstall]
```

| Artifact | User scope | Project scope |
|----------|-----------|---------------|
| MCP server `atlas` | `~/.claude.json` | `<project>/.mcp.json` |
| Hooks | `~/.claude/settings.json` | `<project>/.claude/settings.json` |
| Skills | `~/.claude/skills/<name>/` | same (always user scope) |

**MCP server.** Adds `{"type":"stdio","command":"atlas","args":["mcp"],"env":{}}` under `mcpServers.atlas`, the same entry `claude mcp add --scope user atlas -- atlas mcp` writes, so entries created by either tool are interchangeable. `~/.claude.json` is Claude Code's own state file (sign-in, per-project state): Atlas only touches `mcpServers.atlas`, keeps every other key and the key order, and writes atomically. Claude Code rewrites that file itself while it runs, so apply with Claude Code closed if you want to be sure no write is lost. Project-scope `.mcp.json` servers ask for your approval the first time Claude Code loads them (`claude mcp reset-project-choices` resets it).

**Skills.** Claude Code reads `~/.claude/skills` and does not read `~/.agents/skills`, so the bundled skills are written there (ownership tracked in `~/.claude/.atlas-skills.json`). Your own skills in that directory are never touched.

**Hooks.** Two entries in `settings.json`: `hooks.SessionStart` (context injection) and `hooks.PreToolUse` with matcher `Bash` (the guard). Your other hooks and settings are kept, in their original order. A handler is Atlas's when its command is exactly `<atlas> hook claude-code <event> 2>/dev/null || true`; if `atlas` is not on `PATH` the absolute path is used (quoted), and `--apply` rewrites it when the path changes. Committing `.claude/settings.json` and `.mcp.json` (project scope) shares the setup with your team; teammates need `atlas` installed.

Environment: `CLAUDE_CONFIG_DIR` relocates the config directory (`settings.json`, `skills/`) and, as Claude Code itself does, `.claude.json` moves inside it.

After applying: restart Claude Code, then run `claude mcp get atlas` (it should say `✔ Connected`) or `/mcp`.

Docs used: [MCP scopes](https://code.claude.com/docs/en/mcp#mcp-installation-scopes), [skills](https://code.claude.com/docs/en/skills), [hooks](https://code.claude.com/docs/en/hooks).

## `atlas setup codex`

```text
atlas setup codex [--scope user|project] [--apply] [--force] [--uninstall]
```

| Artifact | User scope | Project scope |
|----------|-----------|---------------|
| MCP server `atlas` | `~/.codex/config.toml` | `<project>/.codex/config.toml` |
| Hooks | `~/.codex/hooks.json` | `<project>/.codex/hooks.json` |
| Skills | **shared** `~/.agents/skills/<name>/` | same (always user scope) |

**MCP server.** Adds `[mcp_servers.atlas]` with `command = "atlas"` and `args = ["mcp"]` (what `codex mcp add atlas -- atlas mcp` writes). `config.toml` is your main Codex config, so it is edited format-preservingly: comments, ordering and whitespace are untouched, the new table is appended at the end, and uninstall restores the original text. An existing atlas table you tuned (`startup_timeout_sec`, `enabled`, tool filters, …) keeps those keys when only the command path changes. Other spellings of the same server (inline table, dotted keys, quoted keys) are recognised too. Atlas uses the `toml_edit` crate for this: a hand-written line scanner cannot reliably tell a table header from a continuation line of a multi-line array or string, and this file holds your credentials-adjacent settings. Project-scope `.codex/` layers load only for projects you have marked trusted in Codex.

**Skills: shared.** Codex reads `~/.agents/skills`, the directory OMP uses. Setup ensures the bundled skills are there (a no-op if OMP or `atlas skills install` already put them), and `--uninstall` leaves them in place while the other agent (OMP) is still set up, so removing one agent never breaks the other.

**Hooks.** `hooks.json` next to the config gets `SessionStart` and `PreToolUse` (matcher `Bash`) entries running `atlas hook codex <event>`. **Codex runs a hook only after you review and trust it**: after `--apply`, start Codex and run `/hooks` once to trust the two atlas hooks. When a later `atlas update` changes a hook, Codex marks it for review again. Unreviewed hooks are skipped silently apart from a startup warning. Do not keep atlas hooks both in `hooks.json` and inline `[hooks]` in the same layer (Codex warns); Atlas always uses `hooks.json`.

Environment: `CODEX_HOME` replaces `~/.codex`.

After applying: restart Codex, trust the hooks in `/hooks`, then `codex mcp get atlas`.

Docs used: [MCP](https://developers.openai.com/codex/mcp), [skills](https://developers.openai.com/codex/skills), [hooks](https://developers.openai.com/codex/hooks), [advanced config](https://developers.openai.com/codex/config-advanced).

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `/mcp list` does not show `atlas` | Check `omp plugin list` shows `atlas-omp` enabled; if not, run `atlas setup omp --apply` (or `omp plugin link ~/.atlas/omp-plugin`), then `/reload-plugins` or restart omp. |
| `CONFLICT … an atlas entry not created by atlas` | Claude Code / Codex: you have your own `atlas` server entry. Rename it, or re-run with `--force` to replace it. |
| MCP server fails to start | `atlas` is not on omp's `PATH`. Run `atlas setup omp --apply` again from a shell where `which atlas` works (the plugin `.mcp.json` then gets the absolute path), or fix `PATH` (`~/.atlas/bin`). |
| Agent still runs `atlas dev` | The hook loads at session start: check `ls ~/.atlas/omp-plugin/hooks/pre/atlas.ts`, `omp plugin list`, and restart omp. |
| `skipped  Plugin link  … omp not found on PATH` | Install omp, then run the printed `omp plugin link <dir>` (or re-run `atlas setup omp --apply`). |
| No status message at session start | The hook looks for `atlas.yaml` from the session's working directory upward. |
| `error: --scope project needs an atlas.yaml` | Run it from inside the project. |
| Skills missing in omp / Codex | `atlas skills status` shows what is installed in `~/.agents/skills`. |
| Skills missing in Claude Code | They live in `~/.claude/skills` (or `$CLAUDE_CONFIG_DIR/skills`), not `~/.agents/skills`. |
| `claude mcp get atlas` is not Connected | `atlas` is not on Claude Code's `PATH`: re-run `atlas setup claude-code --apply` from a shell where `which atlas` works. |
| Claude Code ignores the project `.mcp.json` entry | Approve it in the interactive prompt, or `claude mcp reset-project-choices`. |
| Codex does not run the atlas hooks | Trust them once: `/hooks` in Codex. They are skipped until reviewed. |
| Codex ignores `<project>/.codex/` | The project must be trusted in Codex (`projects.<path>.trust_level = "trusted"`). |

## Other agents

Cursor is not set up by `atlas setup`. [MCP](mcp.md) documents the manual entry. Future agents will arrive as additional `atlas setup <agent>` variants that reuse the same dry-run, merge and ownership rules; no dates are promised.

## Custom agents in Atlas Desktop

Settings → Agents → **Custom agents** runs any agent CLI (OpenCode, Gemini CLI, Amp, Aider…) like the built-in ones: it is listed in the + menu of a tab strip, can be the default agent (⌘⇧A), and shows its state in the tab and the sidebar. The form takes a name, the command (program plus fixed arguments, quoted like a shell), extra arguments, resume arguments (`{session}` is replaced by the session id) and environment (`KEY=value` per line; `ATLAS_*` is reserved). Entries live in `~/.atlas/desktop/settings.json` under `customAgents`:

```json
{ "customAgents": [
  { "id": "opencode", "label": "OpenCode", "command": "opencode",
    "args": ["--model", "gpt-5"], "resume": ["--session", "{session}"], "env": { "FOO": "1" } }
] }
```

An entry whose `id` is not a slug (`a-z`, `0-9`, `-`, at most 32), clashes with a built-in or earlier entry, or has no `label` or `command` is ignored. A custom agent never gets the built-in agents' auto-approve flags; put such flags in its arguments.

**State without hooks.** A custom agent's tab shows *working* while its terminal prints (hostd's `pty_activity`, at most one a second) and *idle* after 3 quiet seconds. Typing that the program echoes counts as output. Real states (needs you, done) need the hook contract below.

### Hook event contract

Every agent terminal Atlas Desktop starts has these variables:

| Variable | Value |
|---|---|
| `ATLAS_HOOK_ENDPOINT` | Path of a file with `ATLAS_HOOK_PORT=<port>` and `ATLAS_HOOK_TOKEN=<token>` lines (`~/.atlas/desktop/hook-endpoint.env`, mode 0600). |
| `ATLAS_PANE_KEY`, `ATLAS_TAB_ID` | The tab the agent runs in. |
| `ATLAS_WORKTREE_ID` | The worktree. |
| `ATLAS_AGENT` | The agent id (`omp`, `claude`, `codex` or the custom id). |

To report a state, POST to `http://127.0.0.1:<port>/hook/<agent id>` with the header `X-Atlas-Hook-Token: <token>` and a JSON body of at most 1 MB:

```json
{ "pane_key": "<ATLAS_PANE_KEY>", "tab_id": "<ATLAS_TAB_ID>", "worktree_id": "<ATLAS_WORKTREE_ID>",
  "payload": { "event": "user_prompt_submit", "session_id": "…", "prompt": "…" } }
```

The reply is `204` (accepted), `401` (wrong token), `413` (body too large), `400` (not JSON), `404` (path) or `405` (not POST). Without `pane_key` the app matches the tab by agent and `payload.cwd`. Events and the state they set:

| `payload.event` | State |
|---|---|
| `user_prompt_submit`, `pre_tool_use`, `post_tool_use`, `agent_start`, `message_end` | working |
| `permission_request`, `tool_approval_requested` | needs you (blocked) |
| `stop`, `agent_end` | done |
| `session_end`, `session_shutdown` | idle |
| `session_start` | no change (records `session_id` / `session_file`) |

Optional payload fields: `session_id`, `session_file`, `prompt`, `tool_name`, `text` (last message), `cwd`. Hook events win over terminal activity for 5 seconds after each one. Keep the call short (Atlas's own hooks use a 1 s timeout) and ignore failures: outside Atlas Desktop `ATLAS_HOOK_ENDPOINT` is not set.

```sh
. "$ATLAS_HOOK_ENDPOINT"
curl -s -m 1 -X POST "http://127.0.0.1:$ATLAS_HOOK_PORT/hook/$ATLAS_AGENT" \
  -H "X-Atlas-Hook-Token: $ATLAS_HOOK_TOKEN" -H 'Content-Type: application/json' \
  -d "{\"pane_key\":\"$ATLAS_PANE_KEY\",\"payload\":{\"event\":\"stop\"}}" || true
```
