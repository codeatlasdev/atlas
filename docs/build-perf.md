---
title: "Build performance"
description: "Rust build profiles, Cargo config, test runner and target/ hygiene, with measured numbers."
---

# Build performance

How the Rust workspace builds fast and keeps `target/` small, what was measured, and what was rejected. Everything here is stable Rust (the toolchain pinned in `rust-toolchain.toml`); nothing needs nightly.

## Before / after

M4 Pro (12 cores, 24 GB), Rust 1.99.0, macOS, 2026-10-14, a separate `CARGO_TARGET_DIR`, nothing else compiling.

| Metric | Before | After |
|---|---|---|
| clean `cargo build --workspace` | 82 s, peak RSS 1.27 GB | **49 s**, peak RSS 1.16 GB |
| `target/` after the clean build | 3.5 G | **2.3 G** |
| incremental: touch `atlas-core/src/lib.rs` | 15 s | **11 s** |
| incremental: touch a leaf (`atlas-cli/src/main.rs`) | 1 s | 2 s |
| incremental: touch `atlas-desktop/src/main.rs` | 11 s | **6 s** |
| `cargo check --workspace` after touching core | 10 s | **4 s** |
| test build (`cargo test --no-run` before, `cargo nextest run --no-run` after) | 18 s | **12 s** |
| `target/` after build + test build | 6.2 G | **3.2 G** |

Before, the critical path of a clean build was: `aws-lc-sys` build script 42 s (C + cmake, pulled only by `reqwest` → `rustls` → `aws-lc-rs`), `objc2-foundation` 34 s, `naga` 29 s, `tauri-utils` 24 + 21 s (host and target), `read-fonts` 24 s, `objc2-app-kit` 17 s, `wgpu-core` 15 s. Duplicate versions in the graph: `syn` 2/3, `thiserror` 1/2, `toml_*`, `derive_more`.

## How to measure

```bash
cargo clean
/usr/bin/time -l cargo build --workspace --timings   # wall time, peak RSS; report in target/cargo-timings/
du -sh target
touch crates/atlas-core/src/lib.rs && time cargo build --workspace
touch crates/atlas-cli/src/main.rs && time cargo build --workspace
touch crates/atlas-desktop/src/main.rs && time cargo build --workspace
touch crates/atlas-core/src/lib.rs && time cargo check --workspace
time cargo nextest run --workspace --no-run
```

Measure with nothing else compiling (a concurrent `cargo install` turned 49 s into 83 s).

## Daily flow

| Command | Use |
|---|---|
| `cargo check` | fastest feedback |
| `cargo lint` | clippy, same flags as CI |
| `cargo t && cargo t-doc` | tests (nextest) and doctests |
| `bun run check` | the gate, before a push |
| `bun run sweep` | free `target/build` when it grows (`--dry-run` for sizes) |
| `cargo build --profile debug` | full debug info for lldb sessions |

## Adopted

| Lever | Where | Effect |
|---|---|---|
| `ring` instead of `aws-lc-rs` for rustls | `crates/atlas-tui/Cargo.toml`, provider installed in `runtime/health.rs` | removes the 42 s C/cmake build script |
| `debug = "line-tables-only"` in dev | root `Cargo.toml` | backtraces keep file:line; uv measured −20 % on the warm loop |
| no debug info for deps and build scripts | `[profile.dev.package."*"]`, `[profile.dev.build-override]` | with the line above: 82 → 49 s, 3.5 → 2.3 G |
| `[profile.debug]` | root `Cargo.toml` | full debug info only when asked |
| `build.build-dir = target/build` | `.cargo/config.toml` | intermediates in one deletable directory; final binaries stay in `target/debug`, `target/release` |
| cargo-nextest | `bun run check`, CI, `.config/nextest.toml` | process per test, 60 s slow-timeout kills hangs; test build 18 → 12 s |
| aliases `lint`, `t`, `t-doc` | `.cargo/config.toml` | one spelling for humans, agents and CI |

## Rejected

| Lever | Why |
|---|---|
| alternative macOS linker (lld, sold, wild) | ld64 is already fast (Bevy, perf-book, Zed); sold is unmaintained; wild has no usable Mach-O |
| `split-debuginfo = "unpacked"` | already Cargo's macOS default |
| `[profile.dev] opt-level = 1` | slows the edit loop |
| `panic = "abort"` | the TUI and the test harness need unwinding |
| sccache locally | no gain with incremental; maybe a later CI experiment |
| cargo-sweep | unmaintained; `bun run sweep` deletes `target/build` instead |
| per-dep `opt-level = 3` for proc-macros | no measured need; revisit if `--timings` shows macro expansion on the critical path |
| `debug = "line-tables-only"` in release | not adopted: on macOS no `.dSYM` is produced, so the line tables stay in the object files under `target/`, not in the shipped binary (a short-format backtrace of a throwaway binary copied out of `target/` showed no file:line either way; not re-checked with `RUST_BACKTRACE=full`). Shipping a `.dSYM` is a packaging change, not evaluated here |

## Editor

Leave `rust-analyzer.cargo.targetDir` unset so rust-analyzer shares `target/`: a second tree costs gigabytes, the lock contention costs seconds. Clippy on save is optional (`"rust-analyzer.check.command": "clippy"` in VS Code settings, `"check": { "command": "clippy" }` under `lsp.rust-analyzer.initialization_options` in Zed).

macOS: `sudo spctl developer-mode enable-terminal`, then add your terminal under System Settings → Privacy & Security → Developer Tools. Gatekeeper then skips the check on every freshly built binary.

## target/ hygiene

Cargo has no `target/` GC ([cargo#5026](https://github.com/rust-lang/cargo/issues/5026)). `bun run sweep` deletes `target/build` and `target/rust-analyzer`; `scripts/setup.sh` warns above 20 GiB. A `target/` from before this layout holds dead `target/debug/deps`: run `rm -rf target` once.

## Watch list (nightly or not shipped)

| Item | Revisit when |
|---|---|
| build-dir layout v2 (Cargo 1.100, 2026-11-12) | bump the toolchain, re-measure, check `Swatinem/rust-cache` still restores, evaluate sharing one build-dir across the `*-worktrees/` checkouts |
| `cargo clean` GC ([cargo#5026](https://github.com/rust-lang/cargo/issues/5026)) | stabilized → replace `bun run sweep` |
| `feature-unification = "workspace"` | stabilized → ends the rebuilds between `cargo build` (default-members) and `cargo build --workspace` |
| parallel frontend (`-Zthreads`, `--jobs-frontend`) | stabilized; try now with `cargo +nightly build -Zunstable-options --jobs-frontend=4` |
| cranelift backend | unwinding on macOS and objc2 statics ([rustc_codegen_cranelift#1689](https://github.com/rust-lang/rustc_codegen_cranelift/issues/1689)) fixed |
| `hint-mostly-unused` for `objc2-*` | stabilized |
| relink-don't-rebuild, TPDE, wild for Mach-O | shipped for macOS |
