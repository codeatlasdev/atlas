---
title: "Arquitetura"
source: architecture.md
source_sha: "be07b17284983944ba3a77aefbb07a8080a18c35"
translation: machine
---

# Arquitetura

O Atlas é um workspace Rust mais um frontend SvelteKit. Esta página é para quem lê ou muda o código; para usar o Atlas, veja a [Introdução](introduction.md).

## Crates

Sete crates, todos com `publish = false`, em um único workspace do Cargo (`cargo metadata --no-deps` é a fonte da verdade).

| Caminho | Crate | Papel | Depende de (workspace) |
|---|---|---|---|
| `crates/atlas-core` | `atlas-core` | tipos de domínio compartilhados, caminhos (`~/.atlas`), helpers de releases do GitHub | nenhum |
| `crates/atlas-host` | `atlas-host` | `atlas hostd`: PTYs, exec one-shot, acesso a arquivos e scrollback atrás de um socket unix, mais o cliente e o upgrade ao vivo; o estado dos agentes e o servidor de hooks deles; o gateway do Atlas Mobile (iroh) | `atlas-core`, `atlas-lsp`, `atlas-tui` |
| `crates/atlas-lsp` | `atlas-lsp` | language servers atrás do hostd: registro de servidores e detecção de raiz, instalador verificado (rust-analyzer, gopls, servidores npm, Node.js gerenciado), cliente JSON-RPC, gerenciador por raiz com desligamento por inatividade e reinício após crash | nenhum |
| `crates/atlas-term` | `atlas-term` | o terminal na GPU sem o Tauri: estado VT (sobre o `alacritty_terminal`), o renderizador wgpu/Metal, fontes, imagens inline, codificação de teclas | nenhum |
| `crates/atlas-tui` | `atlas-tui` | uma biblioteca: carregamento e validação da configuração, o runtime (manager, scheduler, health, supervisão de processos, logs), o TUI (ratatui), o runner, o socket de controle e o servidor MCP | `atlas-core` |
| `crates/atlas-skills` | `atlas-skills` | as skills de agente compiladas no binário e o instalador delas | `atlas-core` |
| `crates/atlas-cli` | `atlas-cli` | o binário `atlas`: parsing de argumentos (clap), todos os subcomandos, autoatualização, configuração de agentes | `atlas-core`, `atlas-host`, `atlas-skills`, `atlas-tui` |
| `crates/atlas-desktop` | `atlas-desktop` | o Atlas Desktop: o shell Tauri 2, worktrees, o navegador embutido, a bandeja, a ponte de debug | `atlas-core`, `atlas-host`, `atlas-tui` e, só no macOS, `atlas-term` |

```text
atlas-cli ──▶ atlas-tui ──▶ atlas-core
   ├────────▶ atlas-skills ─▶ atlas-core
   ├────────▶ atlas-host
   └────────▶ atlas-core
atlas-desktop ──▶ atlas-tui, atlas-core, atlas-host
              └─▶ atlas-term   (macOS only)
```

Outros diretórios de nível superior:

