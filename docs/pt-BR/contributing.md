---
title: "Contribuir"
source: contributing.md
source_sha: "368d62ae950b4f170c8e9146b7084db26f7d6445"
translation: machine
---

# Contribuir

Obrigado por ajudar o Atlas. Esta página é a versão longa do [CONTRIBUTING.md](https://github.com/codeatlasdev/atlas/blob/main/CONTRIBUTING.md). Ao participar, você concorda com o [Código de Conduta](https://github.com/codeatlasdev/atlas/blob/main/CODE_OF_CONDUCT.md). O Atlas é distribuído sob a licença [MIT](https://github.com/codeatlasdev/atlas/blob/main/LICENSE-MIT) ou [Apache-2.0](https://github.com/codeatlasdev/atlas/blob/main/LICENSE-APACHE), à sua escolha, e as suas contribuições também.

## Formas de ajudar

- **Relate um bug** ou peça uma funcionalidade no [issue tracker](https://github.com/codeatlasdev/atlas/issues). Inclua `atlas version`, o seu sistema operacional, os passos e as linhas relevantes de `.logs/<service>.log` (oculte segredos).
- **Melhore a documentação.** Toda página do site tem um link *Editar no GitHub*; o código-fonte está em `docs/*.md`.
- **Envie uma correção ou uma funcionalidade.** Para qualquer coisa maior que uma correção pequena, abra uma issue antes, para combinarmos o formato antes de você gastar tempo nela.
- **Problemas de segurança** não vão em issues públicas. Veja [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md).

> **Assine seus commits.** O Atlas usa o [DCO](https://developercertificate.org/): faça commit com `git commit -s`. Alterações feitas com ajuda de IA seguem a política do [CONTRIBUTING.md](https://github.com/codeatlasdev/atlas/blob/main/CONTRIBUTING.md#ai-assisted-contributions).

## Configurar o ambiente

Em todos os sistemas você precisa de `rustup`, [`bun`](https://bun.sh) e Python 3 com PyYAML (as verificações de release do `bun run check`), além das peças de cada sistema abaixo. O `gh` é opcional.

```bash
git clone https://github.com/codeatlasdev/atlas.git
cd atlas
bun run setup           # idempotente: verifica as ferramentas, instala as dependências do frontend, cargo-tauri, cargo-deny, cargo-nextest, actionlint e zizmor
```

A toolchain do Rust é fixada em `rust-toolchain.toml`; o `rustup` a instala no primeiro comando `cargo`. Os lints do workspace valem para todos os crates: corrija os avisos, não adicione `allow`s globais. Perfis de build, os aliases `cargo lint`/`cargo t`, o `bun run sweep` e os tempos de build medidos: [Desempenho de build](build-perf.md).

### macOS

As ferramentas de linha de comando do Xcode (`xcode-select --install`). Tudo nesta página funciona, incluindo instâncias de agente, o Atlas Lab e o terminal GPU nativo.

### Linux

WebKitGTK 4.1 e as suas dependências de build para o Atlas Desktop (o `bun run setup` as nomeia quando faltam; só a CLI não precisa de nenhuma):

```bash
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libappindicator-gtk3 librsvg xdotool   # Arch
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev   # Debian/Ubuntu
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel libxdo-devel   # Fedora
```

Os terminais usam xterm.js. As instâncias de agente (`bun run desktop:agent`) rodam em um sway headless próprio e em um barramento de sessão D-Bus próprio, então precisam de `sway`, `grim` e `dbus-daemon` (Arch: `pacman -S sway grim`; Debian/Ubuntu: `apt install sway grim dbus`); a gravação também precisa de `ffmpeg` (com libx264). O Atlas Lab, o espelho, o `debug shot` e o `debug record` funcionam nelas (veja "Lacunas no Linux e no Windows" abaixo). O roadmap do Linux é a #289.

### Windows

Instale o [Git for Windows](https://git-scm.com/download/win) (ele traz o Git Bash), o [rustup](https://rustup.rs) com a toolchain MSVC (Visual Studio Build Tools, "Desktop development with C++"), o [Bun](https://bun.sh) e o [Python 3](https://www.python.org/downloads/) do python.org, não o alias da Microsoft Store para o qual `python3` aponta por padrão. O WebView2 vem com o Windows 10 e o 11.

Os scripts `bun run` chamam os scripts de shell do repositório por `bun scripts/sh.ts`, que os roda com o Git Bash: no Windows, o primeiro `bash` no `PATH` costuma ser o do WSL (`WindowsApps\bash.exe`), que os rodaria dentro do Linux, onde o seu Rust e o seu Bun do Windows não existem. Chame um script manualmente do mesmo jeito: `bun scripts/sh.ts scripts/heavy.sh cargo build`. O `ATLAS_BASH` aponta para outro bash. Verificados no Windows 11: `bun run setup`, `bun run check:changed`, `bun run lint:workflows` e `bun run check` (rc 0 no 40e8b67, com `usr\bin` do Git no `PATH`). Rode o gate pelo Git Bash, ou ponha `C:\Program Files\Git\usr\bin` no `PATH` do PowerShell: alguns testes de Rust iniciam `sh` e `sleep`, e com o `PATH` de um PowerShell comum eles falham com `program not found`. Ainda não verificados no Windows: `bun run desktop:dev` e `desktop:build`.

Os terminais usam xterm.js no Windows; instâncias de agente, o Atlas Lab e o espelho não existem lá (veja abaixo). Testes de scripts só de macOS ou só de POSIX (`dev-signing-identity`, `install.sh`, o load gate do `heavy.sh`) são ignorados no Windows; o CI roda os scripts do repositório e os testes deles no Linux e no Windows (job "Repo scripts").

## O gate

```bash
bun run check
```

O `bun run check` é o gate local: toda verificação que os jobs obrigatórios do CI rodam e que pode rodar em uma máquina só. A lista vive em um único lugar, `scripts/checks/map.ts` (áreas com globs de caminho e as verificações de cada área); o `bun run check` roda tudo, o `bun run check:changed` roda as áreas que a sua alteração toca, e o job plan do CI (`scripts/ci-plan.ts`) transforma as mesmas áreas nas saídas `area=true|false` que controlam os jobs do `ci.yml`. O `bun run check --json` imprime a lista e as verificações que só o CI consegue rodar (a abertura do desktop no Linux, os testes de root, o build arm64, o instalador do Windows), com o motivo. Em ordem:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets --locked -- -D warnings`
3. `cargo nextest run --workspace --locked`, depois `cargo test --workspace --locked --doc` (o nextest não roda doctests)
4. `cargo deny check` (licenças, avisos de segurança, crates duplicados e banidos)
5. `scripts/check-skills.sh` (valida `skills/*/SKILL.md` contra a especificação de Agent Skills)
6. `scripts/lint-workflows.sh` (actionlint e zizmor sobre `.github/`; também `bun run lint:workflows`)
7. `bun run desktop:check` (svelte-check, prettier e os testes do frontend, `bun run test` em `apps/desktop`: `bun test --conditions browser --parallel src`; um `bun test` puro ali para na hora, porque sem a condição `browser` o Svelte resolve o build de servidor e nenhum componente consegue montar). O `--parallel` roda cada arquivo de teste em um global e um registro de módulos novos (ele implica `--isolate`): mocks de módulo como `test/uiKit.ts` e globais simulados valem para o processo inteiro, então sem ele os mocks de um arquivo vazavam para o arquivo seguinte e os testes passavam ou falhavam conforme a ordem dos arquivos. O `bun run test` na raiz roda esses testes mais o `release:test`.
8. `bun run i18n:check` (traduções: textos ausentes, desatualizados, sem uso, duplicados ou divergentes, texto fixo voltado ao usuário, traduções da documentação; offline, segundos; veja [Traduzir o Atlas](#traduzir-o-atlas))
9. `scripts/mobile-check.sh` (fmt e clippy do crate nativo, depois Flutter analyze e test para `apps/mobile`; ignorado com um aviso quando o Flutter não está instalado)
10. `scripts/push-relay-check.sh` (o push relay em Go, em `services/push-relay`; ignorado com um aviso quando o Go não está instalado)
11. `bun run release:test` (os testes dos próprios scripts do repositório, os testes do manifesto de release em Python, os vetores do canal de bootstrap e as verificações de labels); o CI roda no Linux e no Windows, no job "Repo scripts"
12. `bun run dup` (jscpd, no máximo 1,5% de duplicação)
13. `bun run site:check` e `bun run site:build` (astro check, verificação de tipos e build de `site/`, que também renderiza `docs/`)

Os passos 5, 6, 9 e 10 rodam por `bun scripts/sh.ts`, então usam o Git Bash no Windows (veja [Windows](#windows)).

**Adicionar uma verificação ou ensinar um caminho a ela.** Edite só o `scripts/checks/map.ts`: uma verificação entra na sua área, com os jobs `ci:` que a rodam; um caminho entra nos globs da área que ele afeta. Um caminho que nenhuma área reivindica roda todas as verificações (um caminho desconhecido nunca pula uma), então um diretório novo na raiz custa uma execução completa até ser mapeado. O `scripts/checks/map.test.ts` falha, dizendo qual linha adicionar, quando o `ci.yml` ganha um job, uma área controlada ou um comando de verificação que o mapa não lista, ou quando o mapa lista algo que o CI não roda. Alterações só de documentação (`*.md` da raiz, `LICENSE*`, assets, templates de issue) pulam tudo; o `docs/` ainda recompila o site.

**A base do diff.** O `check:changed` e o job plan do CI compartilham um único resolvedor, `scripts/lib/base.ts`: `--base <ref>` (ou `ATLAS_BASE`) se informado; senão, no CI, o primeiro pai do merge-ref do pull request (a ponta do main com a qual o GitHub fez o merge, não o sha da base que o evento registrou) ou o commit que um push substituiu; senão, localmente, o merge base com `origin/main` (depois `main`). Alterações commitadas, em stage, fora do stage e não rastreadas contam todas. Sem base utilizável (`workflow_dispatch`, uma branch nova, histórico raso), roda tudo: "sem base" nunca significa "nada mudou". Rode `git fetch origin main` antes de verificar localmente; um `origin/main` desatualizado faz os arquivos mais novos do main parecerem seus. Outras ferramentas que julgam uma alteração contra uma base (os ratchets de i18n) importam o `resolveBase` dali.

Rode o gate sozinho e leia o código de saída antes do push. Um pull request deve passar nele.

### Comandos

Todo comando é um script do `package.json` da raiz, então `bun run <nome>` a partir da raiz do repositório é o ponto de entrada único (antes, uma vez, o `scripts/setup.sh`). Esta é a tabela única de comandos; as outras páginas apontam para cá.

| Comando | Uso |
|---|---|
| `bun run setup` | a verificação prévia única: rustup, bun, dependências do frontend, `cargo-tauri`, `cargo-deny`, `cargo-nextest`, `actionlint`, `zizmor` (idempotente; igual ao `scripts/setup.sh`) |
| `bun run check` | **o gate**, acima |
| `bun run sweep` | libera `target/build` (artefatos intermediários; o próximo build os recria), veja [Desempenho de build](build-perf.md) |
| `bun run fmt` | `cargo fmt` e prettier para o frontend do desktop |
| `bun run dup` | duplicação do jscpd em Rust, TypeScript, Svelte, Astro e Dart (o `lib/` do app de celular, sem os bindings gerados); mantenha em 1,5% ou menos (configuração: `.jscpd.json`) |
| `bun run lint:workflows` | actionlint e zizmor sobre `.github/` (também parte do `bun run check`) |
| `bun run pr:merge <pr> [--subject s] [--dry-run] [--json]` | squash merge de maintainer: recusa um PR desatualizado, vermelho ou com título errado, escreve `Closes #N` a partir das issues vinculadas ao PR (`CONTRIBUTING.md#pull-request-flow-and-who-merges`) |
| `bun run desktop:dev` / `desktop:build` / `desktop:build:app` / `desktop:check` / `desktop:link` | roda o app com hot reload, gera o bundle, gera só um `.app` de debug, verifica o frontend, liga o bundle de debug |
| `bun run mobile:check` | `scripts/mobile-check.sh`: Flutter analyze e test para `apps/mobile` (também parte do `bun run check`; ignorado quando o Flutter não existe) |
| `bun run mobile:lab` | valida uma branch no seu celular ao lado do seu Atlas de verdade: um hostd sombra mais o app Atlas Dev, sem nunca tocar na sua instalação (veja [Atlas Mobile](mobile.md), seção "Lab lane") |
| `bun run desktop:lab` | o frontend do desktop no seu navegador com HMR, sustentado por uma instância de agente invisível (veja Atlas Lab abaixo) |
| `bun run desktop:agent` | inicia a sua própria instância de agente do app desktop (veja [AGENTS.md](https://github.com/codeatlasdev/atlas/blob/main/AGENTS.md)) |
| `bun run site:dev` / `site:build` / `site:check` / `site:deploy` | o site: servidor de desenvolvimento, build estático, `astro check` mais verificações de tipos de scripts e funções, deploy na Cloudflare |
| `bun run studio` / `studio:check` | a CLI do estúdio de filmes e as suas verificações (o `studio/` não faz parte do `bun run check`) |
| `bun run icons` / `icons:check` | regenera os ícones do app a partir de `assets/*.svg`; falha quando algo derivado está desatualizado |
| `bun run i18n:extract <path-or-glob>` | move o texto fixo dos componentes Svelte para o catálogo (`--dry-run`, `--json`, `--area`; `--plan` imprime os lotes de migração do [`i18n/MIGRATION.md`](https://github.com/codeatlasdev/atlas/blob/main/i18n/MIGRATION.md); um caminho `.rs` imprime uma lista de trabalho e nunca escreve) |
| `bun run i18n:check` / `i18n:translate` / `i18n:fix` / `i18n:add-locale` | traduções: verifica todos os catálogos (também parte do `bun run check`), rascunha com um modelo as chaves ausentes ou desatualizadas, ordena e refaz o lock, adiciona um idioma (veja [Traduzir o Atlas](#traduzir-o-atlas)) |
| `bun run deps:update` | atualiza as dependências JS em todo lugar (veja [Atualizar dependências JS](#atualizar-dependências-js)) |
| `bun run release <alpha\|beta\|rc\|stable\|patch>` / `release:dry` / `release:publish vX.Y.Z` / `release:test` | só para maintainers: publica uma versão, ensaia a publicação, (re)publica uma tag existente, roda os testes das ferramentas de release (veja [Publicar uma versão](release.md)) |
| `cargo test -p atlas-cli --test mcp` | a suíte de integração do MCP (binário real, execução headless ao vivo; precisa de `python3`) |
| `cargo test -p atlas-tui --release -- --ignored --nocapture bench_one_million` | o benchmark do pipeline de logs |
| `cargo test -p atlas-term --release -- --ignored --nocapture screen_bench` | o benchmark do renderizador do terminal |

Os testes escutam só no loopback (`127.0.0.1`, `[::1]`), nunca em `0.0.0.0` ou `[::]`: com o firewall de aplicativos ligado, o macOS pergunta "Do you want the application … to accept incoming network connections?" para cada binário de teste novo que escuta na rede, e cada build é um binário novo. Os testes do Atlas Mobile definem `ServeOpts::mobile_loopback` (`ATLAS_MOBILE_LOOPBACK=1` para um `atlas hostd` iniciado por eles) e vinculam os celulares deles com `mobile::loopback_only`; eles verificam que os sockets abertos são de loopback.

### Testes que precisam de root (somente Linux)

Os testes do limite de memória (`crew/spawn/cgroup.rs`: uma folha recebe os seus limites dentro das configurações de cgroup da própria unidade do helper, e um agente acima do limite é morto por OOM sozinho) rodam o binário de teste de novo em uma unidade transitória do `systemd-run` com `Delegate=memory pids`, `ProtectControlGroups=yes` e o único caminho de cgroup gravável. Eles precisam de root e de systemd como PID 1 (um runner do GitHub tem os dois); em outros ambientes eles imprimem o motivo e passam. Dois outros testes criam um usuário de sistema: `crates/atlas-host/src/crew/spawn/exec.rs` (`a_real_spawn_runs_as_the_crewmates_user_and_hostd_cannot_touch_it`: o processo filho roda com aquele uid e outro usuário não consegue ler o diretório dele) e `crew/spawn/provision.rs` (`a_real_provision_creates_the_user_and_modes_and_a_real_deprovision_removes_them`: `useradd`, `chown`, modos e `userdel` de verdade). Ambos têm `#[ignore]` com o motivo, então o `bun run check` e os jobs normais do CI os pulam. O job do CI **Rust (Linux, root tests)** os roda em um runner descartável (parte do agregado `CI`):

```sh
sudo -E env "PATH=$PATH" cargo test -p atlas-host --lib crew::spawn -- --ignored
```

Para rodá-los você mesmo, use uma máquina Linux ou uma VM descartável, nunca o seu Mac (neste caso eles são excluídos da compilação) e nunca um contêiner que compartilhe o banco de usuários do host. Eles adicionam `atlas-cm-ffffffffffff` e `atlas-cm-eeeeeeeeeeee` e os removem ao terminar.

As regras de egress (`nft`) são uma string pura, testada aqui quanto à ordem e ao conteúdo; aplicá-las, os instaladores de engine e o resto de um crew server em um systemd de verdade são cobertos pelo teste end-to-end do crew ([Atlas Crew](crew.md), seção "End-to-end test on real systemd").

### Atualizar dependências JS

O `bun run deps:update` roda `npm-check-updates -u` na raiz, em `apps/desktop`, em `site` e em `studio` (configuração: `.ncurc.json`) e depois reinstala. O `typescript` é rejeitado de propósito: ele fica na 6.x porque o compilador nativo do TypeScript 7 abandona a API JS da qual `svelte-kit sync`, `svelte-check` e `astro check` dependem. Atualize-o manualmente quando essas ferramentas funcionarem com a 7. O `studio` fixa o `hyperframes` em uma versão exata; depois de uma atualização, rode `bun studio check` e mantenha a versão antiga se ele falhar.

## Compilar e rodar suas alterações

```bash
cargo build -p atlas-cli                   # gera o binário `atlas` (o atlas-tui sozinho é só uma biblioteca)
codesign -f -s - target/debug/atlas        # macOS: assine ad hoc, senão o Gatekeeper o mata
atlas link                                 # liga ~/.atlas/bin/atlas ao build de debug por symlink
atlas unlink                               # restaura o binário de release
```

Para instalar o build em vez de ligá-lo: `cargo build -p atlas-cli && codesign -f -s - target/debug/atlas && cp target/debug/atlas ~/.atlas/bin/atlas`. Verifique com `strings ~/.atlas/bin/atlas | grep <version>`. Para o app desktop, use `bun run desktop:build:app` (só o `.app` de debug); nunca `desktop:build -- --debug`, cuja etapa de DMG monta uma imagem e abre o Finder na sua tela.

### Desenvolver o app desktop

```bash
bun run desktop:dev        # hot reload: Vite + o shell do Tauri; mudanças em Rust recompilam
atlas link --desktop       # faz de /Applications/Atlas.app o seu bundle de debug
```

Enquanto estiver ligado, o app mostra um selo **Dev build** e se recusa a atualizar por cima do seu build.

Os builds de debug rodam uma ponte em `~/.atlas/desktop-debug.sock`, para que você (ou um agente de IA) inspecione o app sem o mouse:

```bash
atlas desktop debug info
atlas desktop debug logs --level csp        # console, erros, violações de CSP
atlas desktop debug eval 'return await window.__TAURI_INTERNALS__.invoke("projects_list")'
atlas desktop debug shot /tmp/x.png
```

Os builds de debug também expõem os stores ao vivo como `window.__atlas` (`ws`, `store`, `agents`, `worktrees`, `api`, `host`); use-os a partir do `debug eval` em vez de `import()`, que cria instâncias de módulo separadas. Verifique CSP e IPC no build **empacotado** (`tauri://localhost`), não só no Vite. Depois de um hot reload, recarregue a webview antes de confiar no comportamento dos sockets.

### Desenvolver o Atlas com o Atlas

O repositório tem o seu próprio `atlas.yaml` (portas 13450 a 13459). O `atlas dev` entrega o Lab (`l` abre http://localhost:13450) e o site de documentação (`w`, porta 13451), ambos iniciados sob demanda, além de ações para o gate (`c`), a formatação (`F`), os testes do desktop (`d`), o build do app de debug (`b`), uma instância de agente (`A`), a varredura de flicker (`x`), os benchmarks de seleção e de gestos (`B`, `H`), os ícones de desenvolvimento (`I`) e um ensaio de release (`Y`). Os agentes chegam às mesmas coisas pelo MCP (`atlas_start_service lab`, `atlas_run_action c`, `atlas_logs_search`), então ninguém precisa lembrar qual script faz o quê.

O CI faz o mesmo por job: o job plan mapeia os caminhos alterados para áreas com o `scripts/checks/map.ts` (veja [O gate](#o-gate)), então um commit que só toca markdown na raiz não roda nenhum job (o `docs/` ainda recompila o site), uma alteração em `.github/workflows` ou `.github/actions` roda todos os jobs de novo, e qualquer caminho que o mapa não conhece roda tudo.

**Um runner perdido ou ausente é rodado de novo, não depurado.** Os runners hospedados do GitHub às vezes derrubam um job sozinhos (issue #520), com uma anotação e sem log: um runner do Windows morre no meio de um passo e, uns 45 min depois, o job termina como falha com "The hosted runner lost communication with the server"; ou nenhum runner pega o job (capacidade do macOS arm64, #547) e ele termina cancelado com "The job was not acquired by Runner of type hosted even after multiple attempts". O `.github/workflows/ci-rerun-lost.yml` (em `workflow_run`, CI concluído com falha) roda de novo os jobs que falharam quando todo job que falhou ou foi cancelado na execução (fora o job agregado `CI`) carrega só uma dessas anotações, uma vez por tentativa até a tentativa 3, e só enquanto o head da execução ainda é a ponta da sua branch (o `scripts/ci-lost-runner.ts` tem as regras e o `ci-lost-runner.test.ts` a tabela). Uma falha real, ou um job cancelado sem essa anotação (à mão, por um push mais novo), significa que não há nova execução. O `bun run pr:merge` diz "runner lost, rerun pending" ou "no runner was available, rerun pending" em vez de "fix it and push" enquanto essa nova execução está pendente, e manda você rodar `gh run rerun --failed` à mão quando as três tentativas terminaram assim.

Durante a edição, o `bun run check:changed` (ação `u`) roda só o que as suas alterações podem afetar, a partir do mesmo mapa do gate: os crates alterados e todo crate que depende deles, mais as verificações de desktop, mobile, scripts, skills, site, duplicação ou workflows que os caminhos tocam (`--base <ref>`, por padrão o merge base com `origin/main`). O `--dry-run` lista os comandos e por que cada um roda, sem rodá-los; o `--json` imprime o plano como JSON (`bun run check:changed --dry-run --json`): a base, os arquivos alterados, quais áreas estão ligadas e por quê, os comandos e as verificações que só o CI consegue rodar. O `bun scripts/check-changed.ts --json --stdin` lê os arquivos alterados da entrada padrão em vez do git: ele é o plugin `affected:` do `atlas.yaml` deste repositório, então os agentes recebem o plano pela ferramenta MCP `atlas_affected` (`tasks`, `tests`, `routes` e `builds` são o contrato dela). O `bun run check` continua sendo o gate antes de um push.

**Lint do Windows a partir do Linux ou do macOS.** Com o [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin) instalado, o `check:changed` também roda o clippy para `x86_64-pc-windows-msvc` nos mesmos crates (em `target/xwin`, para o cache de build do host ficar intacto), pegando quebras de `cfg(windows)` e código que só está morto no Windows antes do CI (a #326 deixou passar um). Configure uma vez: Arch `sudo pacman -S --needed clang lld`, Debian/Ubuntu `sudo apt install clang lld`, Fedora `sudo dnf install clang lld`, depois `cargo install cargo-xwin --locked` e `rustup target add x86_64-pc-windows-msvc`. A primeira execução baixa o CRT do MSVC e o SDK do Windows (vale a licença da Microsoft). O `--no-windows` pula esse passo; sem o cargo-xwin, o loop avisa e segue.

As ações pesadas chamam `scripts/heavy.sh <cmd>`, que passa pela fila da máquina inteira (`atlas run --heavy -- <cmd>`, [Referência da CLI](cli.md#atlas-run---heavy)): no máximo `min(cores / 2, GB of memory / 8)` jobs pesados ao mesmo tempo (3 em um Mac de 12 núcleos e 24 GB) em todas as worktrees e agentes da máquina, em ordem, iniciados só quando a carga de 5 minutos está abaixo de `0.75 * cores` (`ATLAS_HEAVY_MAXLOAD`) e pelo menos 20% da memória está livre (`ATLAS_HEAVY_MINMEM`), sob `nice`, com `CARGO_BUILD_JOBS` limitado a `max(2, cores / slots)`; o `atlas heavy status` mostra quem roda e quem espera. Com um atlas instalado que é anterior à fila, o script volta ao lock por usuário e à espera de carga e (macOS, Linux) de memória que tinha antes, então o `atlas.yaml` deste repositório continua funcionando em qualquer versão. Outros projetos podem usar `heavy: true` em uma ação, o que exige um atlas com a fila (um mais antigo rejeita a chave desconhecida). Use `scripts/heavy.sh` (ou `atlas run --heavy`) para qualquer coisa que compile ou meça desempenho fora do `atlas dev` também.

A migração do app-id (`crates/atlas-desktop/src/id_migrate.rs`) tem uma verificação end-to-end com WebKit de verdade: o `scripts/migration-e2e.sh` compila o app com dois bundle ids descartáveis, grava `localStorage` no "antigo", inicia o "novo" e lê de volta, depois apaga os dois stores e recompila o app de desenvolvimento normal (macOS, cerca de dois minutos).

### Três formas de ver o app que você está alterando

| Modo | Iniciar | O que você vê | Para quê |
|---|---|---|---|
| Lab | `atlas dev lab` / `bun run desktop:lab` | o frontend no seu navegador, HMR | a maior parte do trabalho de UI; os agentes trabalham aqui |
| Mirror | `bun run desktop:mirror` | a janela real inteira de uma instância invisível, ao vivo, com clique e digitação | acompanhar um agente, partes nativas sem uma janela |
| Desktop | `atlas dev desktop` / `bun run desktop:dev` | a janela real do **Atlas Dev** na sua tela (ícone laranja, id próprio), HMR + recompilações de Rust | layout nativo: barra de título, botões da barra de ferramentas, botões de janela, terminal, gestos, menus |

O Atlas Dev roda ao lado do seu Atlas instalado sem tocar nele (bundle id e store do WebKit próprios). Um agente só inicia o modo Desktop quando você pede (`atlas_start_service desktop`, e `atlas_stop_service desktop` para voltar ao headless); por conta própria ele fica no Lab e no mirror.

### Atlas Lab: o frontend no navegador, sem janela do Atlas

A maior parte do trabalho de frontend não precisa do app na tela. Escolha o nível mais leve que cobre a alteração:

| Nível | Rode | Cobre |
|---|---|---|
| 0 | `bun run test`, `cargo test` | lógica, stores, protocolo, contratos entre Rust e TS |
| 1 | `bun run desktop:lab` | qualquer tela Svelte, com HMR, contra o backend real |
| 2 | `bun run desktop:agent` | superfícies nativas: terminal GPU, page stage, gestos, flicker, CSP/IPC do bundle |

O `bun run desktop:lab [--name lab] [--port 1421]` inicia (ou reaproveita) uma instância de agente invisível e serve o frontend com o Vite em `http://localhost:1421`. Abra essa URL em qualquer navegador: todo `invoke()` e todo listener de evento é encaminhado pela ponte de debug da instância (`apps/desktop/scripts/lab-bridge.ts`, `src/lib/debug/labShim.ts`), então o backend, o hostd e os dados são reais e isolados do seu próprio Atlas. Você e um agente podem compartilhar a mesma URL: o agente a conduz em headless, você observa ou clica quando quiser, e nenhuma janela aparece por conta própria. O `bun run desktop:lab --stop` para a instância.

**Veja também as partes nativas:** o `bun run desktop:mirror [--name lab]` serve, na URL que imprime, uma visão ao vivo da janela inteira de uma instância de agente (terminal GPU, page stage e navegador embutido incluídos: quadros do ScreenCaptureKit, ou do `grim` no Linux, não o snapshot da página; no Linux a imagem é a área da página e o terminal GPU não existe, o terminal é xterm.js). Clique nela para clicar no app e digite: com o terminal em foco, as suas teclas chegam ao shell do jeito que o terminal as mapeia (Option como Alt, Ctrl+C); nos outros casos vão para o campo em foco ou para os atalhos do app. Tudo passa pela ponte de debug, nunca por entrada do sistema operacional, e nada aparece na sua tela. O app transmite a sua janela com o ScreenCaptureKit a até 30 fps para a memória (`frame` da ponte de debug), só enquanto alguém assiste. A URL carrega um token por execução: sem ele, toda rota responde 403. Ele pausa enquanto o `scripts/heavy.sh` segura o seu lock, para que os benchmarks meçam um app sem observador.

As instâncias de agente mantêm um store do WebKit cada uma (o WKWebView ignora o `HOME`); o `--stop` o apaga, e o `atlas desktop --agent --gc` apaga os stores das instâncias que não existem mais (falhas, processos mortos).

**Linux: custo de inicialização e de ociosidade (#284).** O `scripts/linux-startup-bench.ts [--runs 5] [--idle-settle 30] [--idle-cpu 60] [--save docs/perf/linux-startup.json] <atlas-desktop | .AppImage>` inicia o app no mesmo sway headless privado da verificação de abertura, N vezes (a primeira com um HOME novo = "cold", as demais "warm"; o page cache não é descartado, então "cold" é uma primeira abertura, não um disco frio), lê o `ATLAS_BOOT_TRACE` para `app_ready` (`js:ready:gate`) e para saber qual entre `show:ready` / `show:deadline` mostrou a janela, observa o `/proc` para ver o spawn, o primeiro `WebKitWebProcess` e a janela mapeada, e então mantém mais uma abertura ociosa: PSS/RSS por processo a partir de `smaps_rollup` (app, cada `WebKitWebProcess`, `WebKitNetworkProcess`, hostd; o `SwapPss` conta, então uma máquina carregada não consegue fazer um processo parecer menor mandando-o para o swap) e CPU durante a janela. Ele conta os shells de login interativos que cada abertura inicia (os arquivos rc do HOME descartável registram quem os iniciou). Sai com 1 quando o `app_ready` nunca chega, a página está em branco ou algum número passa do `BUDGET`; o `--judge <report.json>` rejulga um relatório salvo sem o app, e o `--no-check` só reporta. A ociosidade é medida depois que a primeira abertura termina o onboarding: em um HOME novo, o deck de boas-vindas anima a 60 fps e, com GL por software, custa ~50% de um núcleo. A linha de base commitada é `docs/perf/linux-startup.json` (máquina, GPU, driver e versão do WebKitGTK dentro dela; o processo que domina o RSS é nomeado em `idle.dominant`), e o CI roda o benchmark depois da verificação de abertura em "Desktop opens (Linux)" e envia o relatório como artefato. O orçamento sai dessa linha de base e dos números do próprio runner do CI (2x a pior amostra para tempo, 1,6x para memória); um número de benchmark só é comparável a outro da mesma máquina e com a mesma carga (o `machine.loadAvg1` é registrado).

**Linux: abra o app sem nenhuma janela na sua tela.** O `scripts/linux-shot.sh` roda o app em um sway headless privado (socket Wayland próprio, sessão D-Bus própria, HOME descartável; renderização por software do Mesa, sem GPU) e tira uma captura com o `grim`. O `scripts/linux-launch-check.sh <atlas-desktop | .AppImage>` faz o mesmo e **falha** diante do que um primeiro usuário encontraria: o app sai ou quebra, nenhuma janela é mapeada, a página fica de uma cor só, o `WebKitWebProcess` morre, um marcador de crash/EGL/CSP no stderr, ou qualquer linha de erro no `~/.atlas/desktop/app.log` do próprio app (códigos de saída no cabeçalho dele; evidências em `launch-check-out/`). O `scripts/linux-first-run.sh --cli target/debug/atlas --desktop target/debug/atlas-desktop` percorre o caminho do primeiro usuário com os seus próprios builds (`atlas install`, `atlas doctor`, `atlas desktop --from`, depois a verificação de abertura) em um HOME descartável, então nada dele toca o seu nem baixa um build publicado. Precisa de `sway`, `grim` e `dbus-run-session` (`pacman -S sway grim`, `apt install sway grim dbus`). O binário de debug precisa embutir o frontend, como o `cargo tauri build` faz: `(cd apps/desktop && bun run build) && cargo build -p atlas-desktop --features tauri/custom-protocol` (sem a feature, um build de debug espera o servidor de desenvolvimento do Vite na :1420 e não mostra nada). O CI roda a verificação em toda alteração que toca Rust ou o frontend (job "Desktop opens (Linux)", com a captura anexada como artefato), e o release a roda no AppImage e em cada pacote de distribuição antes de publicar ([Publicar uma versão](release.md)).

**Lacunas no Linux e no Windows.** O Windows não tem instância de agente, então não tem Lab, mirror nem gravação lá; use `bun run desktop:dev` e os testes de nível 0. O que funciona em uma instância de agente (`bun run desktop:agent`, `atlas desktop --agent`):

| | macOS | Linux | Windows |
|---|---|---|---|
| Instância isolada (HOME, hostd e ponte próprios; `--stop`, `--gc`) | sim (display virtual privado) | sim: sway headless próprio + sessão D-Bus própria em `/tmp/atlas-agent-<name>/run`; nunca a sua sessão Wayland/X11/D-Bus | não |
| `debug info\|state\|eval\|logs\|wait\|click\|type\|a11y\|perf\|resize` | sim | sim | não |
| `debug shot` da página, `--selector`, `--rect`, `--window` | sim (snapshot do WKWebView, ScreenCaptureKit) | sim: o `grim` lê o compositor da própria instância (precisa de `sway`, `grim`); `--full` também; os retângulos são em px CSS da página | não |
| `debug record` (`--seconds N` ou `--until <selector>`, `--fps`) | sim (ScreenCaptureKit, `.mov`) | sim: quadros do `grim` do compositor da instância enviados ao `ffmpeg` (precisa do `ffmpeg` com libx264), H.264 de taxa constante em `.mov`/`.mp4`; o resultado informa os `frames` realmente capturados e o `max_gap_ms`, então uma máquina lenta aparece como menos quadros, não como um vídeo mais rápido | não |
| `desktop:mirror` (visão ao vivo + clique + digitação, URL com token) | sim (ScreenCaptureKit, ~30 fps) | sim: a área da página (a webview) pelo `grim`, JPEG, até ~30 fps, só enquanto alguém assiste; cliques e teclas passam pela ponte, nunca por entrada do sistema operacional | não |
| Atlas Lab (`desktop:lab`) | sim | sim: a mesma página Vite/HMR, todo `invoke()` pela ponte da instância (WebKitGTK em vez de WKWebView) | não |
| Varredura de flicker, benchmark de seleção, `debug hide`, gravação e replay de gestos | sim | não (eles leem o ScreenCaptureKit, o monitor de gestos de deslize do macOS e o window server) | não |

Uma instância se recusa a iniciar (saída 3) quando o ambiente dela aponta para qualquer coisa que não seja o próprio display: ela nunca usa `/run/user/<uid>`, não tem `DISPLAY` e desliga a ponte de acessibilidade. No Linux, o app é o binário de debug com o frontend empacotado: o `bun run desktop:build:debug` o gera (o `desktop:agent` o faz na primeira vez). Ele pula a proteção de instância única, então roda ao lado do seu Atlas instalado e do `desktop:dev` (os builds de desenvolvimento usam o id `br.com.codeatlas.atlas.dev`; um binário de `cargo build` puro iniciado SEM `ATLAS_AGENT=1` ainda usa o id do app instalado e passaria o controle a ele: use `desktop:dev`). No Windows, use `bun run desktop:dev` (uma janela normal) e os testes de nível 0; o `check:changed`, o `heavy.sh` (o load gate fica desligado no Windows) e o `atlas.yaml` do repositório funcionam em todo lugar.

Limites: os streams de `Channel` (terminais, gestos) e os assets de `convertFileSrc` não são encaminhados, e o navegador não é o WKWebView; use o nível 2 para eles e para a verificação de CSP/IPC antes de publicar. A ponte responde só à sua própria página (token por execução, mesma Origin, content type JSON) e só quando o `ATLAS_DEBUG_SOCK` é uma instância de agente; ela nunca fala com o seu próprio app.

## Estrutura do código

Veja [Arquitetura](architecture.md) para os crates e para como os processos conversam entre si. Onde cada coisa vai:

| Alteração | Onde |
|---|---|
| um novo subcomando do `atlas` | `crates/atlas-cli/src/commands/` |
| campos de configuração, validação, o JSON Schema | `crates/atlas-tui/src/config/` |
| agendamento, health, reinícios, ações | `crates/atlas-tui/src/runtime/manager/` |
| widgets e teclas da TUI | `crates/atlas-tui/src/tui/` (a tabela de teclas é o `keymap.rs`) |
| ferramentas, recursos e prompts do MCP | `crates/atlas-tui/src/mcp/` |
| terminais, exec, arquivos | `crates/atlas-host/` |
| renderização do terminal | `crates/atlas-term/` |
| backend do desktop | `crates/atlas-desktop/src/` |
| UI do desktop | `apps/desktop/src/` |
| skills de agente | `skills/<name>/SKILL.md` (embutidas em tempo de build) |

## Convenções

- **Testes.** Prefira testes de comportamento: limites, falhas e transições de estado em vez de getters. Os testes do manager ficam em `runtime/manager/tests.rs` e são ligados por `#[cfg(test)] mod tests;` no `mod.rs`; ao dividir módulos, confira se a contagem de testes não cai. Os testes de integração conduzem o binário real.
- **A documentação anda com o código.** Uma alteração que muda uma flag, uma tecla, um campo de configuração ou um comportamento atualiza a página que o documenta, no mesmo pull request. A ajuda e o rodapé da TUI são gerados a partir de uma única tabela de teclas; os atalhos do desktop são o `DEFAULT_KEYMAP` em `apps/desktop/src/lib/keys.ts`.
- **Changelog.** Alterações visíveis ao usuário ganham uma linha em `## [Unreleased]` no `CHANGELOG.md`. As notas de release são montadas a partir dele.
- **Instruções para agentes.** O `AGENTS.md` é um mapa com menos de 8 KB: as regras que causam dano se quebradas e uma tabela "antes de mexer em X, leia Y". O detalhe vai para uma skill em `.agents/skills/<name>/SKILL.md` (curta, com arquivos irmãos para o resto) ou para um `AGENTS.md` aninhado ao lado do código que ele cobre; nunca de volta para o arquivo da raiz. O `.claude/skills` é um symlink para `.agents/skills` porque o Claude Code lê só esse caminho (o Codex e o omp leem `.agents/skills` diretamente, e o omp remove duplicatas pelo realpath). O `bun test scripts/agents-md.test.ts` (parte do `bun run check`) verifica o tamanho, as referências e o link; no Windows sem suporte a symlink, o link continua commitado como um arquivo comum.
- **Formatação.** Vale o `.editorconfig`: tabs em TypeScript, Svelte e shell; 4 espaços em Rust; 2 espaços em YAML, Markdown e JSON.
- **Commits.** Assunto curto no imperativo, prefixos convencionais (`feat:`, `fix:`, `docs:`, `chore:`) como no histórico. Explique o *porquê* no corpo quando não for óbvio.
- **Dependências.** Prefira a biblioteca padrão e o que já está na árvore. Um crate ou pacote novo precisa de um motivo no pull request; o `cargo deny check` deve passar.

### Adicionar um agente de código integrado

Os agentes que o Atlas inicia (omp, Claude Code, Codex, OpenCode) estão listados uma única vez, em `crates/atlas-core/data/agents.json`: id, rótulo, binário, o nome `atlas setup <setup>` que liga os hooks dele, as flags de início e de retomada, e o que ele sabe fazer (`one_shot`: como ele responde a um prompt sem ferramentas, o que as mensagens de commit por IA exigem, `null` quando nenhum foi verificado; `sessions_dir`; `can_ask`; a linha `install` do fornecedor; o `key_env` da chave de API dele). Todo o resto lê esse arquivo:

- Rust: `atlas_core::agents::all()` / `get(id)` (CLI, hostd, Atlas Desktop).
- Frontend do Atlas Desktop: `BUILTIN_AGENTS` em `apps/desktop/src/lib/agentRegistry.ts` importa o mesmo arquivo.
- Atlas Mobile: `apps/mobile/lib/core/protocol/agent_registry_data.dart` é gerado por `bun scripts/gen-agents-dart.ts`; o `bun test scripts` falha quando ele está desatualizado, e o `bun scripts/gen-agents-dart.ts --check --json` avisa isso para um script.

Um agente novo é uma entrada ali, depois o gerador, depois o que é realmente específico de cada agente: um logo (`agentLogos.ts`, `agent_logo.dart`), um harness de `atlas setup`, uma sonda de login (`crew/engine_login.rs`), o script de verificação (`atlas-desktop/src/crew.rs`) e uma engine (`crew/engine/`). Os testes nomeiam cada um desses que estiver faltando. O `scripts/agent-list.test.ts` falha quando uma lista de ids de agente é digitada em qualquer outro lugar.

### Regras do runtime que vieram de incidentes reais

Mantenha estas regras quando mexer no runtime ([Arquitetura](architecture.md)):

- A limpeza de órfãos usa só o arquivo `.pids` e PIDs exatos; nunca `lsof` para matar por porta nem PIDs negativos.
- Nunca rode `docker compose down` nem reinicie o daemon do docker a partir do Atlas.
- Processos filhos: `setsid`, `stdin(null)`, `sh -c` para comandos.
- TUI: entrada por stream de eventos, renderização com dirty flag, inicialização sem bloqueio.
- Desktop: webviews filhas tornam a janela principal multi-webview; use `get_window("main")` para operações de janela e `get_webview("main")` para a webview do próprio app. Nunca ponha um segredo em conteúdo carregado no navegador embutido.

## Ativos de marca

A marca do Atlas tem **uma única fonte**: [`assets/logo.svg`](https://github.com/codeatlasdev/atlas/blob/main/assets/logo.svg) (36 peças cortadas à mão e o gradiente cobalto). O ícone do app, o ícone da barra de menus e o logo animado são desenhados em `assets/templates/*.svg` (placa, sombra e chanfro em volta das peças; `{{TILES}}` marca onde a marca entra). **Todo o resto é gerado, nunca editado à mão**:

| Gerado | Usado por |
| --- | --- |
| `assets/app-icon.svg`, `assets/app-icon-1024.png` | README, o conjunto de ícones do Tauri |
| `crates/atlas-desktop/icons/{32x32,128x128,128x128@2x,icon}.png`, `icon.icns`, `icon.ico` | o bundle do app: Dock, Cmd-Tab, Finder, **notificações** |
| `crates/atlas-desktop/icons/tray.png`, `assets/tray-template.svg` | barra de menus (template monocromático; o macOS o tinge) |
| `crates/atlas-desktop/icons/tray-linux.png` | bandeja do Linux (o mesmo glifo em branco: os hosts de AppIndicator não tingem) |
| `crates/atlas-desktop/icons/32x32.png` (também o ícone do bundle do app) | bandeja do Windows: a marca em cores, porque a área de notificação nunca tinge e um template preto vira um disco sólido ali (`icon()` em `tray.rs`) |
| `assets/logo-animated.svg` | animação do README e da documentação |
| `site/public/{favicon.svg,favicon.ico,apple-touch-icon.png}` | o site |
| `studio/assets/logo/logo.svg` | filmes do estúdio |
| `apps/desktop/src/lib/onboarding/intro/mark.ts` | o globo de boas-vindas (caminhos das peças e caixas) |

```bash
bun run icons          # regenera tudo o que está acima (precisa de librsvg, ImageMagick, cargo-tauri)
bun run icons:check    # falha quando algo derivado está desatualizado; renderiza os rasters de novo e compara os pixels decodificados
```

O `assets/icons.lock.json` registra o hash de cada entrada (incluindo o `scripts/icons.ts`) e o hash de pixels de cada raster. O `bun run check` roda o `scripts/icons.test.ts`, que fica vermelho quando a marca, um template ou o gerador mudou sem um `bun run icons`, quando uma cópia foi editada à mão ou (com as ferramentas de raster instaladas) quando um raster não bate mais com uma renderização nova. Para mudar a marca: edite o `assets/logo.svg` ou um template, rode `bun run icons` e commite tudo junto.

Uma notificação, o Dock e o Cmd-Tab mostram todos o `icon.icns` do **bundle** (o plugin de notificação do Tauri publica sob o bundle id `br.com.codeatlas.atlas`; os builds de desenvolvimento usam `br.com.codeatlas.atlas.dev` e o ícone laranja em `icons-dev/`, regenerado pelo `bun run icons:dev`; no `tauri dev`, o macOS a atribui ao Terminal). Não existe um ícone de notificação separado para manter. O macOS guarda ícones em cache por app: um Atlas instalado pega um ícone novo no próximo release ou na próxima atualização, e um ícone velho no Dock ou na notificação em uma máquina de desenvolvimento é o cache de ícones do sistema, não o repositório. Não limpe caches do sistema para "consertar" isso; atualize ou reinstale o app.

## Fazer vídeos

Todo vídeo publicado (o filme do site, o corte vertical dele, os vídeos de release) é montado a partir de código em [`studio/`](https://github.com/codeatlasdev/atlas/tree/main/studio). Um filme é dado tipado em `studio/films/<id>/film.ts`; o [HyperFrames](https://github.com/heygen-com/hyperframes) (Apache-2.0) o renderiza e o `ffmpeg` mixa o áudio, então uma reconstrução é determinística e funciona em um laptop ou no CI. A escolha do engine, com o raciocínio de licenciamento, está em [`studio/DECISION.md`](https://github.com/codeatlasdev/atlas/blob/main/studio/DECISION.md); as notas voltadas a agentes estão em [`studio/AGENTS.md`](https://github.com/codeatlasdev/atlas/blob/main/studio/AGENTS.md).

Você precisa de `bun`, `ffmpeg`/`ffprobe` e, para a gravação da TUI, do [`vhs`](https://github.com/charmbracelet/vhs). Gravações, takes, voz e música **não estão no git**: ficam em `studio/.cache/` (veja o manifesto com `bun studio assets`) e os arquivos publicados são baixados com o `scripts/fetch-media.sh`.

```bash
bun install                                    # uma vez, na raiz do repositório (workspace)
bun studio check                               # typecheck, tokens de marca, dados do filme, render de teste de 1 s por cena, determinismo
bun studio preview atlas-1-0                   # prévia ao vivo; recompila quando você salva o film.ts ou uma cena
bun studio render atlas-1-0 [--aspect 9:16]    # render completo -> studio/out/atlas-1-0/ (mp4, webm, vtt, poster, sprite de navegação, relatório)
bun studio publish atlas-1-0                   # copia para site/public/media com hashes de conteúdo + film-chapters.json
bun studio release unreleased --as 1.1 --draft # vídeo "What's new" a partir da seção do CHANGELOG.md (16:9 + 9:16)
```

Captura e narração:

- O `bun studio capture app <scene>` grava o build de debug **real** do Atlas Desktop em uma instância separada (`HOME` e `ATLAS_HOSTD_DIR` próprios, projetos de teste, captura de janela por id; ele nunca envia entrada para o seu desktop). Gere antes o bundle de debug (`atlas link --desktop` ou `bun run desktop:build:app`). O `bun studio capture tui <take>` roda o `vhs` sobre `demo/demo.tape` com a saída redirecionada para o cache.
- O `bun studio voice <film>` sintetiza a narração com o ElevenLabs e guarda cada linha em cache por um hash do texto, da voz e do modelo. Os segredos vêm só do ambiente (`ELEVENLABS_API_KEY`, `MAGNIFIC_API_KEY`), nunca do repositório.
- A mixagem final é medida: o render falha quando a loudness integrada não está dentro da tolerância do filme em torno de -14 LUFS.

A narração é uma voz sintética e a trilha de fundo é gerada; mantenha corretos o aviso de divulgação e as notas de licença em `site/public/media/README.md` quando mudar qualquer uma das duas. Uma alteração em `studio/` é verificada com o `bun studio check`, não com o `bun run check`.

## Traduzir o Atlas

O inglês é o idioma de origem e o português do Brasil (pt-BR) é a primeira tradução. As traduções são arquivos comuns no repositório, revisados no pull request como código; não há conta para criar em lugar nenhum. Você não precisa escrever Rust nem Svelte para traduzir. Antes de escrever ou traduzir qualquer texto voltado ao usuário, leia o [`i18n/style/README.md`](https://github.com/codeatlasdev/atlas/blob/main/i18n/style/README.md) (voz, pontuação, sem travessão) e o arquivo do seu idioma.

Onde o texto vive: **uma árvore para a CLI, a TUI, o app desktop e o site**, `i18n/messages/<area>/<lang>.json`, uma pasta pequena por área (`settings`, `crew_onboarding`, `cli_config`, ...). Cada área tem `en.json`, um arquivo por idioma e o `meta.json` (o que cada mensagem é e onde aparece). As palavras que todas as telas compartilham (Cancelar, Excluir, nomes de teclas, "seu Mac" ou "este PC") vivem uma só vez nas áreas `common`. O `i18n/surfaces.json` diz quais áreas cada superfície usa; os arquivos que os apps leem são gerados (`bun run i18n:generate`), então você nunca os edita.

|O quê|Onde|
|---|---|
|Texto da interface|`i18n/messages/<area>/<lang>.json`|
|Páginas de documentação|`docs/<lang>/<page>.md`, com `source_sha` no front matter|

**Corrigir ou traduzir um texto.** Edite o arquivo do seu idioma na pasta da área, mantenha cada `{placeholder}` exatamente como no inglês e rode `bun run i18n:check`. Ele é offline e leva segundos; cada achado nomeia o arquivo, a linha e a chave. Se uma mensagem estiver marcada como desatualizada, o inglês mudou depois da tradução: leia o inglês, atualize a sua, rode `bun run i18n:fix` para atualizar o arquivo de lock da área. Use `bun run i18n:find <text>` para ver se uma palavra já existe em `common`. O tom e os termos que ficam em inglês estão em `i18n/style/` e em `i18n/glossary.yaml`. Plurais pedem cuidado: em pt-BR o zero soa como singular, então dê a "nenhum" uma variante própria onde "0 arquivo" soaria errado.

**Rascunhar com um modelo.** O `bun run i18n:translate` traduz só as chaves ausentes ou desatualizadas, com o glossário e a descrição de cada chave no prompt, usando a sua própria `ANTHROPIC_API_KEY` ou qualquer endpoint compatível com a OpenAI (`I18N_BASE_URL`, o Ollama funciona). Uma pessoa nativa ainda revisa o resultado no pull request.

**Mover texto fixo existente.** O `bun run i18n:extract <path-or-glob> --dry-run --json` mostra o que seria movido; sem `--dry-run`, ele escreve as chaves em inglês (com uma descrição derivada do componente), troca o texto por `m.<key>()`, reaproveita mensagens `common*` quando o inglês é o mesmo e deixa o pt-BR para o `i18n:translate`. Ele nunca toca no que não consegue mover com segurança (plurais, texto unido a uma expressão ou dividido por marcação inline, atalhos, módulos `.ts`, strings de `<script>`): o relatório lista cada um com `file:line` e o motivo, e você os migra à mão. Uma segunda execução não muda nada. Arquivos Rust recebem uma lista de trabalho, não uma reescrita. Os lotes, a ordem deles e o ciclo de qualidade estão em [`i18n/MIGRATION.md`](https://github.com/codeatlasdev/atlas/blob/main/i18n/MIGRATION.md).

**Adicionar um idioma, em um pull request** (um idioma por pull request):

1. `bun run i18n:add-locale <tag> --owner @you` (uma tag BCP-47 como `es` ou `fr`). Ele adiciona o idioma a `i18n/locales.json`, a `atlas_core::lang::SUPPORTED`, ao `Info.plist` do desktop e as linhas de dono em `.github/CODEOWNERS`, e cria o `<tag>.json` em cada pasta de área; os geradores incluem o idioma em todas as superfícies.
2. Traduza com `bun run i18n:translate --locale <tag>` e à mão até o `bun run i18n:check` ficar limpo para o seu idioma. Um idioma novo começa como `community`: uma chave ausente cai para o inglês em vez de quebrar o gate; o pt-BR é o único obrigatório.
3. Abra o pull request. O dono do seu idioma no `CODEOWNERS` o revisa, e um maintainer faz o merge.

O que nunca é traduzido: o `--json` e outras saídas de máquina, o MCP, as skills, os logs, os códigos de saída, os scripts de instalação e o changelog. O `ATLAS_LANG=pt-BR atlas ...` força um idioma para um comando, e o `atlas config lang <tag>` o salva; o desktop tem Configurações > Geral > Idioma de exibição.

## Pull requests

O fluxo é definido uma única vez, no [CONTRIBUTING.md](https://github.com/codeatlasdev/atlas/blob/main/CONTRIBUTING.md#pull-request-flow-and-who-merges): branch `type/short-desc` a partir do `main`, um pull request em rascunho, CI verde e duas aprovações de pessoas que não sejam a autora, para contribuidores externos (os maintainers, o time `atlas-maintainers`, podem fazer merge dos próprios pull requests sem aprovações). Um maintainer faz o merge (squash) dos pull requests externos, e só maintainers publicam versões. A governança e o motivo da regra estão no [GOVERNANCE.md](https://github.com/codeatlasdev/atlas/blob/main/GOVERNANCE.md).

Antes de abrir: faça a alteração com os testes e as atualizações de documentação dela, rode `bun run check` e preencha o template, vinculando a issue que ele fecha. Pull requests pequenos e focados são revisados mais rápido que os grandes.

## Proteções para agentes

Agentes de IA trabalham neste repositório, então as regras sobre onde eles escrevem e o que podem enviar por push são aplicadas por ferramentas, não por texto. Uma única função de decisão (`scripts/lib/agent-guard/decide.ts`, testada em tabela no `scripts/agent-guard.test.ts`) responde "permitir ou recusar, e o que fazer em vez disso" para todo agente, por quatro pontos de entrada:

| Ferramenta | Ligação (commitada) | Como bloqueia |
|---|---|---|
| Claude Code | `.claude/settings.json`: `PreToolUse` em `Write\|Edit\|MultiEdit\|NotebookEdit\|Bash` roda `bun scripts/agent-guard.ts hook` | saída 2; o motivo no stderr é mostrado ao agente |
| Codex | `.codex/hooks.json`: `PreToolUse` em `Bash\|apply_patch` (os hooks precisam ser revisados uma vez em `/hooks`; a camada `.codex/` do projeto precisa ser confiável) | igual, saída 2 |
| omp | `.omp/extensions/agent-guard.ts` (carregado a partir do cwd da sessão) chama a mesma função no próprio processo | `tool_call` retorna `{ block: true, reason }` |
| qualquer ferramenta, uma pessoa também | `scripts/git-hooks/pre-push` por `git config core.hooksPath scripts/git-hooks` (o `scripts/setup.sh` define isso, de forma idempotente, e nunca substitui um caminho de hooks que você escolheu) | o git aborta o push |

O que é recusado (cada mensagem nomeia a correção):

- **Escritas fora da sua worktree.** Uma ferramenta de arquivo ou um redirecionamento de shell (`>`, `>>`, `tee`, `cp`, `mv`, `rm`, `mkdir`, `touch`, `dd of=`, `cd <main checkout>`, `git -C <main> commit`…) cujo alvo realmente cai (symlinks e `..` resolvidos) fora da worktree git do cwd da sessão. Diretórios temporários e o estado próprio dos harnesses (`~/.claude`, `~/.codex`, `~/.omp`, `~/.atlas`, `~/.agents`) são permitidos; o checkout principal, as worktrees de outros agentes e os dados do próprio git (`.git/`) não são. Uma sessão que começa no **checkout principal** pode escrever ali, a menos que um maintainer tenha optado uma vez por `git config atlas.agentWrites deny` (aí o checkout principal fica fechado para agentes, e o `git worktree add` é a saída).
- **`git push` para `main`, `master` ou `release/*`**, `--all`, `--mirror`, `--no-verify`, `-c core.hooksPath=`; um `git push` sem argumentos enquanto estiver no main.
- **Force push** (`-f`, `--force`, `--force-with-lease`, `+refspec`).
- **Merge** (`gh pr merge`, a API de merge ou auto-merge, `bun run pr:merge` exceto com `--dry-run`).
- **`git stash`** (tudo menos `list` e `show`): todas as worktrees de um clone compartilham um único `refs/stash`.
- **Excluir** branches e tags (`git branch -d/-D`, `git tag -d/-f`, `git push --delete`, `git push origin :ref`, `git update-ref -d`), e mudar `core.hooksPath` ou `atlas.agentWrites`.

**Exceção do maintainer.** Um maintainer define `ATLAS_MAINTAINER_OK` no ambiente *antes de iniciar a sessão* (ou em volta de um comando no próprio terminal): uma lista separada por vírgulas de `write`, `push`, `force-push`, `merge`, `stash`, `delete`, `config` ou `all`. Exemplo: `ATLAS_MAINTAINER_OK=push bun run release alpha` (o release envia o `main` e uma tag por push). As proteções leem isso só do ambiente do próprio processo, e um comando que menciona `ATLAS_MAINTAINER_OK` é recusado de cara, então um agente não consegue dar a si mesmo a exceção. O hook de pre-push não distingue um agente de uma pessoa: um comando que o agente roda em um shell no qual o maintainer exportou a variável é do maintainer.

Limites, para ninguém confiar demais: as proteções leem comandos, não os isolam em sandbox. Um script, `python -c`, código `eval` no REPL de um agente ou `sed -i` ainda podem escrever em qualquer lugar; variáveis (`> "$DIR/x"`) não são expandidas. Um bug na própria proteção deixa a chamada passar e avisa no stderr (o hook de pre-push falha fechado). Depure uma decisão com `echo '{"tool_name":"Bash","cwd":"<dir>","tool_input":{"command":"git stash"}}' | bun scripts/agent-guard.ts hook --json`.

## CI e releases

O CI (`.github/workflows/ci.yml`) roda em todo pull request e em pushes para `main` e `release/**`: macOS, Linux e Windows rodam o gate de Rust (uma action composta, `.github/actions/rust-gate`, para não divergirem), Linux e Windows rodam os scripts do repositório e os testes deles (`bun run release:test`, job "Repo scripts"), além de "Desktop opens (Linux)" (o app aberto em headless, `scripts/linux-launch-check.sh`), as verificações do frontend do desktop, o build do site, o `cargo deny`, o relatório de duplicação e o lint dos workflows. Pull requests de forks o rodam com um token somente leitura e sem segredos. Como ele é ligado:

- **Uma verificação obrigatória.** O último job, `CI`, passa só quando todos os outros passaram, então os jobs podem ser renomeados ou divididos sem mexer no ruleset. O `pr.yml` adiciona `Signed-off-by` (DCO) e `Conventional title` (o título do pull request vira o assunto do commit do squash). Essas três são as verificações obrigatórias.
- **Sem uma segunda execução de testes depois do merge.** A branch precisa estar atualizada para o merge, então o commit do squash no `main` tem exatamente a árvore que o pull request testou. O `scripts/ci-tested.sh` confere isso (mesma árvore do head do pull request e uma execução verde do CI para ela), e então o push pula os testes e só compila o que mantém o cache aquecido. Uma árvore diferente, um push direto ou qualquer erro de API roda tudo.
- **Os caches vêm do `main`.** O cache de um pull request é visível só para aquele pull request ([documentação do GitHub](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#restrictions-for-accessing-a-cache)) e expulsaria o cache do `main` que todo pull request restaura (10 GB por repositório), então os caches são salvos só a partir do `main` e de `release/*`. As execuções ali nunca são canceladas; um novo push em um pull request cancela a execução mais antiga dele.
- **Fixado e auditado.** Toda action é fixada em um SHA de commit completo, com a versão exata em um comentário (`# v7.0.1`), atualizada pelo Dependabot depois de uma quarentena de 7 dias. O `scripts/lint-workflows.sh` roda actionlint e zizmor (configuração: `.github/zizmor.yml`, cada exceção com o seu motivo) localmente no `bun run check` e no CI.
- **Language servers de verdade, fora do gate.** Os quatro testes com `#[ignore]` de `crates/atlas-lsp/tests/real.rs` (rust-analyzer, os servers npm em um Node gerenciado, gopls: eles baixam os seus servers) rodam em `.github/workflows/lsp-real.yml`, um job de matriz por teste: toda semana, à mão (`gh workflow run lsp-real.yml`) e em pull requests que tocam `crates/atlas-lsp/**` ou o workflow. Ele não faz parte do `CI`: uma versão nova de server ou uma queda de download não deve bloquear um merge, então leia a última execução dele quando mexer no crate de LSP. Localmente: `cargo test -p atlas-lsp --test real -- --ignored --nocapture`, com `HOME` e `TMPDIR` apontados para um diretório temporário.
- **O instalador do Windows tem o seu próprio job.** O "Installer" roda o `scripts/install-ps1.test.ts` no `windows-latest`, só quando o `site/public/install.ps1` ou esse teste muda (área `installer` em `scripts/checks/map.ts`; qualquer caminho desconhecido ou alteração de workflow o roda também). Ele roda `irm <url> | iex` como um usuário faz, no `powershell.exe` (Windows PowerShell 5.1, o que um Windows de fábrica tem) e no `pwsh`, contra um servidor local que faz o papel do GitHub (o script lê `ATLAS_GITHUB_ORIGIN`, um gancho de teste que também permite http puro; nunca o defina fora desse teste), com o `curl.exe` e o `tar.exe` reais e um `atlas.exe` de mentira compilado com o `csc.exe` do .NET Framework. Ele cobre a Stable mais nova sem build para Windows (a volta para a alpha, #519), uma divergência de checksum e o passo do canal. Ele não consegue rodar no macOS nem no Linux (o script precisa do `curl.exe` e do `tar.exe`); mantenha o `install.ps1` seguro para a 5.1: nada de `2>$null` ou `2>&1` em um comando nativo fora do `Invoke-Native`, só ASCII, nada de `??` ou `$IsWindows`.

Versões: [Publicar uma versão](release.md).
