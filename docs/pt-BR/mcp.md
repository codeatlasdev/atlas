---
title: "MCP"
source: mcp.md
source_sha: "ee8cb4e61e498f4bedca0d10f96bd83c83192d0d"
translation: machine
---

# Servidor MCP para agentes de IA

`atlas mcp` é um servidor [Model Context Protocol](https://modelcontextprotocol.io) via stdio. Ele deixa um agente (OMP, Claude Code, Codex, Cursor, …) ver o seu ambiente de desenvolvimento como você vê na TUI: estado dos serviços, logs, configuração. O agente também pode reiniciar, parar e iniciar serviços e executar as ações do seu `atlas.yaml`.

- Protocolo: JSON-RPC 2.0, delimitado por quebra de linha, revisão MCP `2025-11-25` (também aceita `2025-06-18`, `2025-03-26`, `2024-11-05`). Capacidades: tools, resources (`atlas://status`, `atlas://config`, template `atlas://logs/{service}`), prompts (`diagnose <service>`, `add-service <name> <command> [port]`, `status`; o OMP os mostra como `/atlas:<name>`).
- Nenhuma configuração além de registrar o comando: o `atlas mcp` descobre o projeto pelo diretório em que o agente o inicia.
- O servidor envia `instructions` de uso na resposta ao `initialize`; clientes como o OMP colocam esse texto no prompt de sistema do agente, então o agente já sabe que deve chamar `atlas_status` primeiro.

## Como funciona

```text
agent ──stdio──▶ atlas mcp ──files──▶ .logs/*.log, .logs/.tui-status.json, atlas.yaml     (always works)
                      └──unix socket──▶ .logs/atlas.sock ──▶ running `atlas dev`          (control tools)
```

| Recurso | Origem | Precisa do `atlas dev` rodando? |
|---|---|---|
| status | `.logs/.tui-status.json` (heartbeat atualizado cerca de uma vez por segundo) | não, informa `running: false` |
| logs, pesquisa nos logs | `.logs/<service>.log` (e `<service>.prev.log`) | não, mostra a última execução |
| configuração | `atlas.yaml` + `atlas.local.yaml` | não |
| reiniciar / parar / iniciar serviço, executar ação | socket de controle `.logs/atlas.sock` | **sim** |
| esperar ficar pronto | consulta o arquivo de status | precisa de um heartbeat ativo |

O socket de controle é servido pela TUI, pelo runner em segundo plano (`atlas dev --detach`) e pelo `atlas dev --headless` (não pelo `--ci`); veja [Runtime › Runner e attach](runtime.md#runner-e-attach). É um Unix domain socket com modo `0600` (só o seu usuário pode usá-lo), criado enquanto o lock de singleton do projeto está retido e removido na saída. As requisições passam pelos mesmos caminhos de código do teclado, então as proteções `confirm`, `pause_services` e "já está rodando" se comportam do mesmo jeito.

Se o `atlas dev` não está rodando, as ferramentas de controle retornam um resultado de erro: `atlas dev is not running; start it in a terminal`. Peça ao usuário que o inicie, ou rode `atlas dev --detach` você mesmo: o comando retorna na hora e deixa os serviços rodando para o usuário se conectar depois com `atlas dev` (`atlas down` os para).

## Permissões do agente e auditoria

Por padrão, um agente pode usar todas as ferramentas de controle, exatamente como antes. Um projeto cujas regras dizem "só a pessoa desenvolvedora inicia e reinicia a stack" declara isso no `atlas.yaml`:

```yaml
agents:
  control: deny      # the read-only switch: agents can read status, logs and config, nothing else
```

O próprio `atlas dev` aplica a regra (o servidor do socket de controle), então qualquer cliente MCP fica coberto, e não apenas um front end. Uma chamada recusada é um erro de ferramenta normal, com orientação, e nada acontece:

```text
this project's atlas.yaml denies agents restart_service on api: ask the developer to run it, or to allow it under `agents:` in atlas.yaml
```

Controle mais fino, por ferramenta e por alvo (veja [Configuração](configuration.md), seção `agents`):

```yaml
agents:
  control: deny                          # default for every control tool without its own entry
  restart_service: { allow: [worker] }   # these services only
  start_service: allow
  run_action: { deny: [Reset local DB] } # everything except these actions (key or label)
```

`atlas_status`, `atlas_logs`, `atlas_logs_search`, `atlas_config` e `atlas_wait_ready` só leem e nunca são restringidas. A política vale só para agentes: a TUI, o Atlas Desktop e a CLI são você, e nunca são recusados. Ela é lida do `atlas.yaml` quando o `atlas dev` inicia e o `atlas.local.yaml` não pode afrouxá-la, então uma mudança vale a partir do próximo `atlas dev`.

### Trilha de auditoria

Toda chamada de controle é registrada em `.logs/atlas-audit.log`, um objeto JSON por linha, não importa quem a fez:

```json
{"ts":"2026-11-13T10:00:05.123-03:00","caller":"agent","client":"omp/14.1","session":{"OMP_SESSION_ID":"s-42"},"tool":"restart_service","target":"api","args":{},"result":"denied","detail":"this project's atlas.yaml denies agents restart_service on api: ask the developer to run it, …"}
```

| Campo | Significado |
|---|---|
| `caller` | `agent` (veio pelo `atlas mcp`) ou `human` (teclado, paleta e menus da TUI, Atlas Desktop, a CLI) |
| `client` | `name/version` do cliente MCP, do handshake do `initialize`; ausente em `human` |
| `session` | as variáveis de sessão do host do agente encontradas no ambiente (`OMP_SESSION_ID`, `CLAUDE_SESSION_ID`, `CLAUDE_CODE_SESSION_ID`, `CODEX_SESSION_ID`, `ATLAS_AGENT_SESSION`); ausente quando nenhuma está definida |
| `tool` / `target` | o comando de controle (`restart_service`, `stop_service`, `start_service`, `run_action`) e o nome do serviço ou a chave/rótulo da ação como foi escrito; `args` leva os outros argumentos (`confirmed`), cortados em 200 caracteres cada |
| `result` | `allowed` (o atlas aceitou), `denied` (a política `agents:` recusou) ou `error` (passou pela política, mas o atlas recusou: serviço desconhecido, confirmação pendente, ação já rodando; `detail` diz o motivo) |

Leia o arquivo com `atlas_logs {"service": "audit"}` (as `lines` mais recentes, da mais antiga para a mais nova) ou `atlas_logs_search {"service": "audit", "query": "denied"}`; as duas só olham a execução atual, a menos que `include_previous` esteja definido na pesquisa. O arquivo não é um serviço: ele nunca aparece no tail de todos os serviços, na pesquisa sem `service` nem na lista de serviços com logs. Cada execução do `atlas dev` começa um arquivo novo (o anterior vira `atlas-audit.prev.log`), e um arquivo com mais de 5 MB é rotacionado do mesmo jeito. Requisições que nunca citam um alvo válido (falta o argumento `service`) são erros de protocolo e não são registradas.

> **Escopo.** A política limita o que um agente pode fazer *pelo atlas*. Ela não isola o agente: um agente com acesso a um shell ainda pode rodar `kill` ou `atlas down`. Para isso, use as regras de permissão do próprio host do agente.

## Ferramentas

Todas as ferramentas retornam texto. Falhas definem `isError: true` com uma mensagem sobre a qual o agente pode agir; nomes de ferramenta desconhecidos e requisições malformadas são erros JSON-RPC.

| Ferramenta | Argumentos | Precisa do `atlas dev` |
|---|---|---|
| [`atlas_status`](#atlas_status) | nenhum | não |
| [`atlas_metrics`](#atlas_metrics) | `service`, `last?` | não |
| [`atlas_logs`](#atlas_logs) | `service?`, `lines?`, `since_minutes?`, `level?` | não |
| [`atlas_logs_search`](#atlas_logs_search) | `query`, `service?`, `limit?`, `include_previous?` | não |
| [`atlas_config`](#atlas_config) | nenhum | não |
| [`atlas_restart_service`](#atlas_restart_service--atlas_stop_service--atlas_start_service) | `service` | sim |
| [`atlas_stop_service`](#atlas_restart_service--atlas_stop_service--atlas_start_service) | `service` | sim |
| [`atlas_start_service`](#atlas_restart_service--atlas_stop_service--atlas_start_service) | `service` | sim |
| [`atlas_run_action`](#atlas_run_action) | `action`, `confirmed?` | sim |
| [`atlas_hygiene`](#atlas_hygiene) | nenhum | não |
| [`atlas_wait_ready`](#atlas_wait_ready) | `service?`, `timeout_s?` | heartbeat ativo |
| [`atlas_heavy_status`](#atlas_heavy_status) | nenhum | não |
| [`atlas_last_result`](#atlas_last_result) | `name`, `project?` | não |
| [`atlas_affected`](#atlas_affected) | `files?`, `base?` | não |
| [`atlas_estimate`](#atlas_estimate) | `action` ou `command`, `label?`, `inputs?`, `outputs?`, `env?`, `tools?` | não |

### `atlas_status`

O `atlas dev` está rodando neste projeto, e qual é o estado de cada serviço? Chame esta ferramenta primeiro.

```json
{
  "running": true,
  "pid": 4242,
  "heartbeat_age_s": 1,
  "services": [
    { "name": "api", "state": "running", "port": 3001, "health": "http://localhost:3001/health",
      "exit_code": null, "restarts": 0, "uptime_s": 412, "started_at": 1760000000, "note": null, "off": false,
      "port_conflict": null, "ready": true, "probe_error": null, "unhealthy_since": null, "unhealthy_misses": null, "unhealthy_lines": [] },
    { "name": "worker", "state": "failed", "port": 3003, "health": null,
      "exit_code": 1, "restarts": 5, "uptime_s": null, "started_at": 1760000412, "note": "gave up after 5 restarts", "port_conflict": null },
    { "name": "web", "state": "failed",  "port": 3002, "health": null,
      "exit_code": null, "restarts": 0, "uptime_s": null, "started_at": 1760000413,
      "note": "port 3002 held by PID 12345 (node, cwd /work/app): stop it yourself (kill 12345) or change the service port",
      "port_conflict": { "pid": 12345, "cmd": "node", "cwd": "/work/app" } }
  ],
  "tunnel": { "state": "running", "local_port": 54320, "ssh_host": "prod-db" }
}
```

Por serviço: `exit_code` é o último código de saída do processo (`null` enquanto roda, se nunca saiu ou se morreu por sinal); `restarts` são as tentativas de reinício da sequência atual; `uptime_s` são os segundos em `running` (`null` nos outros estados); `started_at` é o unix time, em segundos, do último spawn; `note` é a anotação da barra lateral (`waiting for :3001 (12s)`, `waiting for ready_log (12s)`, `start timeout: still waiting for :3001 (75s)`, `restarting in 4s (2/5)`, `dependency db failed`, …).

`port_conflict` é `{pid, cmd, cwd}` do processo **de fora** que ocupava a porta do serviço no último início (um dev server iniciado fora do atlas), e `null` nos outros casos; `cwd` é `null` onde o sistema operacional não informa (Windows). É um campo próprio, nunca embutido em `note`, e é limpo quando o processo que ocupava a porta sai. Não cabe ao agente encerrar esse processo: avise o usuário, que pode pará-lo ou trocar a porta do serviço (de propósito, não existe ferramenta para encerrá-lo).

Campos de saúde (veja [Saúde e prontidão](configuration.md)), presentes em todas as linhas:

| Campo | Significado |
|---|---|
| `ready` | resultado da sonda `ready:` opcional do serviço: `true` enquanto `running`, `false` enquanto `degraded` por uma sonda que falha, `null` quando não há sonda (ou o serviço não está de pé) |
| `probe_error` | por que um serviço `degraded` está degradado: a mensagem da sonda que falha (`GET /ready → 503`, `connection refused`, `timed out after 5s`, `` `cmd` exited 1: … ``) ou a linha de log que se repete (`repeating error: …`); `null` nos outros casos |
| `unhealthy_since` | unix time, em segundos, em que um serviço `unhealthy` (travado) parou de responder; `null` nos outros casos |
| `unhealthy_misses` | sondas de liveness perdidas em sequência naquele momento; `null` nos outros casos |
| `unhealthy_lines` | as últimas linhas de saída do serviço (até 20) naquele momento, para você ver o que ele fazia quando travou; `[]` nos outros casos |
| `memory_rss` | bytes de toda a árvore de processos do serviço, arredondados para MB inteiros; `null` enquanto ele não está rodando |
| `memory_trend` / `memory_trend_mb_min` | `rising`, `flat` ou `falling` e a velocidade em MB por minuto, com sinal; `null` até existirem cerca de 30 s de amostras |
| `budget_state` | `ok`, `warn` (acima de `memory_warn`: o serviço fica `degraded` e `probe_error` diz `using 2.1 GB, over memory_warn 1.5 GB`) ou `over` (acima de `max_memory`); veja [Memória](configuration.md) |
| `memory_leak` | `leak suspected: +600 MB over 12 reloads` enquanto o RSS só cresce de um reload ou reinício para o seguinte; `null` nos outros casos. A série completa está em [`atlas_metrics`](#atlas_metrics) |

Um serviço travado fica assim (`note` é `unhealthy for 42s: 3 missed probes`):

```json
{ "name": "web", "state": "unhealthy", "port": 3002, "health": "http://localhost:3002/",
  "exit_code": null, "restarts": 0, "uptime_s": 900, "note": "unhealthy for 42s: 3 missed probes",
  "ready": null, "probe_error": null, "unhealthy_since": 1760000870, "unhealthy_misses": 3,
  "unhealthy_lines": ["12:01:04 [stderr] Error: ETIMEDOUT db.internal:5432", "…"] }
```

`tunnel` é `null` quando nenhum [túnel SSH](configuration.md) está rodando; caso contrário, o `state` dele usa os mesmos valores dos serviços (`failed` durante a reconexão). A saída do ssh do túnel pode ser lida com `atlas_logs` (`service: "tunnel"`).

`state` é um entre `starting`, `running`, `degraded`, `unhealthy`, `failed`, `stopped`, `disabled` (parado por um usuário ou agente, ou deixado de fora da execução) e `suspended` (pausado por uma ação). `unhealthy` = o processo está vivo, mas parou de responder à sonda de saúde (travou; nada saiu, então não é `failed`). `degraded` = ele responde, mas a sonda `ready:` falha, o mesmo erro se repete no log ou ele passou do orçamento `memory_warn`; `probe_error` diz qual deles. `off: true` marca um serviço deixado de fora desta execução por `atlas dev SERVICE…` / `--profile` (estado `disabled`): inicie-o com `atlas_start_service`, que limpa a marca. Quando nada está rodando: `{"running": false, "reason": "no status file", "hint": "atlas dev is not running; start it in a terminal"}`. Outros motivos: o pid está morto, ou o heartbeat tem mais de 120 s.

### `atlas_metrics`

Memória de um serviço ao longo do tempo ([#131](https://github.com/codeatlasdev/atlas/issues/131)): a série que o `atlas dev` guarda, para o agente medir antes e depois de uma mudança (um refactor, uma atualização de dependência, um vazamento suspeito) sem amostragem avulsa com `ps`. Funciona a partir de `.logs/.tui-metrics.json`, então não precisa de socket de controle ativo, mas só um `atlas dev` rodando grava esse arquivo.

| Argumento | Tipo | Padrão | Notas |
|---|---|---|---|
| `service` | string | obrigatório | Nome exatamente como no `atlas.yaml` (veja `atlas_config`); um nome desconhecido lista os serviços conhecidos. |
| `last` | inteiro ≥ 1 | a série inteira | Só as N amostras mais recentes. |

```json
{
  "service": "web", "running": true, "state": "degraded", "updated_at": 1760000900, "interval_s": 5,
  "samples": [[1760000890, 1363148800], [1760000895, 1420296192], [1760000900, 1426063360]],
  "rss_bytes": 1426063360, "min_bytes": 1363148800, "max_bytes": 1426063360,
  "trend": "rising", "trend_mb_per_min": 360.0, "budget_state": "warn", "cycles": 11,
  "leak": "leak suspected: +660 MB over 11 reloads"
}
```

`samples` são pares `[unix_seconds, bytes]`, da mais antiga para a mais nova, uma a cada `interval_s` (5 s), até a última hora (720). Os bytes são de toda a árvore de processos (no macOS, o footprint físico). `trend` é `rising`, `flat` ou `falling` (`null` até cerca de 30 s de dados), `trend_mb_per_min` é a velocidade com sinal, `budget_state` é `ok` / `warn` / `over` em relação a [`memory_warn` / `max_memory`](configuration.md) do serviço, `cycles` são os reloads e reinícios que o monitor de vazamento contou, e `leak` é a suspeita (`null` quando não há). `running` é `false` quando nenhum `atlas dev` ativo é dono do projeto: a série é então o que a última execução deixou, e `state` é `null`. Se nada foi amostrado ainda, a ferramenta retorna o erro `no metrics yet: …`.

### `atlas_logs`

Mostra o fim dos logs de `.logs/`. O ANSI já foi removido. Cada linha retornada tem o formato `[service:line_no] HH:MM:SS [stdout|stderr] text`.

| Argumento | Tipo | Padrão | Notas |
|---|---|---|---|
| `service` | string | todos, combinados por horário | Precisa ter um arquivo de log; caso contrário, o erro lista os serviços que têm. Saída de [ação](runtime.md#ações) do projeto: `action:<label>` (por exemplo `action:Run checks`) ou o nome do arquivo `action-run-checks`. `audit`: a [trilha de auditoria](#permissões-do-agente-e-auditoria) das chamadas de controle (`HH:MM:SS agent omp/14.1 restart_service api -> denied: …`); ela nunca é combinada no tail de todos os serviços, e `level` / `since_minutes` não se aplicam a ela. |
| `lines` | inteiro 1-2000 | 100 | Quantidade de linhas finais a retornar. |
| `since_minutes` | inteiro ≥ 0 | nenhum | Só as linhas dos últimos N minutos (pelo prefixo `HH:MM:SS`, horário local, limitado a 24 h). Linhas sem timestamp (continuações de stack trace) são mantidas. |
| `level` | `error` \| `warn` | nenhum | `error` = só erros; `warn` = avisos e erros. Detectado pelo texto (`error`, `fatal`, `panic`, `warn`, códigos de cor). |

Linhas individuais com mais de 2.000 caracteres são cortadas com `… [clipped]`. Resultado vazio: `(no matching log lines)`. Um arquivo de log com mais de 8 MiB é lido só a partir do fim (os números de linha em `[service:line_no]` continuam sendo os números reais de linha do arquivo); o próprio log em disco é rotacionado para `<service>.prev.log` aos 50 MiB. As linhas em disco são as linhas montadas (linhas parciais unidas, atualizações de barra de progresso com `\r` colapsadas, linhas acima de 16 KiB cortadas com ` … [truncated]`).

```text
[api:212] 10:00:05 [stderr] ERROR db down
[web:88] 10:00:07 [stdout] warn: slow
```

### `atlas_logs_search`

Pesquisa nos logs. `query` é uma substring literal, ou uma regex quando escrita entre barras (`/ECONNREFUSED|timeout/`). A comparação não diferencia maiúsculas de minúsculas. Os arquivos são lidos em fluxo, linha a linha (CRLF e UTF-8 incorreto são tratados) e só as `limit` ocorrências mais recentes são mantidas, então pesquisar um log grande não o carrega na memória.

| Argumento | Tipo | Padrão | Notas |
|---|---|---|---|
| `query` | string | **obrigatório** | Não vazio. Uma regex incorreta é um erro. |
| `service` | string | todos | Restringe a um serviço. Retorna erro com a lista dos serviços conhecidos se ele não tem log atual nem anterior. |
| `limit` | inteiro 1-500 | 50 | São retornadas as `limit` ocorrências **mais recentes**. |
| `include_previous` | booleano | `false` | Pesquisa também os logs da execução anterior (`<service>.prev.log`); essas linhas ganham a marca `(prev run)`. |

Resultado vazio: `(no matches)`.

### `atlas_config`

Resumo do `atlas.yaml` combinado com o `atlas.local.yaml`, para o agente aprender os nomes de serviço e as chaves/rótulos de ação válidos. Sem `atlas.yaml`, resume a stack [detectada](configuration.md) e diz isso em `source` (`"file"` ou `"detected"`). Falha com a mensagem de validação se existe um `atlas.yaml` incorreto.

```json
{
  "project": "shop",
  "source": "file",
  "services": [
    { "name": "api", "port": 3001,
      "depends_on": [{ "service": "database", "condition": "healthy" }],
      "command": "bun run dev:api", "enabled": true }
  ],
  "actions": [
    { "key": "R", "label": "Reset local DB", "confirm": "This wipes the local database. Continue?",
      "pause_services": ["api"], "session": false }
  ]
}
```

### `atlas_restart_service` / `atlas_stop_service` / `atlas_start_service`

Argumento: `service` (nome exatamente como no `atlas.yaml`; nomes desconhecidos retornam um erro que lista os conhecidos).

- **restart**: para e depois inicia. Também serve para iniciar um serviço parado ou com falha.
- **stop**: para o serviço e o marca como `disabled`: as verificações de saúde o ignoram e ele nunca é reiniciado automaticamente.
- **start**: traz de volta um serviço parado, desabilitado ou com falha (mesmo mecanismo do restart).

As três retornam assim que a requisição é aceita, não quando o serviço está de pé:

```json
{ "service": "api", "queued": true }
```

Continue com [`atlas_wait_ready`](#atlas_wait_ready). Quando a [política `agents:`](#permissões-do-agente-e-auditoria) do projeto nega a chamada, o resultado é um erro que diz ao agente para pedir à pessoa desenvolvedora.

### `atlas_run_action`

Executa uma ação do `atlas.yaml` por `action` (o caractere da tecla ou o rótulo, sem diferenciar maiúsculas de minúsculas; a tecla tem prioridade) no `atlas dev` em execução.

| Argumento | Tipo | Padrão | Notas |
|---|---|---|---|
| `action` | string | **obrigatório** | Tecla ou rótulo, veja `atlas_config`. |
| `confirmed` | booleano | `false` | Defina como `true` **só depois que o usuário aprovar** o texto de confirmação da ação. |

**Confirmação obrigatória.** Uma ação com mensagem `confirm:` é destrutiva por declaração. Sem `confirmed: true`, a chamada é recusada e o texto é retornado:

```text
action 'Reset local DB' requires confirmation: "This wipes the local database. Continue?". Ask the user, then retry with confirmed: true.
```

O agente deve mostrar esse texto ao usuário e tentar novamente só se ele concordar. Outros erros: ação desconhecida (lista as conhecidas), a ação já está rodando ou (no runner / headless) a ação tem `platform` (precisa do seletor de dispositivo da TUI) ou é uma ação `session: true` (precisa do terminal que é dono dela), ambos só na TUI dona. Em caso de sucesso: `{ "action": "Reset local DB", "queued": true }`. O comando roda em segundo plano (serviços pausados são reiniciados quando ele termina); acompanhe o progresso com `atlas_status` e `atlas_logs`. Uma ação `session: true` abre a aba dela na TUI dona.

### `atlas_hygiene`

Relatório só de leitura sobre o que o Atlas e os agentes deixaram para trás: processos órfãos marcados (`orphans[]`: `pid`, `name`, `session`, `kind`, `label`, `project`), disco por projeto (`projects[]`: `target`, `node_modules`, `logs`, `worktrees`, `indexed`), diretórios temporários e de cache registrados em relação aos limites (`dirs[]`) e o que uma correção liberaria (`freeable`). É o mesmo valor de `atlas doctor --hygiene --json`. Não muda nada: para liberar espaço, peça ao usuário que rode `atlas doctor --hygiene --fix` (ou use Configurações > Workspaces > Hygiene no Atlas Desktop). Veja [sessões e higiene](runtime.md#sessões-e-higiene).

### `atlas_wait_ready`

Bloqueia até um serviço (ou todos os serviços) estar `running`, consultando o status ao vivo duas vezes por segundo. `running` já inclui a sonda `ready:` do serviço quando ele tem uma, então um serviço `degraded` ou `unhealthy` continua sendo esperado.

| Argumento | Tipo | Padrão | Notas |
|---|---|---|---|
| `service` | string | todos | Quando omitido, serviços `disabled` e `suspended` não são esperados. Um serviço específico que está desabilitado continua esperando. |
| `timeout_s` | inteiro 1-**600** | 60 | Valores acima de 600 são limitados a 600. |

Resultados:

- pronto: `ready after 3.2s` seguido de `api=running web=running` (nunca informado em menos de 1,5 s, então uma chamada logo depois de um reinício não vê o `running` antigo);
- um serviço entrou em `failed`: erro `service 'api' failed` mais todos os estados (falha rápido);
- timeout: erro `timed out after 60s waiting for readiness` mais todos os estados, depois um bloco por serviço bloqueante que tenha um motivo: para `degraded`, a mensagem da sonda que falha (`api: ready probe failed: GET /ready → 503`, ou a linha repetida); para `unhealthy`, há quanto tempo ele está travado, quantas sondas perdeu e as últimas linhas de saída:

  ```text
  timed out after 60s waiting for readiness
  api=degraded web=unhealthy
  api: ready probe failed: GET /ready → 503
  web: unhealthy for 42s (3 missed probes); last output:
      12:01:04 [stderr] Error: ETIMEDOUT db.internal:5432
  ```

  Com `service` definido, só esse serviço é listado.
- atlas não está rodando: erro `atlas dev is not running; start it in a terminal`.

> **Timeouts do cliente.** Clientes MCP cancelam uma chamada que demora demais. O padrão do OMP é 30 s por requisição, então um `timeout_s` acima de cerca de 25 é cortado pelo cliente. Mantenha `timeout_s` ≤ 25 e chame de novo, ou aumente o timeout do cliente (OMP: `"timeout": 120000` na entrada do servidor, até cobrir o máximo de 600 s, se você precisar). O próprio Atlas responde a toda requisição de controle em até 30 s.

### `atlas_heavy_status`

Só leitura e vale para a máquina inteira (não precisa de projeto nem de `atlas dev`): a fila de jobs pesados compartilhada por todos os projetos, worktrees e agentes desta máquina ([`atlas run --heavy`](cli.md)).

```json
{ "slots": 2, "load": 3.1, "mem_available_pct": 42,
  "running": [ { "label": "gate", "command": "bun run check", "cwd": "/work/atlas-wt-a", "project": "atlas-wt-a",
                 "pid": 4242, "state": "running", "queued_at": 1763040000000, "started_at": 1763040004000,
                 "slot": 0, "age_s": 61, "avg_s": 240, "eta_s": 179 } ],
  "queued":  [ { "label": "gate", "...": "...", "state": "queued", "age_s": 20, "avg_s": 240, "eta_s": 179 } ] }
```

`load` é a média de carga de 5 minutos e `mem_available_pct` é o percentual de memória livre (cada um `null` onde o sistema operacional não informa). `eta_s` são os segundos até um job em execução terminar, ou até um job na fila começar, calculados pela média móvel das últimas 8 execuções bem-sucedidas do mesmo `label`; `null` quando o label não tem histórico. **Agentes devem rodar comandos pesados (cargo build/test, `bun run check`, suítes e2e, benchmarks) como `atlas run --heavy -- <cmd…>` em vez de iniciá-los diretamente**, para que uma dúzia de agentes nunca sobrecarregue a máquina; use esta ferramenta para ver quem ocupa os slots e quanto tempo dura a espera.

### `atlas_last_result`

Só leitura e vale para a máquina inteira (sem projeto, sem `atlas dev`): o último resultado de uma tarefa, ação ou comando que passou pelo [cache de tarefas](cli.md), não importa quem o rodou. `name` é o rótulo da ação, o `--label` ou a linha de comando exata; `project` restringe a um projeto (o nome da pasta da raiz dele). Um nome desconhecido é um erro.

```json
{ "name": "test", "command": "cargo test", "project": "atlas", "key": "9f2c…64 hex…",
  "hit": true, "exit": 0, "duration_ms": 184000, "at": 1763040000000,
  "log_tail": "test result: ok. 412 passed…", "saved_ms": 184000 }
```

`hit: true` significa que o resultado veio do cache e nada rodou (`duration_ms` é quanto o comando levou quando rodou de verdade, `saved_ms` é o tempo que o hit economizou); `hit: false` significa que ele rodou. `log_tail` são os últimos cerca de 2 KiB das duas saídas. Falhas também são informadas (`exit` ≠ 0), embora só o exit 0 seja salvo no cache. Consulte esta ferramenta antes de rodar de novo um build ou uma suíte, e rode o comando como `atlas run --cache -- <cmd>` para que a execução idêntica de outro agente responda em milissegundos.

### `atlas_affected`

Só leitura e não inicia nada: o que uma mudança afeta, para o agente rodar as poucas coisas que a mudança pode quebrar em vez da suíte inteira. `files` são os arquivos alterados (relativos à raiz do projeto; caminhos absolutos dentro dela também funcionam); se omitidos, a ferramenta usa a mudança desta worktree: o que foi commitado desde a base do merge com `base` (padrão `origin/main`, senão `main`, senão `master`), mais os arquivos staged, unstaged e não rastreados (arquivos que o git ignora não contam como mudança). Fora de um repositório git, `files` é obrigatório.

```json
{ "source": "rules", "base": null, "files_from": "git",
  "files": ["crates/atlas-core/src/lib.rs"],
  "tasks": ["Run checks"], "tests": ["cargo test -p atlas-core"],
  "routes": ["/docs"], "builds": ["site"] }
```

`source` diz qual grafo respondeu, nesta ordem:

1. `command`: o plugin que o projeto declara como [`affected: { command }`](configuration.md) no `atlas.yaml`. Ele roda na raiz do projeto, recebe os arquivos alterados no stdin (um por linha) e imprime JSON `{tasks, tests, routes, builds}` (toda chave é opcional). Um projeto que já tem um grafo de testes ou um script de impacto em rotas o conecta aqui (`bun scripts/affected.ts --json`). Um plugin que falha é um erro com o stderr dele, nunca um retorno silencioso a outro grafo.
2. `rules`: o grafo declarado inline como [`affected: { rules }`](configuration.md): globs para tarefas, testes, rotas e builds.
3. `cache_inputs` (alternativa, sem bloco `affected:`): as ações do `atlas.yaml` com um bloco `cache:` cujos globs de `inputs` casam com um arquivo alterado, em `tasks` pelo rótulo. `tests`, `routes` e `builds` ficam vazios porque nada os declara.

Listas vazias significam que nada do que foi declarado é afetado. Este repositório usa a forma de plugin: `affected: { command: bun scripts/check-changed.ts --json --stdin }` retorna os comandos de verificação em `tests` e os crates alterados mais os dependentes deles em `builds`.

### `atlas_estimate`

Só leitura e não executa nada: quanto custaria rodar uma tarefa agora. Passe `action` (uma ação do `atlas.yaml` por rótulo, sem diferenciar maiúsculas de minúsculas, ou por tecla) ou `command` com `label`, `inputs`, `outputs`, `env` e `tools` opcionais, iguais às flags de [`atlas run --cache`](cli.md), para que a chave seja a mesma.

```json
{ "name": "Run checks", "command": "cargo test", "project": "atlas",
  "runs": 5, "median_ms": 184000, "p90_ms": 231000, "sources": ["task cache"],
  "would_hit": true, "saved_ms": 184000, "key": "9f2c…64 hex…", "key_error": null,
  "run_with": "atlas run --cache --label 'Run checks' --input 'crates/**' -- 'cargo test'", "note": null,
  "queue": { "slots": 3, "running": 1, "queued": 0, "load": 2.4, "mem_available_pct": 61, "wait_eta_s": 0, "label": "Run checks" } }
```

- **Duração.** `median_ms` e `p90_ms` (nearest rank) das execuções que realmente aconteceram: os misses registrados no cache de tarefas para aquele label ou linha de comando (as últimas 64 execuções que o cache viu); na falta deles, o histórico da [fila heavy](cli.md) para o mesmo label (últimas 8 execuções, em segundos inteiros; não é combinado com o do cache, porque uma ação `--heavy` com cache é gravada nos dois para a mesma execução). Hits no cache não são amostras (eles repetem a duração original). Sem histórico, `runs` é `0` e as durações são `null`: nenhum número é inventado.
- **`would_hit`.** A chave de cache é calculada a partir dos bytes que estão em disco agora (edições não commitadas incluídas) e consultada sem marcar a entrada como usada: `true` significa que existe uma execução idêntica salva, então `run_with` a devolve em milissegundos de qualquer worktree e `saved_ms` é o que isso economiza. Editar um input muda o valor para `false`. `key_error` diz por que não foi possível calcular uma chave (um glob incorreto, uma ferramenta sem `--version`).
- **`queue`.** A fila heavy da máquina inteira: slots, jobs em execução e em espera, carga de 5 minutos, percentual de memória livre e `wait_eta_s`, os segundos até um slot liberar para um novo job (`0` com um slot livre, `null` quando o histórico não permite saber). Com muitos jobs à frente, rode antes as verificações baratas.
- Uma ação sem bloco `cache:` tem durações, mas não tem chave e `run_with` é `null`; `note` diz como torná-la cacheável.

Uma ação desconhecida, `action` e `command` juntos, ou um glob incorreto em `inputs` gera um resultado `isError` que nomeia o problema.

### Fluxo típico de um agente

1. `atlas_status`: está de pé, o que está falhando?
2. `atlas_logs {"level": "error"}` ou `atlas_logs {"service": "api", "lines": 200}`; `atlas_logs_search {"query": "/ECONN|timeout/"}`.
3. Corrija o código.
4. `atlas_restart_service {"service": "api"}` e depois `atlas_wait_ready {"service": "api"}`. Um serviço `degraded` ou `unhealthy` aparece no passo 1 com o motivo (`probe_error`, `unhealthy_lines`): leia esses campos antes dos logs.
5. Verifique os logs de novo.

Para decidir o que rodar depois de uma edição, sem iniciar nada:

1. `atlas_affected {}`: quais tarefas, testes, rotas e builds a mudança toca (acrescente `files` para perguntar sobre uma edição que você ainda não fez).
2. `atlas_estimate {"action": "<task>"}` para cada uma: `would_hit` (não custa nada), `median_ms`/`p90_ms` e a espera em `queue`. Rode as baratas primeiro.
3. Rode cada uma como a linha `run_with` que a ferramenta retorna (`atlas run --cache [--heavy] …`). `atlas_last_result` mostra o que foi impresso.

## Configurar o cliente

O `atlas` precisa estar no `PATH` (veja [Instalação](installation.md)). Sem `--dir`, o `atlas mcp` serve o ancestral mais próximo do diretório de trabalho que contém `atlas.yaml`; os clientes iniciam os servidores a partir do diretório do projeto, então um `atlas mcp` simples costuma bastar. Passe `--dir <path>` (ou defina o `cwd` do cliente) para fixar um projeto.

### OMP (principal)

`atlas setup omp --apply` faz isso por você: grava o plugin `~/.atlas/omp-plugin/` (o `.mcp.json` dele registra o servidor) e roda `omp plugin link`. Alternativa manual, no nível do projeto, versionada junto com o repositório, em `.omp/mcp.json`:

```json
{
  "mcpServers": {
    "atlas": {
      "type": "stdio",
      "command": "atlas",
      "args": ["mcp"],
      "timeout": 120000
    }
  }
}
```

No nível do usuário (todos os projetos), em `~/.omp/agent/mcp.json`: mesmo formato. Acrescente `"cwd": "/path/to/project"` ali se quiser um projeto fixo; do contrário, o OMP usa o diretório de trabalho dele e o Atlas encontra o projeto a partir dele.

Notas sobre o OMP:

- `timeout` é em milissegundos. O timeout de requisição padrão do OMP é 30 s (também `OMP_MCP_TIMEOUT_MS`); `120000` deixa folga para chamadas de `atlas_wait_ready` com um `timeout_s` maior (veja [Timeouts do cliente](#atlas_wait_ready)).
- O OMP coloca as `instructions` do servidor no prompt de sistema. Para desativar isso neste servidor, defina `"instructions": false` na entrada.
- O OMP pode enviar ids JSON-RPC em string; o Atlas devolve os ids sem alterá-los, então isso funciona.
- A descoberta espera um pouco e continua em segundo plano; as ferramentas podem aparecer um instante depois da inicialização.
- Comandos: `/mcp list` (servidores e ferramentas), `/mcp test atlas` (conecta e lista), `/mcp reload` (relê a configuração), `/mcp reconnect` (reinicia o processo do servidor, por exemplo depois de `atlas update`).
- O OMP também importa configurações MCP do Claude Code, do Codex e do Cursor, então qualquer uma das configurações abaixo também é reconhecida pelo OMP.

`atlas setup claude-code --apply` / `atlas setup codex --apply` gravam essas entradas por você ([Configuração de agentes](agents.md)); as formas manuais:

### Claude Code

```bash
claude mcp add --scope user atlas -- atlas mcp
```

ou `--scope project` / um `.mcp.json` na raiz do projeto (o escopo de usuário fica em `~/.claude.json`):

```json
{ "mcpServers": { "atlas": { "command": "atlas", "args": ["mcp"] } } }
```

### Codex

`~/.codex/config.toml`:

```toml
[mcp_servers.atlas]
command = "atlas"
args = ["mcp"]
```

### Cursor

`.cursor/mcp.json` no projeto:

```json
{ "mcpServers": { "atlas": { "command": "atlas", "args": ["mcp"] } } }
```

### Qualquer outro cliente

Rode `atlas mcp [--dir <project>]` como servidor stdio. Verificação manual rápida:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"atlas_status"}}' \
  | atlas mcp
```

## Notas sobre o protocolo

- `initialize` retorna a versão de protocolo pedida se o Atlas a aceita; do contrário, retorna `2025-11-25`; `serverInfo` é `{ "name": "atlas", "version": <atlas version> }`; capacidades: `tools` com `listChanged: false`.
- `ping`, `tools/list` e `tools/call` são atendidos mesmo que o cliente tenha pulado o handshake. Notificações nunca recebem resposta.
- As requisições rodam em paralelo, então um `atlas_wait_ready` longo nunca bloqueia `ping` nem outras chamadas.
- Lotes JSON-RPC (batches) só são aceitos quando a versão negociada é `2025-03-26` (a única revisão que os tem); nas outras, eles recebem o erro `-32600`. Os ids de requisição devem ser strings ou números.
- O stdout é só protocolo. Os diagnósticos vão para o stderr; o `atlas mcp` imprime ali `atlas mcp: serving <dir> on stdio`.
- O servidor sai quando o cliente fecha o stdin, depois de responder às chamadas em andamento.

## Solução de problemas

| Sintoma | Causa / correção |
|---|---|
| Ferramentas ausentes no cliente | O `atlas` não está no `PATH` do cliente (apps com interface gráfica muitas vezes não têm o `PATH` do seu shell: use o caminho absoluto, por exemplo `~/.atlas/bin/atlas`). No OMP, rode `/mcp test atlas`. |
| `atlas_config` falha | O `atlas.yaml` não foi encontrado a partir do diretório de trabalho (defina `--dir` ou `cwd`) ou está incorreto (`atlas config check`). |
| `atlas_status` diz `running: false`, mas a TUI está aberta | O agente está olhando um diretório de projeto diferente do da TUI. Compare o `--dir`/`cwd`. |
| `atlas dev is not running; start it in a terminal` | Inicie o `atlas dev` (ou `atlas dev --detach` / `--headless`) para esse projeto. O `--ci` não serve o controle. |
| Chamada de controle: `timed out waiting for atlas dev` | O loop de execução não respondeu em 30 s (está travado ou encerrando). |
| `atlas_wait_ready` expira com `degraded` / `unhealthy` | O serviço está de pé, mas inutilizável (`degraded`: a sonda `ready:` falha ou um erro se repete) ou travado (`unhealthy`). A mensagem de timeout nomeia o motivo; veja [Saúde e prontidão](configuration.md). |
| `atlas_wait_ready` cortado após 30 s | O timeout do cliente é menor que `timeout_s`; veja [Timeouts do cliente](#atlas_wait_ready). |
| `unknown service 'x'. Known services: …` | Use os nomes de `atlas_config`. |
| `no log file for service 'x'` | O serviço ainda não produziu log. Serviços sem comando nunca têm um. |
| `.logs/atlas.sock` antigo após uma queda | Inofensivo: o próximo `atlas dev` o substitui. |
