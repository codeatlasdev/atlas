# Runtime and TUI rules (atlas-tui)

Nested notes, loaded when you work under `crates/atlas-tui/`. Moved verbatim from the old root `AGENTS.md`. The orphan-kill and container-runtime rules also stay in the root: breaking them crashes the user's machine.

- Orphan kill uses only the `.pids` file and exact PIDs; never `lsof` port-kill or negative PIDs (port forwarders of container runtimes such as OrbStack own those ports, and a recycled process-group id can hit atlas itself).
- Never restart the container runtime (`orb restart docker`) or run `docker compose down` from atlas.
- Children: `setsid`, `stdin(null)`, `sh -c` for commands.
- TUI: EventStream input, dirty-flag rendering, non-blocking startup.
- `pre_command` output goes to `.logs/setup.log` / `.logs/setup.json`; `pre_command_when: always` is the default with an explicit command.

Manager tests live in `runtime/manager/tests.rs` and are wired by `#[cfg(test)] mod tests;` in `mod.rs` — when splitting modules, check the test count does not drop.
