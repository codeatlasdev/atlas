---
title: "Desempenho do build"
source: build-perf.md
source_sha: "61acde51e9782685ba40f20106f973a5a66f7a67"
translation: machine
---

# Desempenho do build

Como o workspace Rust compila rápido e mantém o `target/` pequeno, o que foi medido e o que foi rejeitado. Tudo aqui é Rust estável (o toolchain fixado em `rust-toolchain.toml`); nada exige nightly.

## Antes e depois

M4 Pro (12 núcleos, 24 GB), Rust 1.99.0, macOS, 14/10/2026, um `CARGO_TARGET_DIR` separado, nada mais compilando.

| Métrica | Antes | Depois |
|---|---|---|
| `cargo build --workspace` limpo | 82 s, pico de RSS de 1,27 GB | **49 s**, pico de RSS de 1,16 GB |
| `target/` depois do build limpo | 3,5 G | **2,3 G** |
| incremental: tocar `atlas-core/src/lib.rs` | 15 s | **11 s** |
| incremental: tocar uma folha (`atlas-cli/src/main.rs`) | 1 s | 2 s |
| incremental: tocar `atlas-desktop/src/main.rs` | 11 s | **6 s** |
| `cargo check --workspace` depois de tocar o core | 10 s | **4 s** |
| build dos testes (`cargo test --no-run` antes, `cargo nextest run --no-run` depois) | 18 s | **12 s** |
| `target/` depois do build + build dos testes | 6,2 G | **3,2 G** |

Antes, o caminho crítico de um build limpo era: o build script do `aws-lc-sys` com 42 s (C + cmake, puxado só por `reqwest` → `rustls` → `aws-lc-rs`), `objc2-foundation` 34 s, `naga` 29 s, `tauri-utils` 24 + 21 s (host e target), `read-fonts` 24 s, `objc2-app-kit` 17 s, `wgpu-core` 15 s. Versões duplicadas no grafo: `syn` 2/3, `thiserror` 1/2, `toml_*`, `derive_more`.

## Como medir

```bash
cargo clean
/usr/bin/time -l cargo build --workspace --timings   # tempo de parede, pico de RSS; relatório em target/cargo-timings/
du -sh target
touch crates/atlas-core/src/lib.rs && time cargo build --workspace
touch crates/atlas-cli/src/main.rs && time cargo build --workspace
touch crates/atlas-desktop/src/main.rs && time cargo build --workspace
touch crates/atlas-core/src/lib.rs && time cargo check --workspace
time cargo nextest run --workspace --no-run
```

Meça com nada mais compilando (um `cargo install` simultâneo transformou 49 s em 83 s).

## Fluxo do dia a dia

| Comando | Uso |
|---|---|
| `cargo check` | o feedback mais rápido |
| `cargo lint` | clippy, com as mesmas flags do CI |
| `cargo t && cargo t-doc` | testes (nextest) e doctests |
| `bun run check` | o gate, antes de um push |
| `bun run sweep` | libera o `target/build` quando ele cresce (`--dry-run` mostra os tamanhos) |
| `cargo build --profile debug` | informações de debug completas para sessões do lldb |

## Adotado

| Ajuste | Onde | Efeito |
|---|---|---|
| `ring` no lugar de `aws-lc-rs` para o rustls | `crates/atlas-tui/Cargo.toml`, provider instalado em `runtime/health.rs` | remove o build script de C/cmake de 42 s |
| `debug = "line-tables-only"` em dev | `Cargo.toml` da raiz | os backtraces mantêm arquivo:linha; o uv mediu −20 % no ciclo com cache quente |
| sem informações de debug para dependências e build scripts | `[profile.dev.package."*"]`, `[profile.dev.build-override]` | junto com a linha acima: 82 → 49 s, 3,5 → 2,3 G |
| `[profile.debug]` | `Cargo.toml` da raiz | informações de debug completas só quando você pede |
| `build.build-dir = target/build` | `.cargo/config.toml` | os intermediários ficam em um diretório que dá para apagar; os binários finais continuam em `target/debug` e `target/release` |
| cargo-nextest | `bun run check`, CI, `.config/nextest.toml` | um processo por teste, e o slow-timeout de 60 s mata os que travam; build dos testes de 18 → 12 s |
| aliases `lint`, `t`, `t-doc` | `.cargo/config.toml` | uma única grafia para pessoas, agentes e CI |

