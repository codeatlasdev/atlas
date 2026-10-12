---
title: "Logs"
description: "How Atlas collects, stores, searches and exposes service logs."
---

# Logs

Every line a service prints is assembled, classified once, kept in a bounded in-memory buffer, written to disk and streamed to whoever is watching: the [TUI](tui.md), [Atlas Desktop](desktop.md) and [MCP](mcp.md) clients.

## From pipe to line

- Real lines are assembled from the raw pipe. A partial line (a prompt such as `Password: `) shows after 150 ms of silence; `\r\n` is one line break; a lone `\r` keeps only the last state of a progress bar; invalid UTF-8 is replaced with `�`; a line over 16 KiB is cut and ends with ` … [truncated]`.
- Level (`error`, `warn`, `info`, `debug`), timestamp and a sequence number are computed once, when the line arrives.
- When a service starts again after printing output, a dim `──── HH:MM:SS restarted ────` rule separates the runs. It is never searched or counted as output.

Levels are inferred from the text: `error`, `err `, `fatal`, `panic` or red ANSI are *error*; `warn` or yellow are *warn*; `debug`, `trace` or gray are *debug*; the rest are *info*.

## Buffers

Each service keeps its own ring in memory: the last 50,000 lines and at most 8 MiB, whichever comes first. A chatty service cannot push out another service's history. **All** merges the rings in arrival order. The full history is on disk.

## On disk

`<project>/.logs/<service>.log` holds `HH:MM:SS [stdout|stderr] text` with ANSI stripped. The previous run is rotated to `<service>.prev.log` when a service is spawned again, and a log that grows past 50 MB rotates the same way. Output of actions is written to `.logs/action-<slug>.log`. The complete file list is in [Runtime › Files in `.logs/`](runtime.md#files-in-logs).

## Viewing logs

| Where | How |
|---|---|
| TUI | `/` find, `Tab` find/filter, `Ctrl+R` regex, `v` level, `V` time range, `]e` / `]w` error and warning jumps, `}` / `{` and `o` for links. See [TUI › Log viewer](tui.md#log-viewer) |
| Desktop | The **Logs** tab: ⌘F find with regex and case toggles, service chips with error counts, level filter, time range, `e` / `w` jumps, ⌘-click on URLs and `path:line`. See [Desktop › Logs](desktop.md#project-panel) |
| MCP | `atlas_logs` (tail, `level`, `since_minutes`) and `atlas_logs_search` (substring or `/regex/`). See [MCP](mcp.md#atlas_logs) |
| Shell | `tail -f .logs/api.log`, or `atlas dev --headless` to stream all services to stdout |

## Searching

Find and filter share one matcher: a lowercase query is case-insensitive, an uppercase letter makes it case-sensitive (*smartcase*), and regex uses the Rust `regex` syntax. MCP search is always case-insensitive and treats a query wrapped in slashes (`/ECONNREFUSED|timeout/`) as a regex.

## JSON logs

In the TUI, `J` turns on JSON compaction: a line that is exactly one JSON object renders as `LEVEL message key=value …`, colored by level. Pino and bunyan numeric levels (`30`, `40`, `50`) and string levels are understood. The raw line is untouched on disk.

## Bursts

The runner never disconnects a slow client. A client that falls behind skips lines and sees an `N lines skipped` marker; the complete output is still in `.logs/`. The desktop app receives output in about 16 ms batches. Rendering cost depends on the visible rows, not on the buffer size.

## Hiding quiet services

`hide_logs: true` on a service removes its tab and keeps its lines out of **All**. The file in `.logs/` is still written. Use it for databases and other chatty infrastructure.
