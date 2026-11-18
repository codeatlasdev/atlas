---
title: "Referência da CLI"
source: cli.md
source_sha: "8bf1f4659bc1c0d87bec1040247b4e680075745d"
translation: machine
---

# Referência da CLI

```text
atlas <command> [options]
```

Global: `-h/--help` em todo comando, `-V/--version` no `atlas`.

Duas formas não precisam de subcomando:

```text
atlas <path>            # abre uma pasta ou arquivo no Atlas Desktop
atlas -- <cmd...>       # roda um comando como serviço da stack desta pasta
```

### `atlas <path>`

`atlas .`, `atlas ~/code/api`, `atlas src/main.rs`. O caminho abre no Atlas Desktop: uma pasta abre como projeto, e um arquivo abre o **projeto do arquivo** (a pasta mais próxima com um `atlas.yaml`, senão a mais próxima com `.git` ou com um `package.json` / `Cargo.toml` / `pyproject.toml` / `go.mod`, senão a própria pasta do arquivo) e depois o arquivo no editor. Uma pasta sem `.git`, `atlas.yaml` ou manifesto abre como [pasta simples](desktop.md#pastas-simples) (explorador e editor, sem serviços). Um arquivo de texto sem projeto acima dele, como `atlas ~/Downloads/notes.md`, abre no [editor leve](desktop.md#abrir-um-arquivo-avulso) do desktop e não adiciona nenhum projeto. Não é preciso ter um `atlas.yaml`. Se o Atlas Desktop está rodando, o caminho vai para ele; senão o app é iniciado com o caminho e o abre na inicialização (instale-o antes com `atlas desktop`).

**Um subcomando sempre vence.** `atlas dev` roda o subcomando mesmo quando existe uma pasta chamada `dev`; abra essa pasta com `atlas ./dev`. Qualquer outra primeira palavra é um caminho só se for `.`, `..`, tiver um separador de caminho ou existir no disco; caso contrário é um erro de digitação e falha como sempre falhou (`atlas devv` → `error: unrecognized subcommand 'devv'` + `tip: a similar subcommand exists: 'dev'`, saída `2`). O `atlas` sozinho imprime a ajuda.

### `atlas -- <cmd...>`

```text
atlas -- bun dev
atlas -- node -e "require('http').createServer((q,s)=>s.end('ok')).listen(4123)"
```

Roda o comando como um **serviço ad-hoc** da stack da pasta atual (sua [raiz do projeto](#atlas-path)): supervisionado como qualquer serviço (logs em `.logs/<name>.log`, sondagem de porta quando o comando cita uma com `--port N`, `PORT=N` ou `.listen(N)`, reiniciar e parar pela TUI, pelo Desktop e pelo MCP). Ele **não** é salvo no `atlas.yaml`. O nome vem do comando (`bun dev` → `bun-dev`; flags e código entre aspas ficam de fora; um nome já usado ganha `-2`, `-3`…). Um argumento é tratado como linha de comando pronta (`atlas -- "bun dev"`); vários são escapados para o shell da plataforma (`cmd.exe` no Windows). Se um runner em segundo plano já é dono da raiz, o serviço é adicionado a ele na hora; se nada está rodando, a stack é iniciada em segundo plano (como `atlas dev --detach`) com o serviço ad-hoc incluído. Se uma TUI é dona da raiz (ela não aceita serviços na hora), o comando falha e avisa: rode `atlas down` e depois o comando de novo. Uma TUI conectada a um runner mostra os serviços ad-hoc que já existiam quando ela se conectou.

| Comando | Finalidade |
|---|---|
| [`dev`](#atlas-dev) | inicia o ambiente de desenvolvimento (TUI, runner em segundo plano, headless, CI ou status) |
| [`down`](#atlas-down) | para o atlas em execução (runner em segundo plano ou TUI) e os serviços dele |
| [`init`](#atlas-init) | cria o `atlas.yaml` |
| [`init`](#atlas-init) | cria o `atlas.yaml` (também: salva o que o `atlas dev` detectou) |
| [`deploy`](deploy.md) | envia uma versão para um servidor: troca atômica, reinícios condicionados à saúde, rollback automático |
| [`mcp`](#atlas-mcp) | servidor MCP para agentes de IA |
| [`update`](#atlas-update) (alias `up`) | autoatualização |
| [`install`](#atlas-install-oculto) | instala o binário em `~/.atlas/bin` |
| [`setup`](#atlas-setup) | conecta o atlas ao seu agente de IA (OMP, Claude Code, Codex) |
| [`skills`](#atlas-skills) | gerencia as skills de IA |
| [`version`](#atlas-version) | imprime versão, SO e arquitetura |
| [`uninstall`](#atlas-uninstall) | remove o Atlas desta máquina |
| [`desktop`](#atlas-desktop) | abre o Atlas Desktop, instalando-o na primeira execução |
| [`wt`](#atlas-wt) | worktrees do Atlas Desktop: listar, criar, remover |
| [`term`](#atlas-term) | terminais do Atlas Desktop: listar, ler, enviar, esperar, encerrar |
| [`browser`](#atlas-browser) / [`file`](#atlas-file) | controla o navegador embutido, abre arquivos e diffs |
| [`link` / `unlink`](#atlas-link--unlink) | aponta o `atlas` instalado (e o `Atlas.app`) para um build de debug local |

Em qualquer comando, exceto `update`, `self-update` e `mcp`, o Atlas procura uma nova versão em segundo plano (com `curl`, sem chamadas à API do GitHub; para o [canal de versões](#atlas-update) que você segue; no máximo a cada 6 horas, em cache em `~/.atlas/.update_check` junto com o canal para o qual foi calculado, então trocar de canal descarta o cache; sessões longas do `atlas dev` continuam verificando). Só uma versão *mais nova* é anunciada, nunca um downgrade. Quando existe uma versão mais nova:

- **Antes de a TUI iniciar** (só no `atlas dev`, veja [Prompt de atualização](#prompt-de-atualização-atlas-dev)) o Atlas pergunta se deve atualizar agora;
- **Dentro de uma TUI em execução** um modal pergunta `Atlas v<new> available — update when you quit?` (Enter = sim, Esc = agora não; uma vez por versão por sessão). Aceitar mostra `⬆ on exit` na linha da marca na barra lateral; depois que a TUI sai, os serviços são parados e o terminal é restaurado, o Atlas roda a atualização e imprime o resultado (ele não reinicia). Conectado a um runner (`atlas dev` contra um `--detach`), o runner mantém a versão antiga até `atlas down && atlas dev --detach`. Um `⬆` verde simples marca uma atualização disponível que você não aceitou;
- depois que o comando termina (inclusive quando você sai da TUI sem aceitar), se o stderr é um terminal, um banner é impresso:

```text
────────────────────────────────────────────────
 ⬆ Update available
 atlas v0.3.27 → v0.3.28   Run: atlas update
────────────────────────────────────────────────
```

Erros são impressos como `error: <message>` (mais linhas `caused by:`) e saem com `1`.

## `atlas dev`

```text
atlas dev [SERVICE...] [--profile <NAME>]... [--no-deps] [-d, --dir <DIR>]
          [-D, --detach] [--headless] [--ci] [--timeout <SECONDS>] [--status]
```

| Flag | Padrão | Significado |
|---|---|---|
| `SERVICE...` | todos | Inicia só esses serviços mais tudo de que eles dependem de forma transitiva em `depends_on`. Veja [Selecionar serviços](#selecionar-serviços). |
| `--profile <NAME>` | nenhum | Inicia os serviços de uma entrada de [`profiles:`](configuration.md#profiles) (mais as dependências). Repetível; perfis e `SERVICE`s são unidos. |
| `--no-deps` | desligado | Com `SERVICE`/`--profile`: inicia exatamente esses, sem puxar as dependências. |
| `-d, --dir <DIR>` | `.` | Diretório do projeto (`~` é expandido). Ele não precisa de um `atlas.yaml`: sem um, o Atlas [detecta](configuration.md#sem-atlasyaml-config-detectada) os serviços em memória (a partir de `package.json`, `Cargo.toml`, `pyproject.toml`, `go.mod`) e imprime `no atlas.yaml: using detected config (…); save it with 'atlas init'`. Nada é gravado. |
| `-D, --detach` | desligado | Inicia os serviços em um **runner em segundo plano** e retorna: o runner vira um daemon completo (sessão própria, stdin `/dev/null`, stdout/stderr em `.logs/runner.log`), mantém o lock do projeto e serve o socket de controle e o heartbeat de status como o `--headless`. Imprime `atlas runner started (pid N) · attach: atlas dev · stop: atlas down` e sai com `0` assim que o socket responde. Um `atlas.yaml` ou uma seleção com problema é reportado no seu terminal e nada é iniciado. Funciona com `SERVICE…` / `--profile` / `--no-deps`. Se um runner já está ativo, imprime `atlas runner already running (pid N) …` e sai com `0` (a seleção não é aplicada a ele); se uma TUI é dona do projeto, sai com `1`. Veja [Runtime](runtime.md#runner-e-anexar) (runner e anexar). |
| `--headless` | desligado | Sem TUI. Inicia os serviços, envia `[name ] line` para o stdout e as mudanças de estado para o stderr, e para tudo com Ctrl-C / SIGTERM / SIGHUP. Serve o [socket de controle](mcp.md#como-funciona) para que as ferramentas de controle do MCP funcionem. |
| `--ci` | desligado | Inicia tudo, espera estabilizar, imprime um relatório JSON no stdout, para tudo e sai. |
| `--timeout <SECONDS>` | `60` | Espera máxima no modo `--ci`. |
| `--status` | desligado | Imprime o estado de um `atlas dev` já em execução e sai. Só lê; não inicia nada. |

Sem `--detach`/`--headless`/`--ci`/`--status`, o `atlas dev` roda a [TUI](tui.md). **Se um runner (`--detach` ou `--headless`) já é dono do projeto, ele se conecta a esse runner** (mesmo painel, os serviços continuam rodando quando você sai; veja [TUI](tui.md#modo-anexado), modo conectado); `SERVICE…` / `--profile` são ignorados nesse caso, com um toast. Se o stdin ou o stdout não é um terminal, imprime `atlas dev needs an interactive terminal. Agents/scripts: use 'atlas dev --status' or 'atlas dev --ci'.` e sai com **2**.

Precedência quando as flags são combinadas: `--status`, depois `--detach`, depois `--ci`, depois `--headless`.

### Selecionar serviços

```text
atlas dev web                      # web + tudo de que web depende (ex.: api, database)
atlas dev web worker               # união dos dois, mais as dependências deles
atlas dev --profile backend        # membros de profiles.backend, mais as dependências
atlas dev --profile backend --profile front   # união de dois perfis
atlas dev api --no-deps            # só o api; as dependências NÃO são iniciadas
```

- A seleção é formada pelos serviços citados (e pelos membros dos perfis) mais o `depends_on` **transitivo** deles. Serviços de infra (sem comando, com sondagem de porta, como o `database`) entram só quando um serviço selecionado depende deles; um de que nada selecionado depende fica desligado. Sem `SERVICE` e sem `--profile`, tudo inicia como antes. Quando `infra:` (docker compose) está configurado mas nenhum serviço sem comando está modelado no `atlas.yaml`, o Atlas não sabe quem precisa dele, então o `docker compose up -d` continua rodando (comportamento antigo).
- Serviços fora da seleção aparecem na barra lateral como `⊘ … off` (estado `disabled`), **não são iniciados no boot** e podem ser iniciados à mão pelo menu do serviço (`m`), pela paleta de comandos ou pelo `atlas_start_service` do MCP (um início manual não espera o `depends_on`, como em [`autostart: false`](configuration.md#services)). Depois de iniciados, fazem parte da execução como qualquer outro serviço.
- A validação acontece antes de qualquer coisa iniciar (saída `1`, nada é tocado): um nome de serviço ou perfil desconhecido falha com `unknown service 'wev' (did you mean 'web'?); available: …`; `--no-deps` sem um `SERVICE`/`--profile` é um erro. Um perfil que lista um serviço desconhecido é um erro de configuração (`atlas config check`).
- Funciona na TUI, no `--headless` e no `--ci`. O `--ci` só avalia os serviços selecionados: os outros aparecem como `"state": "disabled", "off": true` e nunca fazem a execução falhar.
- O `--status`, o arquivo de status e o `atlas_status` do MCP marcam esses serviços: `STATE` mostra `off`, o arquivo de status tem `"deselected": true`, as linhas do MCP têm `"off": true`. O `atlas_wait_ready` sem serviço ignora os serviços `disabled`, então espera só pela seleção.

### `--status`

```text
$ atlas dev --status
atlas dev running (pid 4242, updated 1s ago)
mode: runner · 1 attached
SERVICE              STATE     PORT   HEALTH
api                  running   3001   http://localhost:3001/health
web                  starting  3002   -
worker               off       -      -
tunnel               running   54320  prod-db -> localhost:5432
```

`mode:` diz quem é dono da execução: `runner` (`--detach` ou `--headless`; mostra quantos clientes TUI estão conectados) ou `tui` (um terminal TUI é dono e não aceita conexões). `off` marca um serviço deixado de fora desta execução por `SERVICE…` / `--profile` (veja [Selecionar serviços](#selecionar-serviços)).

A linha `tunnel` aparece só enquanto existe um [túnel SSH](configuration.md#tunnel): `STATE` é o estado dele, `PORT` a porta local, depois `<ssh_host> -> <remote_host>:<remote_port>`.

| Saída | Significado |
|---|---|
| `0` | um `atlas dev` ativo é dono deste projeto (arquivo de status recente, pid vivo) |
| `3` | não está rodando; imprime `atlas dev not running (<reason>)`, por exemplo `no status file`, `process 123 is not alive`, `status is stale (last heartbeat 500s ago)` |

Um arquivo de status fica obsoleto após 120 s sem heartbeat (o Atlas o atualiza cerca de uma vez por segundo). Nomes de estado: `starting`, `running`, `failed`, `stopped`, `disabled`, `suspended`.

### `--ci`

Inicia todos os serviços selecionados (tudo, a menos que `SERVICE…` / `--profile` restrinja; infra primeiro, depois os serviços de app na ordem das dependências), espera pelo menos 10 s e até que nada esteja em `starting`, nenhum gate de dependência ou reinício esteja pendente e os serviços de app tenham sido iniciados, ou até o `--timeout` expirar. Depois imprime:

```json
{
  "ok": true,
  "services": [
    { "name": "database", "state": "running", "port": 5432 },
    { "name": "migrate",  "state": "stopped", "port": null, "completed": true },
    { "name": "api",      "state": "running", "port": 3001 },
    { "name": "web",      "state": "failed",  "port": 3002, "reason": "dependency api failed" }
  ]
}
```

`ok` é `true` quando todo serviço está `running`, `stopped` ou `disabled`. Serviços deixados de fora por `SERVICE…` / `--profile` aparecem como `{ "name": "worker", "state": "disabled", "port": null, "off": true }` e nunca afetam o `ok`. Um serviço que sai com código diferente de zero, desistiu de reiniciar ou cuja dependência falhou fica `failed` (com `reason`) e deixa o `ok` falso. Um `oneshot` que saiu com `0` fica `stopped` com `"completed": true` e conta como OK. O progresso vai para o stderr.

| Saída | Significado |
|---|---|
| `0` | `ok: true` |
| `1` | `ok: false`, erro de configuração, ou outro atlas já é dono do projeto |
| `130` | interrompido por Ctrl-C / SIGTERM / SIGHUP (os serviços são parados antes) |

O `--ci` não serve o socket de controle. Tanto o `--ci` quanto o `--headless` rodam o `pre_command` antes de iniciar os serviços (veja [pre_command](configuration.md#pre_command)).

### Uma instância por projeto

`--ci`, `--headless` e `--detach` usam o mesmo lock de instância única da TUI. Se outro `atlas dev` já roda nesse diretório, `--ci` e `--headless` imprimem `atlas dev is already running (pid N); use 'atlas dev --status'` e saem com `1`, sem tocar em nenhum processo. A TUI simples **se conecta** a um runner, e só quando quem detém o lock é outra TUI (à qual não dá para se conectar) ela pergunta `Kill it and restart? [y/N]`; veja [Runtime](runtime.md#singleton-e-o-socket-de-controle) (instância única e socket de controle).

### Prompt de atualização (`atlas dev`)

Antes de a TUI assumir o terminal, o `atlas dev` simples oferece uma atualização pendente:

```text
╭──────────────────────────────────────────────────────────────────────╮
│ ⬆ Atlas update available: v0.4.1 → v0.5.0                            │
│ Changelog: https://github.com/codeatlasdev/atlas/releases/tag/v0.5.0 │
╰──────────────────────────────────────────────────────────────────────╯
Update now? [Y/n]
```

- A versão mais nova vem do cache (`~/.atlas/.update_check`) quando ele tem menos de 6 horas; caso contrário o Atlas consulta ao vivo com um timeout rígido de **2 s** e volta em silêncio para o cache em caso de timeout ou erro. Ele nunca atrasa a inicialização além disso.
- **Enter / `y`** roda a mesma lógica do [`atlas update`](#atlas-update) e depois reexecuta o novo binário com exatamente os mesmos argumentos, então o `atlas dev …` continua na nova versão. Se a atualização falha, o erro é impresso e o Atlas pergunta `Continue with v<current>? [Y/n]`.
- **`n`** continua com a versão atual e silencia essa versão por **24 horas** (salvo no cache; caches antigos sem o campo ainda carregam).
- Nunca é perguntado com `--status`, `--ci`, `--headless`, `--detach` ou no runner, quando o stdin/stdout não é um terminal, ou com `ATLAS_NO_UPDATE_PROMPT=1`. Em um build linkado ou de debug (`atlas link`), o Atlas só imprime uma dica de uma linha e nunca substitui o binário.

## `atlas down`

```text
atlas down [-d, --dir <DIR>]
```

Para o atlas que é dono do projeto: pede a ele pelo socket de controle que pare todos os serviços e saia (o mesmo caminho gracioso do Ctrl-C: árvores de tarefas inteiras, sem `docker compose down`, com a política de saída do túnel aplicada) e depois espera até **15 s** para ele sumir. Funciona com um runner em segundo plano, uma execução `--headless` e uma TUI de terminal.

| Saída | Código |
|---|---|
| `atlas stopped (pid N)` | `0` |
| `atlas is not running` | `0` (nada a fazer) |
| `atlas (pid N) is still stopping after 15s; check: atlas dev --status` | `1` |

## `atlas doctor`

```text
atlas doctor [--json]
atlas doctor --hygiene [--dry-run | --fix] [--json]
```

O `atlas doctor` simples verifica a instalação, cada item com o único comando que o corrige, e sai com 1 quando algo está quebrado:

- **new terminal**: inicia o shell da sua conta do jeito que um terminal faz (Linux: interativo, não login; macOS: login) e pergunta qual `atlas` ele roda. Falha quando não há nenhum (rode o instalador de novo) e avisa quando outro `atlas` vem antes no `PATH` ou quando o shell não responde em 6 s.
- **this terminal**: se `~/.atlas/bin` está no `PATH` deste shell (imprime a linha `export`).
- **desktop app**: instalado e na versão da CLI (`atlas update`); ausente em uma sessão gráfica, sugere `atlas desktop`.
- **terminal host**: se o hostd responde (ele inicia sob demanda, então é só um aviso).

O `--hygiene` olha em vez disso o que o Atlas e os agentes que ele roda deixaram para trás, e o disco que ele ocupa. **Só observa por padrão** (`--dry-run`); o `--fix` altera as coisas.

Ele lista:

- **Processos órfãos**: processos de uma sessão cujo dono (terminal, `atlas dev`, hostd) sumiu, com pid, nome, o que os iniciou e o projeto, além de quantos processos de sessões ativas ele deixou em paz. São encontrados pela tag `ATLAS_SESSION` no Linux e pelo que o dono registrou em suas raízes no macOS e no Windows, que não conseguem ler o ambiente de outro processo; nesses sistemas, um processo que fez fork e se desanexou em cerca de 2 s depois de iniciar nunca foi visto e não aparece na lista ([sessões e higiene](runtime.md#sessões-e-higiene));
- **Disco por projeto** (as pastas do Atlas Desktop): `target/`, `node_modules`, `.logs` e `.atlas/worktrees/*`;
- **Diretórios temporários e de cache do Atlas** em relação aos limites deles;
- **No macOS, diretórios pesados que o Spotlight indexa** (`target/` e `node_modules` de um projeto ou de suas worktrees sem um `.metadata_never_index` neles ou acima).

O `--fix` encerra os órfãos (SIGTERM, 3 s, SIGKILL, pids exatos, nunca um pid reciclado), exclui os diretórios registrados de sessões mortas e reduz os outros até os limites, descarta os `*.prev.log` rotacionados quando o `.logs` de um projeto passa de 200 MiB (o `<service>.log` ativo fica) e adiciona o marcador `.metadata_never_index` aos diretórios pesados indexados (só um arquivo marcador). **`target/`, `node_modules`, worktrees e qualquer arquivo que o Atlas não criou nunca são removidos**: eles são reportados para você decidir.

| Saída | Código |
|---|---|
| o relatório (e, com `--fix`, o que foi encerrado, liberado e marcado) | `0` |
| o `--fix` deixou um processo rodando após o SIGKILL | `1` |

O `--json` imprime `{"report": …, "fixed": …|null}`, o mesmo valor que o painel do desktop e a ferramenta `atlas_hygiene` do MCP leem.

## `atlas init`

```text
atlas init [-d, --dir <DIR>]      # padrão .
```

Grava um `atlas.yaml` mínimo (`name:` a partir do diretório mais os serviços detectados, ou um exemplo comentado) e imprime o que detectou. Falha se o `atlas.yaml` já existe. É também assim que você **salva** o que o `atlas dev` [detectou](configuration.md#sem-atlasyaml-config-detectada) em uma pasta sem um: o arquivo é exatamente a configuração que aquela execução usou.

## `atlas run --heavy`

```text
atlas run --heavy [--label <NAME>] [--no-nice] -- <cmd...>
atlas heavy status [--json]
```

Rode um comando pesado (um build, uma suíte de testes, um benchmark) pela **fila de toda a máquina**: todo projeto, worktree, agente e shell da máquina a compartilha, então cinco agentes iniciando suas suítes ao mesmo tempo nunca levam a carga ou a memória além do limite. No máximo N jobs rodam ao mesmo tempo (padrão `min(cores / 2, GB of memory / 8)`, no mínimo 1); os demais esperam por ordem de chegada (ninguém fura a fila) e começam no instante em que um slot libera. O comando roda como filho, com o seu stdin/stdout/stderr, sob `nice -n 10` no Unix (`--no-nice` mantém a prioridade dele), e o código de saída dele é o do `atlas run` (`128 + signal` quando um sinal o encerrou). Ctrl-C/SIGTERM enquanto está na fila tira o job da fila (saída `130`, o comando nunca roda); enquanto roda, o sinal é repassado ao comando. Uma única palavra é uma linha de comando de shell (`atlas run --heavy -- "bun run check && bun test"`), várias formam um argv.

- **Slots:** `ATLAS_HEAVY_SLOTS` (inteiro positivo), senão `heavy.slots` em `~/.atlas/config.json` (`{"heavy": {"slots": 3}}`), senão `min(cores / 2, memory in GB / 8)`, no mínimo 1: 12 cores e 24 GB dão 3 slots, 16 GB dão 2, 8 GB dão 1 (um build a frio de um workspace grande chega perto de 8 GB, então a memória, e não só os cores, limita quantos rodam juntos).
- **Gate de carga:** um job com slot ainda espera até que a carga de 5 minutos fique abaixo de `ATLAS_HEAVY_MAXLOAD` (padrão: `0.75 * cores`; `0` desliga o gate), por no máximo `ATLAS_HEAVY_WAIT` segundos (padrão 600), e depois roda mesmo assim, com um aviso. O Windows não tem load average, então o gate fica desligado lá.
- **Gate de memória:** ele também espera enquanto menos de `ATLAS_HEAVY_MINMEM` por cento da memória está livre (padrão `20`; `0` desliga o gate; também `heavy.min_mem_pct` em `~/.atlas/config.json`, e o ambiente vence). A memória livre é `kern.memorystatus_level` no macOS (o número de pressão do próprio kernel), `MemAvailable / MemTotal` no Linux e `GlobalMemoryStatusEx` no Windows; uma máquina que não sabe informar fica sem gate de memória. Um job retido pela memória avisa; depois de `ATLAS_HEAVY_WAIT` segundos ele roda mesmo assim, com um aviso, como no gate de carga. A memória é verificada antes da carga: uma máquina cheia congela, uma ocupada só fica mais lenta.
- **Paralelismo do build:** o ambiente do job limita quantos workers cada ferramenta inicia, para que os jobs que rodam juntos fiquem dentro da máquina: `CARGO_BUILD_JOBS` e `UV_THREADPOOL_SIZE` (o thread pool das ferramentas de node/bun) são definidos como `max(2, cores / slots)`, a menos que você já os tenha definido, caso em que o seu valor fica. Um job sozinho em uma máquina de 2 slots ainda recebe metade dos cores para os builds; defina `CARGO_BUILD_JOBS` você mesmo para ter mais.
- **Sem estado obsoleto:** um slot é um lock de arquivo do SO em `~/.atlas/heavy/` (`ATLAS_HEAVY_DIR` o move) mantido pelo processo `atlas run` em execução, então um job que é morto, cai ou perde a máquina libera o slot na hora, e o registro dele é ignorado no mesmo instante. Cada job também grava um pequeno registro JSON (pid, label, cwd, project, queued_at, started_at).
- `--label` dá nome ao job na fila e no histórico de ETA dele (padrão: o nome do programa). O Atlas guarda os últimos 8 tempos de execuções bem-sucedidas por label e mostra um ETA a partir da média deles.
- `atlas heavy status` lista quem ocupa os slots e quem espera, com idades e ETAs (`--json` é o que a ferramenta [`atlas_heavy_status`](mcp.md#atlas_heavy_status) do MCP retorna).
- No `atlas.yaml`, `heavy: true` (esta versão em diante) em uma [ação](configuration.md#actions) (e `deploy.build_heavy: true` para o [`deploy.build`](deploy.md)) a faz passar pela fila. O `scripts/heavy.sh <cmd>` neste repositório é um wrapper do `atlas run --heavy`.

## Cache de tarefas (`atlas run --cache`, `atlas cache`)

```text
atlas run --cache [--input <GLOB>]... [--output <PATH>]... [--env <NAME>]... [--tool <PROG>]... [--cache-failures] [--heavy] [--label <NAME>] -- <cmd...>
atlas cache status [--json]
atlas cache gc
atlas cache clear
```

Rode um comando pelo **cache de tarefas de toda a máquina** para que um trabalho idêntico nunca rode duas vezes, qualquer que seja a worktree ou o agente que peça. A chave é um sha256 do argv, dos **bytes de cada arquivo de entrada como estão no disco agora** (edições não commitadas incluídas, respeitando o `.gitignore`), dos valores dos nomes `--env` declarados e da `--version` das `--tool`s declaradas. A pasta em que o comando roda não faz parte da chave: duas worktrees com os mesmos bytes compartilham um resultado, e duas com edições diferentes nunca compartilham.

- **Hit:** as saídas salvas são colocadas de volta (clone copy-on-write onde o sistema de arquivos permite, uma cópia caso contrário; uma pasta declarada é esvaziada antes), o log é reproduzido no mesmo stdout/stderr, uma linha `atlas cache: hit <key> (…)` vai para o stderr e o código de saída é `0`. Nada roda; uma entrada pequena leva alguns milissegundos.
- **Miss:** o comando roda (com `--heavy`, atrás da [fila heavy](#atlas-run---heavy)), a saída dele passa ao vivo, e **só um código de saída 0 é guardado** (`--cache-failures` também guarda uma falha, e um hit reproduz o código de saída dela). Uma execução com falha ainda é reportada ao `atlas_last_result`.
- **Entradas:** globs de `--input` (repetíveis) escolhem os arquivos que contam; sem nenhum, contam todos os arquivos sob a pasta atual que o git não ignoraria. **Saídas:** caminhos de `--output` (arquivos ou pastas dentro da pasta atual, sem `..`) são o que um hit restaura; sem nenhum, só o log e o código de saída ficam em cache. Um caminho de saída incorreto ou uma `--tool` que não consegue imprimir a versão dela interrompe a execução antes de o comando começar.
- **Armazenamento:** `~/.atlas/cache/` (`ATLAS_CACHE_DIR` o move). O conteúdo são blobs por sha256 (saídas e logs, guardados uma vez, não importa quantas entradas e worktrees os compartilhem) e entradas por chave. O limite de tamanho é `ATLAS_CACHE_MAX_BYTES`, senão `cache.max_bytes` em `~/.atlas/config.json` (`{"cache": {"max_bytes": 4294967296}}`), senão 2 GiB: as entradas usadas há mais tempo são removidas para que o armazenamento nunca termine uma gravação acima do limite, e uma entrada maior que o limite sozinha não é guardada. As gravações são atômicas e um lock serializa as alterações, então dois agentes colocando a mesma chave em cache ao mesmo tempo deixam uma entrada válida. Um log é mantido até os 8 MiB mais recentes.
- `atlas cache status` mostra as entradas, o tamanho em relação ao limite e os contadores (hits, misses, tempo economizado: a duração original, uma vez por hit; total e por projeto; `--json` tem o formato do `stats.json`, que o Atlas Desktop lê; inclui `days`, os hits e o tempo economizado por dia UTC nos últimos 30 dias, de onde sai o "economizado nos últimos 7 dias"). `atlas cache gc` descarta os blobs que nenhuma entrada usa e reduz até o limite; `atlas cache clear` esvazia tudo.
- No `atlas.yaml`, uma [ação](configuration.md#actions) com um bloco `cache:` (`inputs`, `outputs`, `env`, `tools`) roda pelo `atlas run --cache`. A ferramenta [`atlas_last_result`](mcp.md#atlas_last_result) do MCP responde o que uma tarefa fez por último: saída, duração, quando, final do log, hit ou miss.

## `atlas config`

```text
atlas config check [-d, --dir <DIR>]   # padrão .
atlas config schema
```

- `check` valida o `atlas.yaml` mais o `atlas.local.yaml`. Imprime `OK: <path> is valid` e sai com `0`, ou imprime todos os erros e sai com `1`. Um arquivo ausente ou que não pode ser lido é um `error:` e sai com `1`. Detalhes: [Validação](configuration.md#validação).
- `schema` imprime o JSON Schema (draft-07) do `atlas.yaml` no stdout.

## `atlas crew`

```text
atlas crew install <ssh-target> [-y|--yes] [--port <N>] [--identity <KEY>] [--binary <PATH>]
atlas crew update <ssh-target> [--version <X.Y.Z>] [--rollback] [--port <N>] [--binary <PATH>]
atlas crew uninstall <ssh-target> [-y|--yes]
atlas crew export <ssh-target> [-o <FILE>] [--crewmate <ID>]... [--with-secrets]
atlas crew import <ssh-target> <FILE> [--rename]
atlas crew status <ssh-target>
atlas crew logs <ssh-target> [-f|--follow]
atlas crew login <ssh-target> <omp|claude|codex|opencode>
atlas crew pair <ssh-target> [--port <N>]
```

Transforma um servidor Linux (x86_64 ou aarch64) com systemd e sudo em um crew server que roda crewmates 24 horas por dia, e o gerencia por SSH. O `install` também migra um servidor instalado com o nome antigo `atlas-bot` e mantém os dados e o usuário de sistema dele; `atlas bot …` é um alias oculto de `atlas crew …`. `--binary` (ou `ATLAS_CREW_BINARY` no `install`) envia um build de Linux em vez de baixar a versão desta CLI. O `update` coloca no servidor a versão desta CLI (ou `--version X`), mantém o binário anterior como `atlas.prev`, espera o hostd e faz rollback sozinho se ele não responder; `--rollback` restaura o `atlas.prev`. O `status` termina com a versão do atlas no servidor e se ela está atrás desta CLI. O `uninstall` lista primeiro o que vai excluir, oferece uma exportação e pergunta o nome do servidor em um terminal (`--yes` pula as perguntas, mas ainda imprime o que foi removido; sem terminal e sem `--yes` ele recusa). O `export` grava um `.tar.gz` versionado com os crewmates, a memória, o histórico de tarefas, as automações e o uso (segredos só com `--with-secrets`, e então o arquivo fica com permissão 0600; nunca o checkout de `work/` nem os logins dos agentes); o `import` o restaura, recusando colisões de id a menos que haja `--rename`. Guia completo: [Atlas Crew](crew.md#fazer-backup-e-mover-crewmates), seção sobre backup e migração de crewmates.

## `atlas deploy`

Faz o build local, envia, e deixa um driver no host fazer lock → troca → gate de saúde → rollback. Subcomandos: `rollback`, `status`, `logs`, `history`. Códigos de saída 0/1/2/3/4. Referência completa: [deploy.md](deploy.md).

## `atlas mcp`

```text
atlas mcp [-d, --dir <DIR>]
```

Roda o servidor MCP por stdio. `--dir` é o diretório do projeto; quando omitido, o Atlas usa a [raiz do projeto](#atlas-path) do diretório atual (o `atlas.yaml` mais próximo, senão o `.git` ou manifesto de pacote mais próximo, senão o próprio diretório atual), então as ferramentas também funcionam em uma pasta sem `atlas.yaml` (o `atlas_config` então reporta os serviços [detectados](configuration.md#sem-atlasyaml-config-detectada)). O stdout carrega só mensagens do protocolo; uma linha `atlas mcp: serving <dir> on stdio` vai para o stderr. Sai quando o cliente fecha o stdin. Guia completo: [MCP](mcp.md).

## `atlas update`

```text
atlas update [--check] [--channel <stable|beta|alpha>] [--json] [--downgrade] [-y, --yes]
atlas update channel [<stable|beta|alpha>] [--json]
# alias: atlas up; também alias oculto: atlas self-update
```

Procura a versão mais nova do seu [canal de versões](versioning.md#canais) (com `curl`, sem login e sem chamadas à API do GitHub: o feed público `releases.atom` para todo canal: Stable é a tag mais nova sem rótulo de pré-lançamento, nunca só o redirecionamento `releases/latest`, que é o alpha mais novo enquanto o Alpha é o padrão) e, se ela é mais nova que a versão em execução, baixa o `atlas-<version>-<platform>.tar.gz` e o `.sha256` dele da versão, verifica o checksum, substitui o binário em execução no lugar (mantendo um backup que é restaurado se o app não puder acompanhar) e depois atualiza as skills e qualquer configuração de OMP, Claude Code ou Codex que encontrar, usando o novo binário (falhas não são fatais). Um [Atlas Desktop](desktop.md) instalado é levado para a mesma versão (`atlas desktop --sync`) e o terminal host é reconciliado. `--check` só reporta. Instalações pelo Homebrew são orientadas a rodar `brew upgrade atlas`. Recusa enquanto o `atlas link` está ativo (ele sobrescreveria o build de desenvolvimento); rode `atlas unlink` primeiro.

**Falhas de rede.** Toda requisição tem um timeout de conexão de 10 s e repete erros transitórios até 3 vezes; um download não tem limite total de tempo (o bundle do desktop é grande), mas é abortado quando fica abaixo de 1 KB/s por 30 s, e o curl o retoma na nova tentativa. Uma consulta que falha é um erro, nunca uma resposta diferente: se o feed de versões não pode ser lido, ou se o build beta/alpha mais novo não pode ser confirmado como publicado (só um 404 explícito significa "ainda não publicado"), o `atlas update` reporta a falha em vez de oferecer o Stable ou um build mais antigo, e o resultado em cache da verificação é mantido.

**Canais.** `stable`, `beta` e `alpha` (o padrão enquanto o Atlas está em alpha; Stable de novo quando existir um Stable de verdade) descrevem quanto tempo uma versão ficou em teste, não o quão pronto o Atlas está; veja [Versionamento e canais de versões](versioning.md). O canal é uma palavra em `~/.atlas/update-channel`, compartilhada com o Atlas Desktop; sem arquivo (ou com um ilegível) você segue o padrão. Uma instalação que nunca escolheu um canal vê então o alpha mais novo como atualização (nunca um downgrade). `ATLAS_UPDATE_CHANNEL=<name>` o sobrescreve para um processo (scripts, CI); um valor incorreto é ignorado.

- `--channel <name>` **salva** esse canal e depois atualiza a partir dele. Sem a flag, o canal salvo é usado. Um nome desconhecido é um erro que lista os válidos (código de saída 2).
- `atlas update channel` lista todos os canais com a promessa, o risco, a cadência, o público e o que cada um oferece *agora* para a sua instalação, e marca o que você segue. Em um terminal ele termina com um seletor numerado (`1`-`3` ou o nome; Enter mantém o atual); com pipe, só imprime. `atlas update channel <name>` salva o canal e imprime o que a troca significa mais o que ele oferece agora.
- `--check --json` imprime um único objeto e nunca instala:

  ```json
  { "installed": "1.3.0", "channel": "beta", "offered": "1.4.0-beta.1", "tag": "v1.4.0-beta.1", "action": "update" }
  ```

  `action` é `update` (uma versão mais nova é oferecida), `downgrade` (só uma versão mais antiga é oferecida) ou `none`; `offered` e `tag` são `null` quando o canal não tem nada publicado. `atlas update channel --json` imprime `{ "selected", "installed", "channels": [...] }`, uma entrada por canal com o mesmo texto que o comando imprime mais `selected`, `tag`, `offered` e `action`.

**Versões retiradas.** Uma versão listada em `data/yanked.json` (como adicionar uma: `docs/release.md`, "Yanking a bad release") nunca é oferecida por nenhum canal. Quando a versão que *você roda* foi retirada, o `atlas update` imprime o motivo e o que a substitui, o `--check --json` o traz como `"yanked": { "version", "reason", "replacement" }` (`null` caso contrário), e uma versão segura mais antiga é instalada sem `--downgrade` (a confirmação de downgrade ainda vale). Uma lista ilegível é registrada no log e ignorada.

**Downgrades nunca são automáticos.** Quando o canal oferece algo *mais antigo* que o build que você roda (por exemplo, você roda um alpha e troca para o Stable), o `atlas update` não instala nada: ele diz qual versão você roda, qual o canal oferece, que você volta a receber atualizações quando o canal passar do seu build e como voltar. `atlas update --downgrade` instala essa versão mais antiga mesmo assim, depois de um aviso de compatibilidade de dados e de uma confirmação `[y/N]` (padrão Não; `--yes` a pula, e fora de um terminal ela é obrigatória). O download passa pela mesma verificação de checksum, backup e rollback de qualquer atualização. Não está disponível para instalações pelo Homebrew ou por pacote de sistema (o gerenciador de pacotes é dono da versão).

**Instalações de que outra ferramenta é dona.** Um binário instalado pelo Homebrew, ou por um pacote de sistema do Linux, nunca é sobrescrito pelo próprio `atlas update`.

- *Homebrew:* o `atlas update` imprime `brew upgrade atlas` e sai com 0 (o app desktop vê `@@atlas managed homebrew` e não reporta "up to date").
- *Pacote de sistema (Arch, Debian/Ubuntu, Fedora/openSUSE):* o dono é encontrado com `pacman -Qo`, `dpkg -S` ou `rpm -qf` (a regra do caminho `/usr/bin` é o plano B). `atlas update --check` reporta a versão atual e a mais recente. `atlas update` baixa o asset correspondente e o `.sha256` dele, verifica **antes de qualquer elevação de privilégio**, imprime o comando, confirma (`--yes` pula; obrigatório sem terminal) e instala via `sudo` (`pkexec` sem terminal; o `sudo` nunca roda sem um), mantendo o `~/.atlas/.update.lock`. Depois atualiza as skills e as configurações de agente com o binário instalado. Veja [Pacotes Linux](installation.md#pacotes-para-linux). Se nenhum gerenciador de pacotes é encontrado, imprime onde obter o pacote e emite `@@atlas managed package`. Sem terminal e sem `--yes` (o Update do app), ou sem `pkexec`, ele não baixa nada: imprime o comando do gerenciador de pacotes e emite `@@atlas managed package <manager> <install|manual> <command…>` (saída 0 sob o app, um erro que cita o comando nos demais casos); o app mostra o comando com um botão Copy e, para `install`, um "Install now" que roda `atlas update --yes`.
- `--check --json` acrescenta `"managed": true`, `"manager"` (`pacman`, `apt`, `dnf`, `zypper` ou `null`) e `"command"` (a linha exata, só quando uma atualização é oferecida), e `action` é `package` (pacote Linux) ou `managed` (Homebrew) em vez de `update` quando uma atualização é oferecida; `none` e `downgrade` continuam como são.

**O que ele recusa.** Um build de debug ou um binário alcançado por um symlink de `atlas link` nunca é substituído (a mesma regra que o `atlas dev` usa antes de oferecer uma atualização; `atlas unlink` volta ao build de release). Antes de baixar, ele verifica se o diretório da CLI e o app instalado podem ser gravados e nomeia o artefato que não pode, então nunca substitui a CLI e depois faz rollback dela porque o app é somente leitura. Skills e hooks de agente só são atualizados depois que o app acompanhou; quando o app não consegue, a CLI sofre rollback e o binário anterior os atualiza, então eles nunca são mais novos que a CLI que fica.

**Cache.** O cache do aviso diário `~/.atlas/.update_check` é gravado de forma atômica (um arquivo `.tmp` e um rename); um arquivo ilegível mantém o "agora não" dele; e uma verificação que falhou (offline) é carimbada, então nada tenta de novo por 30 minutos (um início do `atlas dev` offline não espera mais pela rede). Toda verificação ao vivo deixa a resposta ali, incluindo `atlas update --check` e `--check --json` (uma oferta, ou "nada mais novo"): é assim que o **Check now** de um celular atualiza o que o hostd mostra como a linha de versão do computador ([Atlas Mobile](mobile.md#atualizar-o-atlas-pelo-celular)). O banner de saída imprime o comando do gerenciador de pacotes nas instalações por pacote.

## `atlas desktop`

```text
atlas desktop                          # abre o Atlas.app; na primeira execução instala a versão que corresponde a esta CLI
atlas desktop --install [--latest]     # (re)instala: a versão desta CLI, ou a versão mais nova
atlas desktop --from <PATH> [--no-open]  # instala um Atlas.app local ou um Atlas-*.app.tar.gz
atlas desktop --status                 # caminho instalado, versão, linkado ou não
atlas desktop --dev                    # dentro do repositório atlas: bun run desktop:dev (hot reload)
atlas desktop [-y|--yes] [--appimage]  # Linux: instala o pacote da distro sem perguntar / o AppImage em vez dele
```

Instala em `/Applications/Atlas.app` (ou em `~/Applications` quando `/Applications` não é gravável). Baixa o `Atlas-<version>-<platform>.app.tar.gz` e o `.sha256` dele da versão pública do GitHub (com `curl`, sem login) e verifica o checksum. O novo bundle é copiado ao lado do antigo e só é trocado quando está completo, então um download que falha nunca quebra o app instalado. Recusa enquanto o app está linkado (`atlas link --desktop`). Guia completo: [App Desktop](desktop.md).

No Linux, uma distro que tem um pacote de versão (Arch, Debian, Ubuntu, Fedora, RHEL, openSUSE; lido de `/etc/os-release`, só x86_64) recebe esse pacote em vez do AppImage: o `atlas desktop` o baixa, verifica o checksum, mostra o comando de root e pergunta `[Y/n]` (`sudo` em um terminal, `pkexec` sem um), e depois abre `/usr/bin/atlas-desktop`. `--yes` / `-y` pula a pergunta; `--appimage` instala o AppImage mesmo assim. Quando ninguém pode confirmar (sem terminal, sem `--yes`), a resposta é não ou o gerenciador de pacotes falha, o AppImage é instalado como antes. Detalhes: [Instalação](installation.md#instalar-o-atlas-desktop), seção sobre instalar o Atlas Desktop.

### `atlas desktop --agent`

```text
atlas desktop --agent [--name NAME] [--scratch]   # inicia uma instância isolada, imprime o ATLAS_DEBUG_SOCK dela
atlas desktop --agent --stop [--name NAME]        # para uma instância, ou todas
```

Só em builds de debug (dentro do repositório atlas ou a partir de `target/debug/atlas`). A instância tem `HOME`, diretório do hostd e socket de debug próprios em `/tmp/atlas-agent-<name>/`, sem ícone no Dock nem na bandeja, nunca toma o foco e vive em um display virtual privado, então nunca aparece no seu desktop. `--scratch` povoa o `HOME` dela só com as suas configurações de `~/.atlas/desktop`. `bun run desktop:agent` o encapsula e gera o bundle quando ele não existe. Detalhes: [App Desktop](desktop.md#instâncias-de-agente-bun-run-desktopagent), seção sobre instâncias de agente.

### `atlas desktop debug`

```text
atlas desktop debug info | state | a11y [SELECTOR] | perf [--seconds N]
atlas desktop debug logs [--level log|info|warn|error|debug|csp] [--since SEQ]
atlas desktop debug eval <JS>
atlas desktop debug shot [PATH] [--selector CSS | --rect x,y,w,h | --full | --window]
atlas desktop debug record PATH (--seconds N | --until SELECTOR) [--fps 24|30|60]
atlas desktop debug wait TARGET [--expr] [--timeout MS]
atlas desktop debug click SELECTOR | type SELECTOR TEXT [--delay MS]
```

Controla um build de **debug** em execução por `$ATLAS_DEBUG_SOCK` (padrão `~/.atlas/desktop-debug.sock`, que é o SEU app: agentes usam `atlas desktop --agent` e o socket dele) sem tocar no mouse nem no teclado. `eval` aceita uma expressão ou um corpo com `return` e aguarda promises. `shot` é um instantâneo da página (retina, sem permissão); `--window` é a imagem do compositor da janela inteira. `click`/`type` são eventos de DOM na webview, nunca entrada do SO. Builds de release não contêm a ponte.

## `atlas wt`

```text
atlas wt ls [--repo NAME] [--json]
atlas wt create --repo <name|id|path> [--name N] [--base REF] [--agent omp|claude|codex|blank] [--prompt TEXT] [--pr N] [--no-setup] [--json]
atlas wt rm <id|name|path> [--force]
atlas wt set <id> [--name N] [--comment C] [--unread true|false]
atlas wt current [PATH]
```

Precisa do Atlas Desktop em execução (socket de controle `~/.atlas/desktop.sock`). O `create` espera até a worktree existir e retorna os terminais dela (ids `pty`). O `rm` mantém uma branch não mesclada, a menos que haja `--force`.

## `atlas term`

```text
atlas term ls | read <pty> [--lines N] | send <pty> <text> [--enter] | kill <pty> | wait <pty> --for exit|idle [--timeout S]   [--json]
```

Terminais do Atlas Desktop. Com o app aberto, passa pelo app (hosts SSH, estado do agente: `wait --for idle` retorna quando o turno de um agente termina); caso contrário, fala com o `atlas hostd` local. `send --enter` para uma aba de agente envia o texto como um prompt. O `wait` sai com 2 no timeout.

## `atlas browser`

```text
atlas browser goto <url> [--tab T] [--worktree W] | snapshot | click @eN | fill @eN <text> | screenshot   [--json]
```

O navegador embutido. O `snapshot` lista os elementos interativos visíveis com refs (`@e1`…); as refs valem até o próximo snapshot.

## `atlas file`

```text
atlas file open <path> [--worktree W]
atlas file diff [--worktree W]
```

## `atlas hostd` (oculto)

```text
atlas hostd [--dir PATH] [--status | --stop | --upgrade | --check | --restart | --reconcile [--choice ask|idle|later]]
```

O daemon de terminal/exec por trás do Atlas Desktop, iniciado pelo app. `--status` imprime as informações dele (saída 1 quando está parado); `--stop` o encerra junto com os terminais dele; `--upgrade` substitui o daemon em execução por este binário mantendo todo terminal vivo. `--check` imprime se ele é mais antigo que este binário (versão, uptime, terminais em execução, o que está desligado, os comandos de correção; saída 0 atual, 3 desatualizado, 1 não está rodando); `--reconcile` o leva para este binário (handoff ao vivo quando possível, reinício silencioso quando nenhum terminal roda, caso contrário ele pergunta, ou segue `--choice`; saída 0 também quando não está rodando); `--restart` encerra os terminais dele, inicia um daemon novo e o verifica. Um daemon sem terminal em execução, sem cliente e sem celular pareado por 10 minutos (`ATLAS_HOSTD_IDLE_SECS`) sai sozinho. `--dir` (ou `ATLAS_HOSTD_DIR`) move o socket, o token e o scrollback dele para fora de `~/.atlas`, que é também como ele roda em hosts SSH.

## `atlas install` (oculto)

```text
atlas install [--dir <DIR>]   # padrão ~/.atlas/bin
```

Copia o binário em execução para `<dir>/atlas`, configura o `PATH` para fish, zsh e bash (pulado quando o diretório já está no `PATH`) e instala as skills incluídas. Usado pela instalação manual; veja [Instalação](installation.md#instalação-manual).

## `atlas setup`

```text
atlas setup                                   # detecta agentes no PATH, imprime o status + os próximos comandos
atlas setup omp|claude-code|codex [--scope user|project] [--apply] [--force] [--uninstall]
```

Integração de agente em um comando para [OMP, Claude Code e Codex](agents.md). Cada variante instala a entrada do servidor MCP `atlas`, disponibiliza as skills incluídas para aquele agente (o Codex compartilha o `~/.agents/skills` do OMP; o Claude Code recebe `~/.claude/skills`) e registra hooks que injetam o `atlas dev --status` no início da sessão e se recusam a iniciar a TUI interativa. É uma **simulação** a menos que `--apply` seja dado; ele mescla na configuração existente sem tocar nas outras entradas (comentários e ordem das chaves preservados), é idempotente e só substitui ou remove artefatos de que o Atlas é dono (`--force` sobrescreve uma entrada `atlas` de terceiros). `--scope project` grava no projeto (o ancestral mais próximo com `atlas.yaml`): `.omp/`, `.mcp.json` + `.claude/`, ou `.codex/`. Sobrescritas: `PI_CODING_AGENT_DIR`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME` e `HOME`. Detalhes completos: [Configuração de agentes](agents.md).

### `atlas hook` (oculto)

```text
atlas hook claude-code|codex <event>   # JSON do evento no stdin, JSON da resposta no stdout
```

O comando que o Claude Code e o Codex rodam para os hooks que o `atlas setup` registra. `session-start` e `pre-tool-use` respondem ao agente; `user-prompt-submit`, `post-tool-use`, `stop`, `stop-failure`, `session-end`, `notification` (Claude) e `permission-request` (Codex) só reportam o estado do agente ao Atlas Desktop quando ele roda em um dos terminais do app. Não é feito para rodar à mão (fica de fora do `--help`); sempre sai com 0 e não imprime nada quando não tem opinião. Veja [Configuração de agentes](agents.md#o-que-os-hooks-fazem), seção sobre o que os hooks fazem.

## `atlas skills`

```text
atlas skills list
atlas skills install [NAMES...] [--local]
atlas skills update [--local]
atlas skills status [--local]
```

Skills são instruções para agentes compiladas dentro do binário `atlas`, então sempre correspondem à versão dele (sem rede). O `install` sem nomes instala todas (`atlas`, `atlas-desktop`, `atlas-install`); o `update` reescreve as skills instaladas cujo conteúdo difere da cópia incluída. Por padrão elas vão para `~/.agents/skills/` (registradas em `~/.atlas/.atlas-skills.json`), que o OMP e o Codex leem (o Claude Code lê `~/.claude/skills`; `atlas setup claude-code --apply` as instala lá); `--local` usa o `.agents/` do diretório atual. `atlas setup omp|codex --apply` também as instala.

## `atlas version`

Imprime `atlas <version>`, o **canal** para o qual o build foi gerado (`alpha`, `beta`, `stable`), o **commit** do qual ele foi construído (`unknown` fora do git), o **build id** (a identidade do executável para o linker: o build id GNU do ELF no Linux, o UUID do Mach-O no macOS, a assinatura do PDB no Windows, `none` onde o executável não tem) e o SO e a arquitetura. Coloque o bloco inteiro em um relatório de bug: o build id é o que encontra os símbolos publicados para aquela versão ([Símbolos](release.md#símbolos)).

```
atlas 1.5.0-alpha.25
  channel: alpha
  commit:  0123456789ab
  build:   56d6b439423186b782f314e535b5c103d49f3d15
  os:      linux
  arch:    x86_64
```

Se a CLI entra em pânico, ela grava um relatório de falha (`crash-<unix_ms>-<pid>.toml`, os 20 mais recentes são mantidos) no diretório de dados dela e imprime a URL do issue tracker. Uma falha no próprio processo (uma falha de segmentação, erro de barramento, instrução de CPU não reconhecida, exceção aritmética ou abort; uma violação de acesso no Windows) deixa também um `crash-native-<unix_ms>-<pid>.toml` ali, no mesmo diretório e com a mesma retenção: o sinal, o endereço da falha, o build (versão, canal, commit, build id) e os endereços brutos da pilha, e mais nada (sem linha de comando, ambiente nem caminhos, já que o handler não consegue redigir). Os dois tipos trazem o build id e `load_bias`/`frames`, que o `scripts/symbols.sh resolve` transforma em funções e linhas. Os relatórios ficam na sua máquina; tokens na linha de comando e na mensagem são redigidos e o seu diretório home é escrito como `~`, mas revise um antes de anexá-lo a uma issue.

## `atlas uninstall`

```text
atlas uninstall [-y, --yes] [--purge] [--dry-run]
```

Lista o que vai remover e depois remove, após confirmação (pule com `--yes`; sem terminal e sem `--yes` ele recusa). Por padrão, isso é o que o Atlas instalou: o comando `atlas`, o app desktop (um app de desenvolvimento linkado perde só o link; no Linux também a entrada de lançador e o ícone dele e os restos de uma atualização interrompida), as linhas `# Atlas CLI` de PATH que ele adicionou aos arquivos de inicialização do seu shell (nunca uma linha sua), o cache de atualização e os sockets do terminal host. Configurações, a lista de projetos, worktrees, o canal de atualização e o plugin do omp em `~/.atlas` são **mantidos** e listados (no Linux também o armazenamento WebKit do app, os relatórios de falha e o estado das janelas); `--purge` exclui todo o `~/.atlas` e esses itens também. `--dry-run` lista e sai. Uma etapa que falha é reportada e as outras ainda rodam. Quando um pacote de distro é dono do `atlas`, a lista nomeia o pacote e o comando de remoção dele (`sudo pacman -R …`, `sudo apt remove …`, `sudo dnf remove …`) e, após a sua confirmação, o roda como root; ele nunca exclui por conta própria os arquivos de um pacote. Não toca nas skills em `~/.agents/skills`. Detalhes: [Instalação](installation.md#desinstalar), seção sobre desinstalar.

## `atlas link` / `unlink`

Para trabalhar no próprio Atlas.

```text
atlas link [--dir <WORKSPACE>] [--no-build] [--desktop]
atlas unlink
```

O `link` compila o `atlas-cli` (a menos que haja `--no-build`), faz backup do `~/.atlas/bin/atlas` em `atlas.backup` e o substitui por um symlink para `<workspace>/target/debug/atlas`; a troca é um único rename, então um link que não pode ser criado (Windows sem o Modo de Desenvolvedor) deixa o binário de release onde estava, e um link deixado pendente por um `cargo clean` é só substituído. Rode dentro do repositório do Atlas ou passe `--dir`. A TUI mostra um selo `⚡ linked` nesse modo. O `unlink` restaura o backup; sem backup, ele transforma o link em uma cópia simples do build de desenvolvimento, então o `atlas` continua funcionando (rode `atlas update` para uma versão de release). O `atlas install` deixa em paz um `atlas` linkado ou mais novo também.

`--desktop` também gera um `Atlas.app` de debug (`cargo tauri build --debug`) e faz de `/Applications/Atlas.app` um symlink para ele. Uma instalação real é movida antes para `~/.atlas/desktop/Atlas.app.backup`. O app mostra um selo **Dev build**, e as atualizações dele ficam pausadas. O `unlink` restaura o app também.