## Rejeitado

| Ajuste | Por quê |
|---|---|
| linker alternativo no macOS (lld, sold, wild) | o ld64 já é rápido (Bevy, perf-book, Zed); o sold não tem manutenção; o wild não tem Mach-O utilizável |
| `split-debuginfo = "unpacked"` | já é o padrão do Cargo no macOS |
| `[profile.dev] opt-level = 1` | deixa o ciclo de edição mais lento |
| `panic = "abort"` | o TUI e o harness de testes precisam de unwinding |
| sccache localmente | sem ganho com build incremental; talvez um experimento futuro no CI |
| cargo-sweep | sem manutenção; o `bun run sweep` apaga o `target/build` no lugar dele |
| `opt-level = 3` por dependência para proc-macros | sem necessidade medida; reveja se o `--timings` mostrar a expansão de macros no caminho crítico |
| `debug = "line-tables-only"` em release | adotado (#318) para símbolos de crash: o `[profile.release]` mantém as tabelas de linhas, então um backtrace de release tem arquivo:linha e o `scripts/symbols.sh extract` consegue publicar o arquivo de debug de cada binário de release (docs/release.md, "Symbols"). No macOS, o workflow de release define `split-debuginfo=packed`, então o `.dSYM` fica em um arquivo separado. Os binários distribuídos voltam a ser stripped logo depois do build (`scripts/symbols.sh split`), então só o arquivo intermediário em `target/` fica maior; os tamanhos distribuídos estão no log do workflow de release (etapas `Symbols`) e em #470 |

## Editor

Deixe `rust-analyzer.cargo.targetDir` sem valor para o rust-analyzer compartilhar o `target/`: uma segunda árvore custa gigabytes, e a disputa pelo lock custa segundos. Rodar o clippy ao salvar é opcional (`"rust-analyzer.check.command": "clippy"` nas configurações do VS Code, `"check": { "command": "clippy" }` em `lsp.rust-analyzer.initialization_options` no Zed).

macOS: rode `sudo spctl developer-mode enable-terminal` e adicione o seu terminal em Ajustes do Sistema → Privacidade e Segurança → Ferramentas de Desenvolvedor. A partir daí, o Gatekeeper pula a verificação em todo binário recém-compilado.

## Higiene do target/

O Cargo não tem GC para o `target/` ([cargo#5026](https://github.com/rust-lang/cargo/issues/5026)). O `bun run sweep` apaga `target/build` e `target/rust-analyzer`; o `scripts/setup.sh` avisa quando passa de 20 GiB. Um `target/` de antes deste layout contém um `target/debug/deps` morto: rode `rm -rf target` uma vez.

## Lista de observação (nightly ou não lançado)

| Item | Reveja quando |
|---|---|
| build-dir layout v2 (Cargo 1.100, 12/11/2026) | atualizar o toolchain, medir de novo, conferir se o `Swatinem/rust-cache` ainda restaura e avaliar compartilhar um build-dir entre os checkouts de `*-worktrees/` |
| GC do `cargo clean` ([cargo#5026](https://github.com/rust-lang/cargo/issues/5026)) | for estabilizado → substituir o `bun run sweep` |
| `feature-unification = "workspace"` | for estabilizado → acaba com os rebuilds entre `cargo build` (default-members) e `cargo build --workspace` |
| frontend paralelo (`-Zthreads`, `--jobs-frontend`) | estabilizado; teste agora com `cargo +nightly build -Zunstable-options --jobs-frontend=4` |
| backend cranelift | o unwinding no macOS e os statics do objc2 ([rustc_codegen_cranelift#1689](https://github.com/rust-lang/rustc_codegen_cranelift/issues/1689)) forem corrigidos |
| `hint-mostly-unused` para `objc2-*` | for estabilizado |
| relink-don't-rebuild, TPDE, wild para Mach-O | forem lançados para macOS |