| Caminho | O quê |
|---|---|
| `apps/desktop` | o frontend do desktop (SvelteKit 3, com o `@codeatlasdev/ui`) |
| `apps/mobile` | o app Atlas Mobile (Flutter, iOS e Android); veja [docs/mobile.md](mobile.md) |
| `skills/` | skills em markdown, embutidas pelo `atlas-skills/build.rs` |
| `docs/` | estas docs; o site as renderiza diretamente |
| `site/` | o site (Astro); veja [O site](site.md) |
| `scripts/` | scripts de setup, release, smoke e apoio ao CI |
| `demo/` | a stack de demonstração (`atlas.yaml`, `demo.tape`) usada pelo `scripts/smoke.sh` e pelas imagens do site |
| `assets/` | a marca: o `logo.svg` é a fonte única do símbolo, `templates/` reúne os designs de ícone do app, bandeja e animados; todo o resto aqui é gerado por `scripts/icons.ts` ([Assets da marca](contributing.md#ativos-de-marca)) |
| `testdata/` | fixtures compartilhadas pelos testes em Rust e em TypeScript (níveis de log) |

O `atlas-cli` é o único crate que produz o binário `atlas`. Compilar só o `atlas-tui` dá uma biblioteca. O `atlas-desktop` e o `atlas-term` existem só no macOS: os `default-members` do workspace os deixam de fora, então um `cargo build`/`cargo test` simples funciona no Linux, e `--workspace` compila tudo em um Mac.

## Processos e sockets

```text
                           ┌──────────────────────────────┐
  you ── atlas dev ───────▶│ runner (services, scheduler, │◀── atlas mcp ◀── AI agent
  (TUI attaches)           │ health, disk logs)           │
  Atlas Desktop ──────────▶│ .logs/atlas.sock             │
                           └──────────────────────────────┘
                                          │ spawns (sh -c, setsid)
                                          ▼
                                   your services

  Atlas Desktop ──▶ atlas hostd ──▶ PTYs (terminals, agents), exec, fs, scrollback
                    ~/.atlas/hostd.sock
  atlas wt|term|browser|file ──▶ ~/.atlas/desktop.sock ──▶ Atlas Desktop
  agent hooks ──▶ loopback HTTP server in Atlas Desktop (token per run)
```

| Endpoint | Servido por | Usado por |
|---|---|---|
| `<project>/.logs/atlas.sock` (0600) | o runner ou o TUI que controla o projeto | TUIs anexados, Atlas Desktop, `atlas mcp`, `atlas down` |
| `~/.atlas/hostd.sock` + `hostd.token` | `atlas hostd` | Atlas Desktop, `atlas term` sem o app |
| `~/.atlas/desktop.sock` | Atlas Desktop | `atlas wt`, `atlas term`, `atlas browser`, `atlas file` |
| `~/.atlas/desktop-debug.sock` | só builds de debug do Atlas Desktop | `atlas desktop debug …` |
| HTTP em loopback, `~/.atlas/desktop/hook-endpoint.env` | `atlas hostd` (`atlas_host::agents`) | `atlas hook`, a extensão `atlas-status` do omp |

### O runner

O `atlas-tui/src/runtime/manager` contém o `ServiceManager`: o scheduler (grafo de dependências e condições), o spawner, os health checks, a política de reinício, as ações, o túnel e o gravador de logs em disco. O mesmo manager controla o TUI, o runner em segundo plano (`--detach`), o `--headless` e o `--ci`. Um TUI anexado roda um *espelho* do manager, alimentado pelo fluxo de eventos do runner em vez de iniciar qualquer coisa. O protocolo de comunicação está documentado em [Runtime › Runner e attach](runtime.md#runner-e-attach).

### MCP

O `atlas-tui/src/mcp` é um servidor MCP stdio escrito à mão (JSON-RPC 2.0). As ferramentas de leitura trabalham a partir dos arquivos em `.logs/` e do `atlas.yaml`; as ferramentas de controle passam pelo socket de controle. Veja [MCP](mcp.md).

### hostd

O `atlas hostd` sobrevive ao app, então os terminais continuam depois de você fechar, de um crash ou de uma atualização. Depois de uma atualização do app, o daemon fica mais velho que o app; daemons com protocolo 2+ são trocados ao vivo (o novo processo recebe todos os masters de PTY por `SCM_RIGHTS`, junto com o estado das sessões e o lock). Detalhes em [Desktop › Arquitetura](desktop.md#arquitetura).

Os language servers também rodam dentro do hostd (`atlas-lsp`), então os editores em projetos por SSH ganham os mesmos recursos de IDE dos locais. Os comandos `lsp_*` abrem documentos e encaminham requisições LSP; diagnósticos, status e `workspace/applyEdit` voltam como pushes. Os servidores iniciam na primeira abertura de uma linguagem, são compartilhados por (servidor, raiz do projeto), param 5 minutos depois que o último documento fecha e reiniciam após um crash (no máximo 3 vezes por minuto). Um servidor que já está no `PATH` (rustup, `~/go/bin`) tem prioridade; senão, ele é baixado uma vez para `~/.atlas/lsp/<server>/<version>/`, com o checksum verificado (rust-analyzer das releases do GitHub, gopls por `go install`, o resto por npm com um Node.js gerenciado quando o do sistema falta ou é antigo demais). Logs: `~/.atlas/lsp/logs/<server>.log`. Um `upgrade` ao vivo não transfere os language servers: eles terminam junto com o daemon antigo, e o app reabre os documentos depois de reconectar.

### O terminal GPU

```text
program ─pty─▶ atlas hostd ──raw bytes──▶ reader task ─▶ Vt (alacritty_terminal + atlas scanner)
                                                              │ snapshot (Frame)
                                  native view over the pane ◀─┴─ render thread (wgpu / Metal)
```

Veja [Terminal GPU](terminal.md#arquitetura).

## O app desktop

O `crates/atlas-desktop` é um shell Tauri 2 com webviews filhas (a feature `unstable`), o que torna a janela principal multi-webview: as operações de janela usam `get_window("main")`, e a webview do próprio app é `get_webview("main")`. O frontend em `apps/desktop` é uma SPA SvelteKit; os stores dela (`workspace`, `worktrees`, `agents`, `browser`…) são a fonte única do estado da interface, e o lado Rust expõe comandos (`projects_list`, `stack_start`, `term_*`, …) por IPC do Tauri.

## O pipeline de logs

A saída dos serviços passa por três lugares que precisam concordar sobre o que é uma linha:

1. **Runtime em Rust** (`atlas-tui/src/runtime/logs.rs`, `logindex.rs`, `loglinks.rs`): o ring buffer, `LogLevel::detect` (tokens de palavra inteira `error|err|fatal|panic|warn|warning|trace|debug`, que não vêm depois de `/ . - _`), `strip_ansi`, links, pesquisa e saltos por nível. Alimenta o TUI e os logs em disco.
2. **IPC do desktop** (`atlas-desktop/src/live.rs`): transmite as entradas, com o nível do runner, para a webview.
3. **Frontend** (`apps/desktop/src/lib/logs.ts`, `ansi.ts`, `Logs.svelte`): mantém em buffer, filtra e renderiza. O `levelOf` segue a mesma regra de tokens de palavra, usada só quando um runner mais antigo não envia nível.

A regra de nível existe em Rust e em TypeScript, então as duas são testadas contra uma única tabela, `testdata/log-levels.json`. Mover a lógica de texto compartilhada para um único crate está no [roadmap](https://github.com/codeatlasdev/atlas/blob/main/ROADMAP.md).

## Dados em disco

Veja [Onde o Atlas mantém as coisas](concepts.md#onde-o-atlas-mantém-as-coisas) e [Arquivos em `.logs/`](runtime.md#arquivos-em-logs).

## Regras de segurança em que o código se apoia

Elas vêm de incidentes reais e são garantidas por testes; mantenha-as quando mudar o runtime:

- A limpeza de órfãos usa só o `.logs/.pids` e PIDs **exatos**. Ela nunca mata por porta (`lsof`) e nunca usa PIDs negativos (kills de grupo de processos).
- O Atlas nunca roda `docker compose down` nem reinicia o daemon do docker.
- Os filhos rodam com `setsid`, `stdin(null)` e `sh -c`.
- O TUI lê a entrada de um fluxo de eventos, renderiza só quando há mudança e nunca bloqueia a inicialização.
- Nada secreto é carregado no navegador embutido; os resultados do Design Mode são lidos de volta com `eval_with_callback`.
