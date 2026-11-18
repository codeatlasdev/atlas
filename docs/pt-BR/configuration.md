---
title: "Configuração"
source: configuration.md
source_sha: "86e6d09cb84d39c590e4034e60cb5faf66621b8d"
translation: machine
---

# Configuração: `atlas.yaml`

O `atlas.yaml` é opcional. O Atlas procura o arquivo no diretório em que você o executa (`atlas dev --dir <path>` para apontar outro); o `atlas.local.yaml` ao lado dele é mesclado por cima, para overrides por máquina. Sem um `atlas.yaml`, veja [Sem atlas.yaml](#sem-atlasyaml-config-detectada).

Conteúdo: [Nível superior](#campos-de-nível-superior) · [services](#services) · [depends_on](#depends_on) · [política de reinício](#política-de-reinício) · [profiles](#profiles) · [infra](#infra) · [tunnel](#tunnel) · [actions](#actions) · [mobile](#mobile) · [env_files](#env_files) · [overrides locais](#overrides-locais-atlaslocalyaml) · [validação](#validação) · [JSON Schema](#json-schema-autocompletar-no-editor) · [exemplo completo](#exemplo-completo)

Chaves desconhecidas são erros em qualquer profundidade (com uma dica "did you mean"), então um erro de digitação nunca passa em silêncio sem fazer nada.

## Sem atlas.yaml: config detectada

`atlas dev` (TUI, `--detach`, `--headless`, `--ci`), `atlas down`, `atlas mcp` e o Atlas Desktop funcionam numa pasta sem `atlas.yaml`. O Atlas monta a config em memória a partir do que encontra, a mesma que o `atlas init` escreveria:

| Encontrado | Serviços |
|---|---|
| `package.json` | todo script chamado `dev`, `dev:*`, `start`, `start:*` ou que contenha `serve` (comando = o corpo do script, porta de `--port N` / `PORT=N` quando presente, senão do bloco `4010–4099`) |
| `pyproject.toml` / `requirements.txt`, `Cargo.toml`, `go.mod` | o comando de execução usual da linguagem, quando um é detectado |
| nada | nenhum serviço: a stack começa vazia, e `atlas -- <cmd>` adiciona [serviços ad hoc](cli.md#atlas----cmd) a ela |

Toda execução com config detectada imprime uma linha, por exemplo `no atlas.yaml: using detected config (services: dev); save it with 'atlas init'`. **Nada é gravado implicitamente**; `atlas init` (ou "Save as atlas.yaml" no Atlas Desktop) grava exatamente o que foi detectado, e a partir daí o arquivo é a fonte da verdade. O `atlas.local.yaml` continua se aplicando por cima de uma config detectada, e um `atlas.yaml` que existe nunca é substituído pela detecção (um arquivo quebrado é um erro, não um fallback). `atlas config check` e `atlas deploy` ainda exigem um arquivo de verdade.

## Campos de nível superior

| Campo | Tipo | Padrão | Descrição |
|---|---|---|---|
| `name` | string | **obrigatório** | Nome do projeto (aparece na TUI, é usado nos logs copiados, é devolvido pelo MCP). |
| `services` | map | **obrigatório** | Nome do serviço → [definição do serviço](#services). A ordem no arquivo não importa; o Atlas ordena por `depends_on`. |
| `pre_command` | string | detecção automática | Comando de shell executado em segundo plano antes de o dashboard precisar das dependências. Veja [pre_command](#pre_command). |
| `pre_command_when` | `always` \| `lockfile` | `always` com um `pre_command`; `lockfile` na detecção automática | Quando o `pre_command` roda. Veja [pre_command](#pre_command). |
| `profiles` | map | `{}` | Grupos nomeados de serviços para `atlas dev --profile NAME`. Veja [profiles](#profiles). |
| `infra` | objeto | nenhum | Arquivo [Docker compose](#infra) iniciado antes dos serviços do app. |
| `tunnel` | objeto | nenhum | [Túnel SSH](#tunnel) aberto na inicialização e supervisionado. |
| `actions` | lista | `[]` | [Comandos personalizados ligados a teclas](#actions). |
| `env_files` | lista de strings | `[]` | [Arquivos dotenv](#env_files) injetados em todo serviço e ação. |
| `mobile` | objeto | `{}` | [Configurações de iOS/Android](#mobile) para ações com `platform:` (normalmente no `atlas.local.yaml`). |
| `deploy` | objeto | nenhum | Motor de release do `atlas deploy`. Veja [deploy.md](deploy.md). |
| `affected` | objeto | nenhum | O que uma mudança afeta, para a ferramenta MCP [`atlas_affected`](mcp.md#atlas_affected): um `command` de plugin ou `rules` declaradas. Veja [affected](#affected). |
| `agents` | objeto | tudo permitido | O que agentes de IA podem fazer pelas [ferramentas de controle do MCP](#agents). `agents: { control: deny }` os deixa só com leitura. |

### pre_command

O Atlas o executa na raiz do projeto (por `sh -c`). Se ele roda numa determinada partida depende de [`pre_command_when`](#pre_command_when) (abaixo). A lógica é a mesma em todos os modos:

- **TUI** (`atlas dev`): em segundo plano enquanto o dashboard abre; os serviços do app esperam ele terminar para começar (a infra sobe enquanto isso). Um toast "Dependencies installed" no sucesso; na falha, um toast de erro `pre_command failed: exit N: <last stderr line>` e os serviços começam mesmo assim (eles falham com os próprios erros se as dependências estiverem mesmo faltando).
- **`--headless`**: roda **antes** de qualquer serviço começar; o sucesso imprime `dependencies installed`, a falha vai para o log (`pre_command failed: …`) e a inicialização continua.
- **`--ci`**: roda **antes** de qualquer serviço começar; uma falha aborta a execução com código de saída `1` e o JSON `{"ok": false, "reason": "pre_command failed: exit N: …", "services": []}`.

- **Omitido**: detecção automática, só para projetos JS (precisa existir um `package.json`; projetos Rust e Go nunca o executam). Se `node_modules` não existe, ou um lockfile (`bun.lock`, `pnpm-lock.yaml`, `yarn.lock`, `package-lock.json`) é mais novo que `node_modules/.atlas-installed` (ou que o próprio `node_modules`), o Atlas roda `bun install`, `pnpm install --frozen-lockfile`, `yarn install --frozen-lockfile` ou `npm ci`, escolhido pelo lockfile que existe (bun, pnpm, yarn, senão npm).
- **String não vazia**: roda esse comando no lugar do detectado automaticamente. Por padrão (`pre_command_when: always`) ele roda em **toda** partida, então `pre_command: bun install` faz o que você espera mesmo com o `node_modules` em dia. Com `pre_command_when: lockfile`, roda só sob a checagem de defasagem acima (o seu comando então precisa criar o `node_modules`, ou roda de novo em toda partida).
- **`""`** (string vazia): nunca instala. Típico no `atlas.local.yaml` quando você mesmo gerencia as dependências. Nem "Run now" o executa.

#### pre_command_when

- `always` (padrão quando `pre_command` é uma string não vazia): roda em toda partida.
- `lockfile`: roda só quando `node_modules` não existe ou um lockfile é mais novo que `node_modules/.atlas-installed`.
- O instalador detectado automaticamente (sem `pre_command`) **sempre** usa `lockfile`, diga o que este campo disser: o `npm ci` apaga o `node_modules`, então nunca é executado sem condição.
- Pode ser sobrescrito no `atlas.local.yaml` (substitui o valor base).

#### Saída e "Run now"

Toda execução que de fato roda grava stdout+stderr combinados em `.logs/setup.log` (primeira linha `$ <command>`, última linha `exit status: N`; truncado por execução) e o estado dela em `.logs/setup.json` (`{"state": "running" | "ok" | "failed", "command", "started_at", "finished_at", "reason"}`). Uma execução pulada pela condição deixa os dois arquivos intactos. O Atlas Desktop mostra o comando, o agendamento, o último status e a saída numa seção **Setup** da visão geral do projeto, com um botão **Run now** que roda o comando uma vez, qualquer que seja o `pre_command_when`.

## services

```yaml
services:
  database:                 # no command: externally managed
    port: 5432

  api:
    command: cd apps/api && air
    dir: apps/api           # optional working directory (relative to project root)
    port: 3001
    health: http://localhost:3001/health
    depends_on: [database]
    env: { LOG_LEVEL: debug }
    open_key: a
    critical: true
    critical_help: "Run `bun run db:up` and retry"
```

| Campo | Tipo | Padrão | Descrição |
|---|---|---|---|
| `command` | string | nenhum | Comando de shell, executado como `sh -c "<command>"` na raiz do projeto (ou em `dir`), então `cd`, `&&`, pipes e redirecionamentos funcionam. **Omitido = gerenciado por fora** (docker, um serviço do sistema): o Atlas nunca o inicia e só observa a `port` dele. |
| `dir` | string | raiz do projeto | Diretório de trabalho, relativo à raiz do projeto. |
| `port` | integer (u16) | nenhum | Porta TCP. Usada para sondar prontidão e saúde, mostrada na barra lateral, injetada no processo filho como `PORT` e alvo padrão de `open_key`. |
| `health` | string | nenhum | URL HTTP(S) usada como sonda de saúde no lugar da checagem simples da porta. O Atlas envia um `GET` de verdade (timeout de 2 s, redirecionamentos não seguidos): **2xx e 3xx são saudáveis**; 4xx/5xx, timeouts e erros de conexão não são. `https://` funciona. |
| `ready_log` | regex | nenhum | Uma linha de stdout/stderr (sem ANSI) que case com esta regex do Rust marca o serviço como pronto. Ela substitui a sonda de porta/health desse serviço e satisfaz os dependentes `log_ready`. Precisa ser uma regex válida. |
| `depends_on` | lista ou map | nenhum | Condição de partida, veja [depends_on](#depends_on). |
| `restart` | `no` \| `on_failure` \| `always` | `no` | Veja [política de reinício](#política-de-reinício). |
| `max_restarts` | integer | `5` | Reinícios consecutivos antes de desistir (`Failed`). `0` = ilimitado. |
| `oneshot` | bool | `false` | O comando é uma tarefa que termina (migration, seed). Sair com `0` é sucesso (`stopped`, exibido como `done`), não uma falha. Implícito quando algum dependente usa `completed_successfully`. |
| `start_timeout` | segundos | `60` | Quanto tempo o serviço pode ficar em `starting` (porta, `health` ou `ready_log` não satisfeitos). Depois disso, o Atlas adiciona uma nota de aviso única (`still starting after 60s: waiting for :3001`) e, com `critical: true`, dispara o alerta crítico. O serviço **não** é encerrado. Até lá, a barra lateral mostra `waiting for :3001 (12s)` / `waiting for ready_log (12s)`. `0` desativa o timeout. |
| `autostart` | bool | `true` | `false` = não inicia no boot; o serviço fica `stopped` até você iniciá-lo pelo menu do serviço (`m`), pela paleta de comandos ou pelo MCP (`atlas_start_service`), ou nomeá-lo na linha de comando (`atlas dev lab`, ou por `--profile`; um serviço lazy puxado só como dependência ainda espera). O início não espera o `depends_on`. `atlas config check` avisa quando um serviço com autostart depende de um que não tem. |
| `critical` | bool | `false` | Quando o serviço está `Failed`, a barra lateral mostra uma entrada **ISSUES** bloqueante com a última linha de log do serviço. |
| `critical_help` | string | nenhum | Dica mostrada sob esse alerta (só faz sentido com `critical: true`). |
| `enabled` | bool | `true` | `false` ignora o serviço por completo. Usado sobretudo a partir do `atlas.local.yaml`. |
| `env` | map string→string | `{}` | Variáveis de ambiente extras só para este serviço. |
| `open_key` | um caractere | nenhum | Tecla do dashboard que abre o serviço no navegador. |
| `open_url` | string | `http://localhost:<port>` | URL aberta por `open_key` no lugar da padrão. |
| `hide_logs` | bool | `false` | Sem aba própria para este serviço, e as linhas dele ficam fora da aba **All**. Use para infra silenciosa, como bancos de dados. Os logs continuam gravados em `.logs/<name>.log`. |
| `lock_file` | string | nenhum | Caminho (relativo à raiz do projeto) de um arquivo JSON com `{"pid": N}`, por exemplo `apps/ai/.mastra/dev.lock`. O Atlas limpa locks obsoletos antes de o serviço iniciar, veja [lock files](runtime.md#arquivos-de-lock-lock_file). |
| `max_memory` | tamanho (`512M`, `4G`) | nenhum | Teto rígido de memória para toda a árvore de processos do serviço. O Atlas mede memória e CPU a cada 5 s (no macOS, o physical footprint, com páginas comprimidas incluídas, como no Activity Monitor); acima do teto por duas medições seguidas, `on_memory_max` decide: `restart` (padrão) reinicia com um alerta e, depois de 3 reinícios numa execução, para o serviço e o marca como failed; `notify` só alerta. Uma proteção contra dev servers com vazamento. Veja [Memória](#memória). O uso aparece por serviço no Atlas Desktop e em `atlas_status` (`mem_mb`, `cpu`). |
| `memory_warn` | tamanho (`512M`, `1.5GB`) | nenhum | Orçamento suave, abaixo de `max_memory`: acima dele por duas medições seguidas, o serviço passa a `degraded` com o motivo (`using 2.1 GB, over memory_warn 1.5 GB`) e nada é encerrado. Veja [Memória](#memória). |
| `on_memory_max` | `restart` \| `notify` | `restart` | O que o Atlas faz quando o serviço continua acima de `max_memory`: `restart` (limitado a 3 por execução, depois `failed`) ou `notify` (um alerta, o serviço continua rodando e fica `degraded`). |
| `health_timeout` | segundos | `2` | Quanto tempo uma sonda de liveness (o `GET` de `health`, ou a conexão TCP a `port`) pode levar antes de contar como falha. Veja [Saúde e prontidão](#saúde-e-prontidão). |
| `failure_threshold` | integer | `3` | Falhas de liveness seguidas, com o processo vivo e depois de ele ter respondido uma vez, antes de o serviço virar `unhealthy`. `0` é tratado como `1`. |
| `ready` | objeto | nenhum | Sonda de readiness, separada da liveness: `http` (caminho ou URL) com um `status` opcional, ou um `command`, mais `timeout`. Enquanto a liveness passa mas esta falha, o serviço fica `degraded`. Veja [Saúde e prontidão](#saúde-e-prontidão). |
| `on_unhealthy` | `notify` \| `restart` | `notify` | O que o Atlas faz quando o serviço vira `unhealthy`: `notify` mostra o estado, levanta um alerta e mantém o contexto do log; `restart` também o reinicia (mesmo orçamento e mesmos atrasos de `max_restarts`). |
| `remote` | objeto | nenhum | O serviço roda em outra máquina; o Atlas encaminha a `port` local até lá por SSH em vez de rodá-lo. `ssh` (um host de `~/.ssh/config` ou `user@host`), `port` (onde o serviço escuta daquele lado), `host` (padrão `localhost`, visto do host SSH). Exige `port`, exclui `command`. Veja [Serviços remotos](#serviços-remotos). |

Observações:

- **Serviços sem comando** (serviços de "infra") com uma `port` são sondados depois de `docker compose up -d`: o Atlas espera até 30 s a porta aceitar conexões e então marca o serviço como `running` ou `failed`. Um serviço sem comando e sem porta nunca é sondado.
- **Ambiente** de um serviço iniciado, da menor para a maior precedência: ambiente herdado < [`env_files`](#env_files) < o `env:` do serviço < variáveis que o Atlas injeta (`PORT` quando `port` está definida, `FORCE_COLOR=1`, `BROWSER=none`; `NO_COLOR` é removida). O stdin é `/dev/null`.
- **Readiness** sem `ready_log`: um `GET` bem-sucedido na URL de `health`, senão uma conexão TCP a `port`, senão o processo estar vivo. Um comando sem porta, sem health e sem `ready_log` fica `running` depois de ficar vivo por **2 segundos** (um processo que cai dentro dessa janela, por exemplo num loop de reinício, nunca fica verde e nunca satisfaz um dependente `healthy`). `atlas config check` imprime um `warning:` não fatal (o código de saída continua `0`) quando uma dependência `healthy` aponta para um serviço assim.
- Comandos que começam com `docker` recebem um `DOCKER_HOST` apontado para o socket do OrbStack quando ele existe (a menos que `DOCKER_HOST` já esteja definido).
- Os valores de `open_key` e de `key` das ações não podem colidir com as [teclas embutidas](tui.md#atalhos-de-teclado) nem entre si; `atlas config check` rejeita conflitos.

### Saúde e prontidão

Duas perguntas separadas, duas configurações separadas:

- **Liveness** (está respondendo?) é a sonda da URL `health` ou de `port`. Cada sonda espera `health_timeout` segundos (padrão `2`). Depois que o serviço respondeu pelo menos uma vez, `failure_threshold` falhas seguidas (padrão `3`) com o processo ainda vivo o tornam `unhealthy`. Uma única resposta zera a contagem, e as falhas antes da primeira resposta nunca contam (a partida é assunto do [`start_timeout`](#services)). Um serviço com `ready_log` não é sondado pela porta.
- **Readiness** (está pronto para uso?) é a sonda opcional `ready:`, executada a cada tick de saúde **só enquanto a liveness passa** (os endpoints de um serviço travado também dariam timeout, e `unhealthy` já diz isso). Um serviço sem `ready:` se comporta como antes.

```yaml
services:
  api:
    command: bun run dev:api
    port: 24310
    health: http://localhost:24310/health     # liveness
    health_timeout: 2                         # seconds per probe (default 2)
    failure_threshold: 3                      # misses in a row before `unhealthy` (default 3)
    on_unhealthy: restart                     # notify (default) | restart
    ready:                                    # readiness
      http: /ready                            # path on the service `port`, or a full http(s):// URL
      status: 200                             # optional; default: any 2xx
      timeout: 5                              # seconds per probe (default 5)
  worker:
    command: bun run dev:worker
    ready:
      command: ./scripts/queue-ready.sh       # sh -c in the service `dir`; exit 0 = ready
```

| Campo de `ready` | Tipo | Padrão | Descrição |
|---|---|---|---|
| `http` | string | nenhum | Um caminho (`/ready`), resolvido contra a `port` do serviço em `127.0.0.1`, ou uma URL `http://` / `https://` completa. Um caminho puro sem `port` no serviço reporta `ready http '/ready' needs the service port`. |
| `status` | integer | qualquer `2xx` | Código de status exato esperado de `http`. Qualquer outro código é uma falha (`GET /ready → 503 (expected 200)`). |
| `command` | string | nenhum | Comando de shell executado com `sh -c` no `dir` do serviço (raiz do projeto quando não definido), stdin `/dev/null`; sair com `0` = pronto. Uma falha reporta o código de saída e a última linha não vazia do stderr. |
| `timeout` | segundos | `5` | Quanto tempo uma sonda de readiness pode levar (`` `cmd` timed out after 5s `` para um comando). |

Defina **exatamente um** entre `http` e `command`; `atlas config check` rejeita os dois ou nenhum. Chaves desconhecidas em `ready` são erros, como em todo lugar.

Qual é o veredito para um serviço com processo vivo:

| Liveness | `ready` / log | Estado |
|---|---|---|
| responde | passa (ou nenhuma configurada), sem erro repetido | `running` |
| responde | a sonda `ready` falha | `degraded`, motivo `ready probe failed: <error>` |
| responde | a mesma linha de erro 3 vezes em 60 s (números, timestamps, UUIDs e ids hex normalizados, então `timeout after 31ms` e `timeout after 87ms` contam como um) | `degraded`, motivo `repeating error: <line>` |
| falha, menos de `failure_threshold` vezes | | sem mudança (o hot restart de um dev server fecha a porta por um instante) |
| falha `failure_threshold` vezes seguidas depois de responder uma vez | | `unhealthy` |

`unhealthy` vence `degraded`, e uma sonda `ready` com falha vence uma linha de log repetida (ela traz o erro do próprio endpoint). `degraded` some sozinho quando a sonda volta a passar ou o erro para de se repetir; `unhealthy` some quando o serviço volta a responder. `unhealthy` é para um processo **vivo**: um processo que sai fica `failed`, como antes, e um serviço que nunca esteve saudável continua `starting`. Conceitos: [Estados do serviço](concepts.md#serviço). Detalhes de execução: [Estados de saúde](runtime.md#unhealthy-e-degraded).

### Memória

O Atlas mede a memória da árvore de processos inteira de cada serviço a cada 5 s (no macOS, o physical footprint, com páginas comprimidas incluídas, como no Activity Monitor) e guarda a última hora (720 medições) por serviço. Dois orçamentos opcionais agem sobre isso, e a vigilância de vazamento não precisa de configuração:

```yaml
services:
  web:
    command: bun run dev:web
    memory_warn: 1.5GB          # soft: over it for two samples in a row → `degraded`, nothing is killed
    max_memory: 4GB             # hard: over it for two samples in a row → on_memory_max
    on_memory_max: restart      # restart (default) | notify
```

| Configuração | O que acontece |
|---|---|
| `memory_warn` | Acima do orçamento por **duas medições seguidas** (~10 s; um pico de build não é um estado), o serviço passa a `degraded` com o motivo `using 2.1 GB, over memory_warn 1.5 GB`. Abaixo dele por duas medições seguidas, volta a `running` sozinho. Precisa ficar abaixo de `max_memory` (`atlas config check` rejeita o contrário). Os tamanhos são lidos como em `max_memory`: `512M`, `1.5GB`, `2048`. |
| `max_memory` + `on_memory_max: restart` (padrão) | Acima do teto por duas medições seguidas, o serviço é reiniciado com um alerta (`using 5.0 GB (max_memory 4.0 GB), restarting (1/3)`); depois de 3 reinícios assim numa execução, ele é parado e marcado como `failed`. |
| `max_memory` + `on_memory_max: notify` | Acima do teto por duas medições seguidas, o Atlas levanta um alerta e deixa o serviço rodando, `degraded` com `using 5.0 GB, over max_memory 4.0 GB` até ele cair abaixo. Para serviços que você prefere reiniciar por conta própria (um banco de dados, um build longo). |

Precedência entre as outras causas de `degraded`: `unhealthy` vence tudo; uma sonda `ready:` com falha vence um erro repetido, que vence o estouro de memória (o achado menos específico: os outros dizem o que está quebrado, este diz que está grande). Enquanto um erro de sonda é o motivo, a memória cair nunca devolve o serviço a `running`; quando a sonda passa e a memória ainda está acima do orçamento, o motivo de memória é o que sobra. Um serviço que não está `running` (starting, failed, stopped) nunca fica degraded por memória.

**Tendência e vazamentos.** Da série, o Atlas deriva um `memory_trend`: `rising`, `flat` ou `falling` (uma inclinação por mínimos quadrados nos últimos 5 minutos, precisa de ~30 s de dados; abaixo de ±2 MB/min é `flat`). Ele também observa o tamanho no fim de todo **reload** (uma linha `[bun] reloaded`, `[vite] hmr update`, `✓ Compiled`, `[nodemon] restarting`, … na saída do serviço; linhas de uma mesma mudança contam uma vez) e de todo **reinício**: um dev server saudável volta a cerca do mesmo tamanho depois de um reload, e um com vazamento cresce a cada vez. Quatro ciclos seguidos sem uma queda de mais de 3% e com pelo menos 64 MB de crescimento no total levantam um alerta, `leak suspected: +600 MB over 12 reloads` (de novo só depois de mais 4 ciclos de crescimento), e o texto acompanha a nota do serviço. Um vazamento suspeito é um aviso, nunca um estado próprio.

Onde aparece: a linha da barra lateral (`memory 2.1 GB · rising +42 MB/min`, `budget warn`, o vazamento), `atlas dev --status` (uma linha `↳` sob um serviço que tenha algo a dizer), o card do Atlas Desktop, e `memory_rss`, `memory_trend`, `memory_trend_mb_min`, `budget_state` e `memory_leak` no arquivo de status e no [`atlas_status`](mcp.md#atlas_status) do MCP; a série inteira é o [`atlas_metrics`](mcp.md#atlas_metrics) do MCP.

### depends_on

Duas formas. A forma de lista espera até cada dependência estar `healthy`:

```yaml
depends_on: [database, redis]
# identical to
depends_on: { database: healthy, redis: healthy }
```

A forma de map escolhe uma condição por dependência:

| Condição | Satisfeita quando |
|---|---|
| `started` | o processo da dependência foi iniciado (para serviços sem comando: a espera pela porta terminou, de um jeito ou de outro) |
| `healthy` (padrão) | a dependência está `running`: a sonda de porta/`health` passou, ou o `ready_log` dela casou |
| `completed_successfully` | o comando da dependência saiu com `0`. O alvo precisa de um `command`; ele é tratado como `oneshot`. |
| `log_ready` | apareceu uma linha que casa com o `ready_log` da dependência. O alvo precisa definir `ready_log`. |

```yaml
services:
  migrate:
    command: bun run db:migrate         # exits 0 when done
    depends_on: { database: healthy }
  api:
    command: bun run dev:api
    port: 3001
    ready_log: 'listening on'
    depends_on: { migrate: completed_successfully }
  web:
    command: bun run dev:web
    depends_on: { api: log_ready }
```

Comportamento:

- Um serviço só é iniciado quando **todas** as condições dele valem. Até lá, a barra lateral mostra `↳ waiting for migrate (completed_successfully)`.
- Se uma condição nunca puder ser cumprida (a dependência falhou, saiu com código diferente de zero ou desistiu de reiniciar), o dependente fica `failed` com o motivo `dependency <name> failed`, e a falha se propaga pela cadeia. Ele nunca espera para sempre.
- Uma dependência que está apenas reiniciando sob a [política de reinício](#política-de-reinício) não é uma falha; os dependentes continuam esperando.
- Um `oneshot` que sai com `0` termina como `stopped` (`done`) e conta como OK no [`atlas dev --ci`](cli.md#atlas-dev).

### Política de reinício

```yaml
services:
  worker:
    command: bun run worker
    restart: on_failure   # no (default) | on_failure | always
    max_restarts: 5       # default 5, 0 = unlimited
```

| `restart` | Reinicia quando o processo filho sai… |
|---|---|
| `no` | nunca |
| `on_failure` | com código diferente de zero ou morto; uma saída limpa com `0` apenas termina (`exited cleanly`) |
| `always` | em qualquer saída |

- Só saídas que o processo filho faz **por conta própria** a disparam. Parar ou reiniciar pela TUI/MCP, sair do Atlas e serviços pausados por um `pause_services` de uma ação nunca são reiniciados automaticamente.
- Backoff antes do reinício *n* (começando em 0): 1 s, 2 s, 4 s, 8 s, 16 s, depois um teto de 30 s.
- O contador zera depois que o serviço fica `running` por 60 s.
- Durante a espera, a barra lateral mostra `↳ restarting in 4s (2/5)`; depois, `↳ restarted ×N`. Quando o orçamento acaba, o serviço fica `failed` com `gave up after N restarts`.
- Evite `restart: always` numa tarefa de execução única: ela é relançada depois de cada sucesso.

### Serviços remotos

Um serviço pesado (um banco de dados, um runtime de IA, um estúdio de vídeo) pode rodar numa máquina mais forte enquanto o resto da stack roda localmente:

```yaml
services:
  db:
    port: 5432                                     # what the local stack connects to
    remote: { ssh: dev-box, port: 5432 }           # where it really listens, seen from dev-box
  ai:
    port: 8000
    remote: { ssh: me@gpu.lan, port: 8000, host: 127.0.0.1 }
    health: http://localhost:8000/health           # probed through the forward
```

- O Atlas não roda nenhuma cópia local. O processo dele é `ssh -N -L <port>:<host>:<remote port> <ssh>` com `BatchMode=yes` (só autenticação por chave: nunca pergunta nada), `ExitOnForwardFailure=yes` (uma porta local ocupada é uma falha, não um no-op silencioso) e keepalives que percebem um link morto em cerca de 45 s.
- No resto, é um serviço comum: estado, a sonda de porta, logs (a saída do ssh é o log do serviço), `depends_on`, profiles, MCP. A porta local do encaminhamento segue o offset de porta por worktree do Atlas Desktop.
- Uma conexão que cai é a forma normal de um encaminhamento terminar, então, sem um `restart:` explícito, ele reinicia com `always` (backoff e `max_restarts` como de costume).
- A sonda de porta só prova que o encaminhamento está de pé; adicione uma URL `health` (ou uma sonda `ready`) para saber se o serviço remoto em si responde.
- `ssh` e `host` precisam ser palavras simples (sem espaços, nunca começando com `-`): elas vão para uma linha de comando do ssh.

## profiles

Grupos nomeados de serviços, selecionados com `atlas dev --profile NAME` (repetível; profiles e `SERVICE`s posicionais são unidos):

```yaml
profiles:
  backend: [server, worker]
  front: [web]
```

```text
atlas dev --profile backend                    # server, worker + their depends_on (migrate, database, redis…)
atlas dev --profile backend --profile front    # union
atlas dev --profile front --no-deps            # web only
```

- Os membros precisam ser nomes em `services:`; qualquer outra coisa é um erro de config com uma dica "did you mean" (`profiles.backend[0]: unknown service 'servr' (did you mean 'server'?)`).
- Uma seleção sempre inclui os [`depends_on`](#depends_on) transitivos dos seus membros (infra como `database` só quando algo selecionado precisa dela); `--no-deps` descarta isso. Os serviços deixados de fora aparecem como `off` e podem ser iniciados à mão. Detalhes e códigos de saída: [`atlas dev` › Selecionar serviços](cli.md#selecionar-serviços).
- Sem `--profile` / `SERVICE`, todo serviço inicia: os profiles nunca mudam a execução padrão.
- O `atlas.local.yaml` pode adicionar ou substituir profiles por nome (veja [Overrides locais](#overrides-locais-atlaslocalyaml)).

## infra

```yaml
infra:
  compose_file: packages/infra/dev/compose.yml   # relative to the project root
```

| Campo | Tipo | Descrição |
|---|---|---|
| `compose_file` | string, **obrigatório** | Arquivo compose. O Atlas roda `docker compose -f <file> up -d` (esperando até 15 s pelo daemon do docker, com uma nova tentativa em caso de falha) antes de iniciar os serviços do app. |

Se o compose falha, o Atlas levanta um alerta crítico (`docker`), marca como `failed` os serviços sem comando que têm uma porta e ainda assim inicia os serviços do app. Sair do Atlas **não** roda `docker compose down`; veja [Runtime](runtime.md#encerramento).

## tunnel

```yaml
tunnel:
  enabled: false
  local_port: 54320
  remote_host: localhost
  remote_port: 5432
  ssh_host: prod-db          # entry in ~/.ssh/config
  keepalive_interval: 15
  keepalive_max: 3
  reconnect_cooldown: 30
```

| Campo | Tipo | Padrão | Descrição |
|---|---|---|---|
| `enabled` | bool | `true` | `false` mantém o bloco, mas desliga o túnel. |
| `local_port` | integer | **obrigatório** | Porta local encaminhada. |
| `remote_host` | string | **obrigatório** | Host do lado remoto. |
| `remote_port` | integer | **obrigatório** | Porta do lado remoto. |
| `ssh_host` | string | **obrigatório** | Alias do host SSH. |
| `keepalive_interval` | integer | `15` | `ServerAliveInterval` (segundos). |
| `keepalive_max` | integer | `3` | `ServerAliveCountMax`. |
| `reconnect_cooldown` | integer | `30` | Segundos entre tentativas de reconexão. |

Quando o bloco existe e `enabled` não é `false`, o Atlas abre o túnel **no boot, em segundo plano** (a interface nunca espera por ele):

```
ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=<keepalive_interval> \
    -o ServerAliveCountMax=<keepalive_max> -o BatchMode=yes \
    -L <local_port>:<remote_host>:<remote_port> <ssh_host>
```

- `BatchMode=yes`: o ssh nunca pergunta nada, então uma chave ausente ou um host key desconhecido falha rápido em vez de corromper a TUI. Antes de habilitar, confirme que `ssh <ssh_host>` funciona sem interação (agente carregado, host key aceito).
- O túnel está **pronto** quando `local_port` aceita uma conexão TCP (até 15 s).
- O ssh roda na própria sessão (`setsid`, stdin `/dev/null`); o stderr dele vai para a aba de log `tunnel` e para `.logs/tunnel.log`.
- A saúde é verificada a cada segundo. Se o ssh morre ou a porta local para de responder, o Atlas reconecta com backoff exponencial: 5 s, 10 s, 20 s, … até o teto de `reconnect_cooldown` segundos. Uma troca do gateway padrão (mudar de Wi-Fi/VPN, no macOS) reconecta na hora.
- Se o túnel não consegue subir, o Atlas levanta um **alerta crítico** chamado `tunnel`, cuja mensagem termina com a última linha de stderr do ssh (uma vez por episódio de falha, não a cada nova tentativa).
- Se algo já aceita conexões em `local_port` e o Atlas não o iniciou, o Atlas o usa como está e nunca o encerra. O Atlas nunca encerra por porta.
- O estado aparece no bloco **TUNNEL** da barra lateral, em `atlas dev --status`, em `.logs/.tui-status.json` e no `atlas_status` do MCP (`tunnel: {state, local_port, ssh_host}`, `null` quando nenhum túnel está rodando). Veja [Runtime](runtime.md#túnel-ssh).

Com `enabled: false`, o bloco continua configurado, mas nada inicia no boot; uma ação com [`open_tunnel: true`](#actions) o inicia sob demanda.

Na saída, o túnel é fechado (SIGTERM ao ssh que o Atlas iniciou), a menos que você tenha desligado **Kill tunnel on exit** com `t` na TUI; nesse caso ele continua rodando e fica registrado em `.logs/.tunnel.pid` para a próxima partida reaproveitá-lo. `--headless` e `--ci` sempre o fecham.

## actions

Comandos personalizados ligados a uma tecla, mostrados na ajuda (`?`) e na paleta de comandos (`:`).

```yaml
actions:
  - key: c
    label: Run check
    command: bun run check

  - key: u
    label: DB refresh local
    command: bun run db:refresh-local
    pause_services: [server, worker]

  - key: R
    label: Reset local DB
    command: bun run db:reset
    confirm: "This wipes the local database. Continue?"
```

| Campo | Tipo | Padrão | Descrição |
|---|---|---|---|
| `key` | um caractere | **obrigatório** | Diferencia maiúsculas de minúsculas. Não pode ser uma [tecla embutida](tui.md#atalhos-de-teclado), o `open_key` de um serviço nem ser usada por outra ação. |
| `label` | string | **obrigatório** | Nome na ajuda, na paleta, na barra lateral e nos toasts, e o que o [MCP](mcp.md) usa para casar (sem diferenciar maiúsculas). |
| `command` | string | **obrigatório** | Executado com `sh -c` na raiz do projeto, com os `env_files` aplicados. |
| `confirm` | string | nenhum | Mostra um modal de confirmação (título = `label`, corpo = este texto) antes de executar, qualquer que seja o gatilho. `Enter`/`y` executa, `Esc`/`n` cancela. Com `pause_services`, o modal acrescenta uma linha `Pauses: web, api`. Pelo MCP, a ação é recusada até a chamada passar `confirmed: true`. |
| `pause_services` | lista de strings | `[]` | Serviços parados (estado `suspended`) antes de o comando rodar e reiniciados quando ele termina. Só os serviços que estão `running` ou `starting` quando a ação começa são pausados, e só esses são reiniciados; um que você tinha parado continua parado. |
| `session` | bool | `false` | Abre uma aba interativa dedicada que transmite stdout/stderr, com stdin anexado. Veja [sessions](tui.md#sessões-e-anexar). |
| `input_map` | map string→string | `{}` | Só em session: quando anexado, apertar uma tecla envia ao stdin a string mapeada em vez da própria tecla. |
| `platform` | `ios` \| `android` | nenhum | Só em session: mostra primeiro um seletor de dispositivo e injeta as [configurações mobile](#mobile). |
| `device_id` | string | nenhum | Fixa o nome de um simulador ou um serial do adb; pula o seletor. Por exemplo `"iPhone 16 Pro"`, `"emulator-5554"`. |
| `host_mode` | `auto` \| `lan` \| `simulator` | `auto` | Ações session com `platform`: qual host o dev server anuncia ao dispositivo. O Atlas define `TAURI_DEV_HOST` e `ATLAS_DEV_HOST` no ambiente da sessão (uma variável que você já definiu por `env_files`, pelo `env:` de um serviço ou pelo seu shell é mantida). `simulator` → `127.0.0.1`. `lan` → o IPv4 de LAN da interface da rota padrão (recorre a `127.0.0.1` se não for possível determiná-lo). `auto` → `lan` quando o dispositivo selecionado é físico (iOS: um dispositivo do `devicectl`; Android: qualquer serial que não comece com `emulator-`), senão `simulator`. Veja [host_mode](#host_mode). |
| `heavy` | bool | `false` | Roda o comando pela [fila de jobs pesados](cli.md#atlas-run---heavy) da máquina inteira (`atlas run --heavy --label <label> -- <command>`): no máximo N jobs pesados rodam ao mesmo tempo na máquina toda, qualquer que seja o projeto, a worktree ou o agente que os iniciou (padrão `min(cores / 2, GB of memory / 8)`), e os demais esperam em ordem, com a operação da barra lateral mostrando a espera. Use para ações que fazem build, rodam suítes de testes ou benchmarks. Também funciona em ações session. Precisa de um atlas que tenha a fila (versões mais antigas rejeitam esta chave: um repositório que precisa funcionar com qualquer atlas instalado chama um script wrapper, como o `scripts/heavy.sh` deste repositório). |
| `cache` | map | nenhum | Guarda em cache o resultado da ação para a máquina inteira ([`atlas run --cache`](cli.md#cache-de-tarefas-atlas-run---cache-atlas-cache)): uma execução idêntica (mesmo comando, mesmos bytes em `inputs`, mesmos valores de `env` e versões de `tools`) restaura `outputs`, reproduz o log e não roda nada; só uma execução bem-sucedida é armazenada. Chaves: `inputs` (globs relativos à raiz do projeto, `.gitignore` respeitado, edições não commitadas contam; omitido = todo arquivo que o git não ignoraria), `outputs` (arquivos ou pastas dentro do projeto, restaurados num acerto), `env` (nomes de variáveis cujos valores entram na chave), `tools` (programas cujo `--version` entra na chave). Combina com `heavy`. Chaves desconhecidas são erros de config. |
| `group` | string | nenhum | Rótulo que agrupa ações: a tela de ajuda (`?`) lista primeiro as ações sem grupo, depois cada grupo na ordem da primeira aparição, sob um cabeçalho; a paleta de comandos (`:`) prefixa as ações agrupadas com `Group ›` (e também casa o nome do grupo na busca). Uma string vazia conta como sem grupo. |
| `open_tunnel` | bool | `false` | Antes de o comando rodar, garante que o [`tunnel`](#tunnel) está de pé: ele é iniciado sob demanda mesmo com `tunnel.enabled: false`, esperando até 15 s (a operação da barra lateral mostra `opening tunnel…`). Se ele não conseguir subir, a ação **falha com o motivo e o comando não roda**. Um túnel que a ação precisou abrir sob demanda é **fechado quando a ação termina**, com sucesso ou falha (se várias ações `open_tunnel` se sobrepõem, a última o fecha; um túnel que já estava de pé, por exemplo com `enabled: true`, é deixado em paz). Exige um bloco `tunnel:`, senão a config é rejeitada (`actions[N].open_tunnel`). |

Comportamento de uma ação que não é session:

- Enquanto roda, a barra lateral mostra um bloco **OPERATIONS**: spinner, rótulo, tempo decorrido `mm:ss`, última linha de saída. O sucesso o limpa; a falha mantém `✗` e a última linha até a próxima execução ou por 30 s.
- Disparar a mesma ação enquanto ela ainda roda é ignorado ("already running").
- Quando termina, os serviços pausados são reiniciados (com sucesso ou falha).
- `atlas dev --headless` pode rodar ações pelo MCP, mas não ações com `platform` (elas precisam do seletor de dispositivo da TUI).

### host_mode

Para ações session com `platform`, o Atlas decide qual endereço de host o dev server deve anunciar e o exporta como `TAURI_DEV_HOST` e `ATLAS_DEV_HOST` (use a que a sua ferramenta ler, por exemplo `vite --host $ATLAS_DEV_HOST`):

| `host_mode` | Valor |
|---|---|
| `simulator` | `127.0.0.1` |
| `lan` | IPv4 de LAN da interface da rota padrão. macOS: `route -n get default` → interface → `ipconfig getifaddr <if>`. Linux: o `src` de `ip route get 1.1.1.1`. Recorre a `127.0.0.1` se não for possível determiná-lo. |
| `auto` (padrão) | `lan` quando o dispositivo selecionado é físico, senão `simulator`. iOS: um dispositivo listado pelo `xcrun devicectl` é físico, um simulador `simctl` não é. Android: um serial que começa com `emulator-` é um emulador, qualquer outro é hardware. Um dispositivo que o Atlas não consegue classificar (ou nenhum selecionado) é tratado como simulador. |

A consulta roda fora do runtime assíncrono. Uma variável que você já definiu (`env_files`, o `env:` de um serviço ou o ambiente de onde o Atlas foi iniciado) nunca é sobrescrita; a outra ainda é definida.

## mobile

Configurações das ações com `platform:`. Elas ficam perto de segredos, então mantenha-as no `atlas.local.yaml`.

```yaml
mobile:
  ios:
    team_id: A6FCTW4KP6        # exported to the action as APPLE_DEVELOPMENT_TEAM
    bundle_id: com.example.app # optional override
  android:
    keystore: ~/.android/debug.keystore
```

| Campo | Tipo | Descrição |
|---|---|---|
| `mobile.ios.team_id` | string | Team ID do Apple Developer. Exportado como `APPLE_DEVELOPMENT_TEAM` para as ações session de iOS. |
| `mobile.ios.bundle_id` | string | Override do bundle identifier. |
| `mobile.android.keystore` | string | Caminho do keystore para assinatura. |

Hoje o Atlas lê só o `team_id` (para definir `APPLE_DEVELOPMENT_TEAM`); os outros campos são validados e mantidos na config.

## env_files

```yaml
env_files:
  - .env
```

Arquivos no estilo dotenv (caminhos relativos à raiz do projeto), carregados em ordem (o último vence) e injetados em todo serviço e em toda ação/sessão. Arquivos ausentes são ignorados em silêncio. Sintaxe aceita: `KEY=VALUE`, `export KEY=VALUE`, aspas simples ou duplas, comentários com `#` (de linha inteira, ou no fim depois de um espaço em valores sem aspas), linhas em branco, `=` dentro de valores.

`env_files` só vale no `atlas.yaml`; o `atlas.local.yaml` não pode defini-lo.

O Atlas **não** limpa o ambiente que herda (não há allowlist): toda variável do shell que iniciou o Atlas chega aos serviços e às ações. Os `env_files` têm precedência **maior** que o ambiente herdado, então uma chave definida nos dois vem do arquivo. Variáveis que você quer forçar a ficar desligadas (por exemplo `MASTRA_SKIP_DOTENV`) precisam ser definidas explicitamente no `env:` de um serviço.

## worktree

```yaml
worktree:
  shared: [.env, node_modules]   # dirs symlinked, files copied from the main checkout
  seed: [node_modules, target]   # heavy dirs cloned copy-on-write before setup (omit = detect, [] = off)
  setup: 'bun install && {{action: Build web}} && bun run db:migrate'
  package_store: true            # bun/pnpm install from one machine-wide store (default; false = off)
  archive: docker compose -p "$ATLAS_WORKSPACE_NAME" down
  setup_timeout: 600             # seconds (default 600)
```

Usado pelo Atlas Desktop quando ele cria ou exclui uma worktree git deste repositório. O `setup` roda com `sh -c` na worktree nova, numa aba de terminal chamada "Setup"; o `archive` roda antes da exclusão. Os dois recebem `ATLAS_ROOT_PATH` (checkout principal), `ATLAS_WORKTREE_PATH`, `ATLAS_WORKSPACE_NAME` e `ATLAS_PORT_OFFSET`.

### Seed de diretórios

Uma worktree nova começa sem `node_modules` e sem `target/`, então o `setup` reinstala e recompila tudo (um `target/` frio tem de 2 a 4 GB e cerca de 100.000 arquivos). Com `seed`, o Atlas clona esses diretórios do checkout principal **antes de o `setup` rodar**, então `bun install` e `cargo build` só fazem a diferença.

- **O que recebe seed.** Omitido: detectado a partir da raiz do repositório, `node_modules` com um `package.json`, `target` com um `Cargo.toml`. Uma lista nomeia exatamente esses diretórios (caminhos relativos, sem `..`); `seed: []` desliga. Um diretório que o checkout principal não tem, ou que a worktree nova já tem (o git o extraiu, ou o `shared` o ligou), é deixado em paz. O `shared` vence o `seed` para o mesmo caminho.
- **Sem seed: virtualenvs do Python.** Um virtualenv não é relocável: os shebangs de `bin/*` e o `pyvenv.cfg` guardam o caminho absoluto do checkout principal, então um clone continuaria rodando o interpretador e o `site-packages` do checkout principal (as instalações editáveis da worktree ficariam sem uso, em silêncio). Qualquer diretório listado com um `pyvenv.cfg` (`.venv`, `venv`, um nome personalizado) é recusado com uma nota visível, e `.venv` nunca é detectado automaticamente. Recrie-o no `setup` (`uv sync`, `python -m venv .venv && pip install -e .`).
- **`node_modules`.** As entradas de `node_modules/.bin` são symlinks relativos e sobrevivem ao clone. Algumas ferramentas guardam caminhos absolutos em caches dentro dele (por exemplo `node_modules/.cache`, `.vite`, `.astro`, caches do Babel/webpack/ESLint indexados por caminho) e addons nativos compilados no lugar podem embutir o caminho de origem: isso é recompilado ou invalidado na primeira execução na worktree, no pior caso custando a partida a frio daquela ferramenta; tire `node_modules` do `seed` se algo se comportar mal.
- **Como.** Um clone copy-on-write: `clonefile(2)` no macOS (APFS), `FICLONE` por arquivo no Linux (btrfs, XFS com reflink, bcachefs). Os blocos são compartilhados até um dos lados escrever, então leva milissegundos e nenhum disco, e escrever na worktree nunca muda o checkout principal (o Atlas nunca faz hardlink de uma árvore: uma escrita chegaria à origem). No macOS, o `target` e o `node_modules` com seed recebem um arquivo `.metadata_never_index`, para o Spotlight não indexá-los.
- **Quando não dá para clonar** (outro volume, ext4, tmpfs, Windows: o clone de blocos no ReFS não é tentado), o diretório é **pulado, nunca copiado por inteiro**, e a linha de progresso e o toast de pronto avisam isso. Checkouts remotos (SSH) não recebem seed.
- **Cargo.** Um `target/` clonado é seguro para compilar e testar. O Cargo mistura o caminho absoluto do workspace no hash do fingerprint e nos nomes de arquivo de cada crate, então os crates da própria worktree e os binários de teste são recompilados com nomes novos (os caminhos `CARGO_BIN_EXE_*` embutidos nos binários de teste do checkout principal ficam em arquivos que ninguém executa), enquanto as dependências do registry, a parte cara, são reaproveitadas. O clone é dono dos próprios blocos, então excluir o checkout principal depois não quebra nada. Por isso, nunca compartilhe um `CARGO_TARGET_DIR` em uso entre worktrees.
- **Tempo.** A etapa de progresso `seed` traz `cloned … in N ms`; quando o terminal Setup termina, um toast acrescenta `setup N s`. Os mesmos números estão no payload `done` de `worktree://progress` (`seed: {seeded, skipped, ms}`).

### Etapas em cache no `setup`

Um `{{action: <label or key>}}` no `setup` é substituído pelo comando daquela ação em `actions:`. Quando a ação tem um [bloco `cache:`](#actions), a etapa roda por `atlas run --cache`: uma worktree nova cujos arquivos de entrada têm os mesmos bytes de uma que já compilou **restaura as saídas do cache de tarefas e não roda nada**. A chave do cache ignora o diretório, então o build do checkout principal serve a todas as worktrees até as fontes diferirem.

```yaml
actions:
  - key: b
    label: Build web
    command: bun run build
    cache: { inputs: ['src/**', bun.lock], outputs: [dist] }
worktree:
  setup: 'bun install && {{action: Build web}}'   # quote it: `: ` starts a mapping in a plain YAML scalar
```

Uma ação sem `cache:` é inserida como o próprio comando simples (ainda atrás da fila de jobs pesados quando `heavy: true`). Uma referência que não nomeia nenhum label ou key de ação é um erro de config (`atlas config check`), então um erro de digitação nunca deixa uma worktree pela metade.

### Package store (`package_store`)

Enquanto o `setup` roda, um projeto bun ou pnpm instala a partir de um store único da máquina em `~/.atlas/pkgstore` (`ATLAS_PKGSTORE_DIR` o move): `BUN_INSTALL_CACHE_DIR` para o bun, `npm_config_store_dir` (o `store-dir` do pnpm) para o pnpm. Um pacote é baixado e descompactado uma vez; o `node_modules` de cada worktree é clonado ou ligado por hardlink a partir dele pelo gerenciador de pacotes. O gerenciador é detectado por `bun.lock`/`bun.lockb` ou `pnpm-lock.yaml`; npm e yarn ficam de fora, e uma variável de store que já esteja no seu ambiente vence. `package_store: false` desliga. Só checkouts locais.

### Qual a velocidade de uma worktree nova

`cargo test -p atlas-cli --test worktree_fast_start -- --nocapture` cria worktrees reais com `git worktree add` de um repositório de fixture (um `node_modules` de 1.500 arquivos, um build de 1,5 s como substituto numa ação em cache) e imprime o tempo até ficar pronta. Medido num Mac Apple M-series, APFS: **a frio 1,68 s** (instalação e build rodam), **a quente 47 ms** (`node_modules` clonado em 13 ms, build restaurado do cache em 8 ms). Os substitutos são custos fixos, então isso prova o mecanismo, não os tempos do próprio bun ou cargo: num projeto real, o caminho a quente é a passada incremental do gerenciador de pacotes mais o que não está em cache. O limite de aceitação da issue (um fluxo parecido com issue:start abaixo de 30 s) é verificado por esse teste, que também roda onde o sistema de arquivos não consegue clonar (o seed é então pulado e só conta a economia do cache).

### Offset de porta

Cada worktree recebe um offset de porta (100, 200, …). Quando `ATLAS_PORT_OFFSET=N` está definido, o atlas soma `N` à `port` de todo serviço, a `tunnel.local_port`, a todo valor de `env` de serviço igual à porta de algum serviço e ao `:<port>` em `open_url`/`health`. Experimente: `ATLAS_PORT_OFFSET=100 atlas config check`. O título da TUI mostra `ports +N`.

## affected

A resposta do próprio projeto para "o que esta mudança afeta?", lida pela ferramenta MCP [`atlas_affected`](mcp.md#atlas_affected) para um agente rodar os testes, rotas e builds que uma mudança pode quebrar, em vez da suíte inteira. Duas formas, as duas opcionais; sem o bloco, o `atlas_affected` recorre às ações cujos [`cache.inputs`](#actions) casam com os arquivos alterados.

```yaml
affected:
  command: bun scripts/affected.ts --json   # a plugin: changed files on stdin, JSON on stdout
  rules:                                    # or declare the graph inline
    - match: ['crates/atlas-core/**']
      tasks: [Run checks]                   # action labels
      tests: ['cargo test -p atlas-core']
      routes: [/docs]
      builds: [site]
```

| Chave | Tipo | Descrição |
|---|---|---|
| `command` | string | Executado com `sh -c` na raiz do projeto (`cmd /c` no Windows). Recebe os arquivos alterados no stdin, um por linha, relativos à raiz do projeto, e imprime no stdout o JSON `{"tasks": [], "tests": [], "routes": [], "builds": []}` (toda chave é opcional, cada uma uma lista de strings; outras chaves são ignoradas). Um código de saída diferente de zero ou uma saída que não seja esse JSON é um erro devolvido ao agente, nunca um fallback silencioso. Vence `rules`. |
| `rules` | lista | `match` (globs relativos à raiz do projeto, `**` atravessa pastas; pelo menos um) e qualquer um entre `tasks` (precisa ser o `label` de uma ação, sem diferenciar maiúsculas), `tests`, `routes`, `builds`. Uma regra se aplica quando qualquer arquivo alterado casa com qualquer um dos globs dela; a resposta é a união das regras que casam, sem duplicatas, na ordem dos arquivos. |

Chaves desconhecidas são erros de config; também são uma regra sem `match`, um glob que não é interpretado, um `command` em branco e uma entrada de `tasks` que não nomeia nenhuma ação. O `atlas.local.yaml` pode sobrescrever `affected` como qualquer outra chave de nível superior. Este repositório declara `affected: { command: bun scripts/check-changed.ts --json --stdin }`: o plano que `bun run check:changed` executaria para os arquivos do stdin, como JSON, sem executá-lo.

## agents

O que um agente de IA pode fazer pelas [ferramentas de controle do MCP](mcp.md#permissões-do-agente-e-auditoria) (`atlas_restart_service`, `atlas_stop_service`, `atlas_start_service`, `atlas_run_action`). Sem um bloco `agents:`, nada é restrito. A chave de uma linha para deixar só com leitura:

```yaml
agents:
  control: deny
```

| Chave | Valor | Significado |
|---|---|---|
| `control` | `allow` \| `deny` | Padrão para toda ferramenta de controle sem entrada própria. |
| `restart_service`, `stop_service`, `start_service` | `allow`, `deny`, `{ allow: [services] }` ou `{ deny: [services] }` | Por ferramenta. Uma lista nomeia serviços: `allow` admite só esses, `deny` recusa só esses. |
| `run_action` | igual | As listas nomeiam ações por key ou label (sem diferenciar maiúsculas), o que o agente usou. |

Precedência: a entrada própria da ferramenta, depois `control`, depois allow. Um `allow: []` vazio recusa tudo naquela ferramenta; um `deny: []` vazio não recusa nada.

```yaml
agents:
  control: deny
  restart_service: { allow: [worker, web] }   # may restart these two, nothing else
  run_action: { deny: [Reset local DB] }      # overrides control: deny for run_action: all actions except this one
```

As ferramentas de leitura (status, logs, config, esperar ficar pronto) nunca são restritas, e você também não: a TUI, o Atlas Desktop e a CLI ignoram a política. O atlas dev a aplica no socket de controle, então todo cliente MCP é coberto; uma chamada recusada devolve `this project's atlas.yaml denies agents restart_service on api: ask the developer to run it, …`. Toda chamada, permitida ou não, é registrada em `.logs/atlas-audit.log`.

`agents:` só vale no `atlas.yaml`; o `atlas.local.yaml` não pode defini-lo nem afrouxá-lo (ele é reportado como campo desconhecido). A política é lida quando o `atlas dev` inicia. Uma chave com erro de grafia (`restart_servce`) é um erro de config, nunca um projeto aberto em silêncio; o `atlas config check` a reporta.

## Overrides locais (`atlas.local.yaml`)

O `atlas.local.yaml` é mesclado sobre o `atlas.yaml` no momento do carregamento. Faça commit do `atlas.yaml`; coloque o `atlas.local.yaml` no gitignore.

```yaml
name: myapp-local
pre_command: ""                  # skip dependency install on this machine

services:
  web:
    port: 3002
  redis:
    enabled: false

tunnel:
  ssh_host: my-personal-alias

actions:
  - key: S                       # replaces the action with key S, or adds it
    label: Seed (local fixtures)
    command: cd apps/api && bun run db:seed:local

mobile:
  ios:
    team_id: A6FCTW4KP6
```

O que pode ser sobrescrito:

| Chave | Regra de mesclagem |
|---|---|
| `name` | substitui |
| `pre_command` | não vazio substitui; `""` desativa; omitido mantém o base |
| `pre_command_when` | substitui |
| `services.<name>` | campo a campo para `command`, `port`, `health`, `critical`, `depends_on`, `enabled`, `restart`, `max_restarts`, `ready_log`, `oneshot`, `start_timeout`, `autostart`. Entradas de serviços que não estão no `atlas.yaml` são ignoradas. Os outros campos de serviço (`dir`, `env`, `open_key`, `open_url`, `hide_logs`, `lock_file`, `critical_help`, `health_timeout`, `failure_threshold`, `ready`, `on_unhealthy`) não podem ser sobrescritos e são rejeitados como chaves desconhecidas. |
| `tunnel` | campo a campo; ignorado se o `atlas.yaml` não tem `tunnel` |
| `infra` | substitui o bloco inteiro |
| `profiles` | mesclado por nome: o mesmo nome substitui a lista daquele profile, um nome novo é adicionado. Os membros são validados contra os `services` mesclados |
| `actions` | mesclado por `key`: a mesma key substitui a ação, uma key nova é acrescentada |
| `mobile.ios`, `mobile.android` | cada um substitui a seção base |

`env_files` não pode ser sobrescrito.

## Validação

O Atlas valida os dois arquivos antes de fazer qualquer coisa (`atlas dev`, `--headless`, `--ci` e a ferramenta de config do `atlas mcp` falham todos com as mesmas mensagens):

```text
$ atlas config check
3 config errors:
  atlas.yaml: services.web.helth: unknown field (did you mean 'health'?)
  atlas.yaml: services.web.depends_on.dbb: unknown service 'dbb' (did you mean 'db'?)
  atlas.yaml: services: dependency cycle: api -> web -> api
```

`atlas config check [--dir <path>]` imprime `OK: <path>/atlas.yaml is valid` e sai com `0`, ou imprime todos os erros e sai com `1`. Verificações:

- campo desconhecido em qualquer profundidade, com uma sugestão por distância de edição
- tipos errados ou campos obrigatórios ausentes (reportados como um único erro de parse)
- os alvos de `depends_on` existem, sem autodependência e sem ciclos
- `log_ready` exige `ready_log` no alvo; `completed_successfully` exige um `command` no alvo
- `ready_log` é uma regex válida
- `ready` define exatamente um entre `http` e `command`
- serviços habilitados têm `port`s únicas (serviços desabilitados não ocupam porta)
- todo membro de `profiles` é um serviço conhecido
- as `key`s das ações são únicas e não colidem com teclas embutidas nem com o `open_key` de um serviço; um `open_key` também não pode ser uma tecla embutida

Teclas embutidas (nunca usáveis como `open_key` nem como `key` de ação): `0-9 j k g G f v E M L P q r s S t m i ? : / n N W T J z -`.

## JSON Schema (autocompletar no editor)

```bash
atlas config schema > .atlas.schema.json
```

Adicione isto como a primeira linha do `atlas.yaml` (funciona com o YAML language server e a extensão YAML do VS Code):

```yaml
# yaml-language-server: $schema=.atlas.schema.json
```

Gere de novo depois de um `atlas update` para pegar os campos novos. Faça commit do arquivo de schema ou coloque-o no gitignore, como preferir.

## Exemplo completo

```yaml
# yaml-language-server: $schema=.atlas.schema.json
name: shop
pre_command: bun install
env_files: [.env]

infra:
  compose_file: packages/infra/dev/compose.yml

services:
  database:
    port: 54321
    hide_logs: true
    critical: true
    critical_help: "Start OrbStack, then retry"

  redis:
    port: 63790
    hide_logs: true

  migrate:
    command: bun run db:migrate
    oneshot: true
    depends_on: { database: healthy }

  server:
    command: bun run dev:server
    port: 3070
    health: http://localhost:3070/health/live
    ready: { http: /health/ready }
    on_unhealthy: restart
    depends_on: { migrate: completed_successfully, redis: healthy }
    restart: on_failure
    open_key: a

  web:
    command: bun run dev:web
    port: 3071
    depends_on: { server: healthy }
    open_key: w

  worker:
    command: bun run dev:worker
    restart: always
    max_restarts: 0

profiles:
  backend: [server, worker]
  front: [web]

actions:
  - key: c
    label: Run check
    command: bun run check
  - key: u
    label: DB refresh local
    command: bun run db:refresh-local
    pause_services: [server, worker]
    confirm: "Drops and recreates the local database. Continue?"
```
