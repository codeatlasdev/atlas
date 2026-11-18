---
title: "Runtime"
source: runtime.md
source_sha: "82b1852a4f384b0e363095314f82970ddae317ff"
translation: machine
---

# Comportamento em runtime

Como o `atlas dev` inicia, supervisiona e encerra o seu ambiente, e o que ele deixa em `.logs/`. O mesmo manager controla a TUI, o runner em segundo plano (`--detach`), o `--headless` e o `--ci`.

## Ordem de inicialização

1. A **configuração** é carregada e validada (`atlas.yaml` + `atlas.local.yaml`). Erros abortam antes de qualquer coisa iniciar. Uma seleção `SERVICE…` / `--profile` também é resolvida aqui (nomes desconhecidos abortam, veja [Selecionar serviços](cli.md#selecionar-serviços)).
2. O **lock singleton** `.logs/.tui.lock` é obtido, veja [abaixo](#singleton-e-o-socket-de-controle). Depois o socket de controle `.logs/atlas.sock` recebe o bind.
3. **`pre_command` / instalação de dependências** (mesma lógica de desatualização da [configuração](configuration.md#pre_command)). **TUI**: o dashboard é desenhado na hora (splash primeiro) e a instalação roda em segundo plano, mas os serviços de app ficam retidos até ela terminar (com sucesso ou falha; uma falha mostra um toast de erro e os serviços iniciam mesmo assim). **`--headless` / `--ci`**: a instalação roda aqui, antes de qualquer coisa iniciar; uma falha aborta o `--ci` (saída `1`, `reason` no JSON) e no `--headless` só vai para o log.
4. **Limpeza de órfãos**: o que sobrou de uma execução anterior que caiu é finalizado, veja [Limpeza de órfãos](#limpeza-de-órfãos).
5. **Infra** (quando há um bloco `infra:` ou serviços sem `command` com uma porta), em segundo plano para a UI continuar responsiva:
   1. `docker compose -f <compose_file> up -d` depois de esperar até 15 s pelo daemon do docker (uma nova tentativa após 2 s);
   2. cada serviço sem `command` com `port` é consultado (a cada 500 ms) até a porta aceitar conexões, por até 30 s, todos em paralelo. Porta alcançável significa `running`; caso contrário, `failed`.
6. Os **serviços de app** (os que têm `command`) entram na fila quando a infra termina, com sucesso ou não (uma falha do compose gera um alerta crítico `docker` e marca os serviços de infra como `failed`, mas os apps iniciam mesmo assim).
7. O **scheduler** inicia cada serviço da fila assim que todas as [condições de `depends_on`](configuration.md#depends_on) valem; serviços sem dependências iniciam imediatamente e em paralelo. Uma dependência que nunca pode ser satisfeita faz os seus dependentes falharem.
8. O Atlas grava `.logs/.tui-status.json` e depois o atualiza a cada segundo.

Em paralelo com os passos 4 a 7, quando há um bloco `tunnel:` habilitado, o [túnel SSH](#túnel-ssh) inicia na própria tarefa em segundo plano; os serviços nunca esperam por ele.

Sem infra para esperar, os passos 5 e 6 se reduzem a nada e os serviços de app iniciam na hora.

## Como um serviço é iniciado

- `sh -c "<command>"` na raiz do projeto (ou no `dir` do serviço); o stdin é `/dev/null` e o stdout e o stderr passam por pipe, linha a linha.
- O processo filho recebe o seu próprio **grupo de processos**. Parar (stop, restart, quit) também encerra a sua **árvore de tarefas**: o Atlas tira um snapshot da tabela de processos uma vez (`ps -ax -o pid=,ppid=,pgid=`), coleta todos os descendentes do filho e envia SIGTERM ao grupo do filho e ao pid e ao grupo de cada descendente, depois SIGKILL ao que sobrar após o período de tolerância. É isso que encerra ferramentas como `bun run dev` → `turbo`, que iniciam as suas tarefas em grupos de processos *novos* (matar só o grupo deixaria essas tarefas rodando e segurando portas). O Atlas nunca envia sinal ao PID 1, a si mesmo nem aos seus ancestrais, e nunca seleciona processos por porta. <a id="service-processes"></a>
- Ambiente: herdado < `env_files` < `env:` do serviço < `PORT` (quando `port` está definido), `FORCE_COLOR=1`, `BROWSER=none`, com `NO_COLOR` removido. O ambiente herdado passa **sem filtro** (sem allowlist), então uma variável exportada no seu shell chega a todo serviço; como os `env_files` têm precedência sobre ele, uma chave presente nos dois vem do arquivo.
- A saída vai para o buffer da TUI e para `.logs/<service>.log`, e é conferida contra `ready_log` quando definido.

## Estados e saúde dos serviços

| Estado | Significado |
|---|---|
| `starting` | iniciado (ou porta de infra ainda fora do ar), ainda não pronto |
| `running` | pronto (veja abaixo) |
| `degraded` | responde ao probe de liveness mas não está utilizável: o seu [probe `ready`](configuration.md#saúde-e-prontidão) falha, ou o mesmo erro se repete no log. Veja [Unhealthy e degraded](#unhealthy-e-degraded) |
| `unhealthy` | processo vivo mas sem responder mais ao probe de liveness (travado), depois de `failure_threshold` falhas seguidas. Não é `failed`: nada encerrou |
| `failed` | encerrou de forma anormal, um probe falhou antes de o serviço ficar saudável, uma dependência falhou ou o limite de reinícios acabou |
| `stopped` | parado; também um one-shot concluído (`done`) |
| `disabled` | parado por você (`s`, stop pelo MCP) ou deixado fora desta execução por `SERVICE…` / `--profile` (mostrado como `off`); ignorado pelos health checks e nunca reiniciado automaticamente |
| `suspended` | pausado pelo `pause_services` de uma ação |

Prontidão e saúde, por serviço:

- Com `ready_log`: o serviço fica `starting` (um filho vivo **não** basta) até a primeira linha de saída que bata com o padrão torná-lo `running`; depois disso, um filho vivo conta como saudável e nenhum probe de porta é feito.
- Sem `ready_log`: um **`GET` HTTP real** na URL de `health` (`health_timeout`, padrão 2 s; redirecionamentos não são seguidos; 2xx/3xx = saudável, 4xx/5xx/timeout/conexão recusada = não; `https://` é aceito), ou uma conexão TCP em `port`, ou (sem nenhum dos dois) o processo filho estar vivo. Um serviço com command mas sem porta, sem health e sem `ready_log` fica `running` depois de 2 segundos vivo (uptime mínimo), então um crash loop nunca pisca em verde. O mesmo probe é usado pelo watcher de inicialização de cada serviço e pelo tick de health de um segundo.
- Um watcher consulta a cada 500 ms enquanto o serviço está `starting` e a cada 3 s depois que ele fica `running`; ele só promove `starting` → `running` (e a recuperação). Nunca marca como fora do ar um serviço vivo: essa decisão é do tick de health de um segundo, que é dono do contador de falhas, do probe `ready` e do contexto de log, recolhe os filhos que encerraram e atualiza o arquivo de status.
- Um serviço cujo filho iniciado morreu fica `failed` mesmo que outra coisa responda na sua porta (um órfão ou outra ferramenta segurando-a). A exceção é um launcher que sai com `0` (`docker compose up -d`) num serviço com porta/health: aí o probe decide.
- Antes de iniciar um serviço que tem `port`, o Atlas pergunta ao SO quem escuta nela (somente leitura: `lsof` no macOS, `/proc` no Linux, `GetExtendedTcpTable` no Windows). Um dono que não faz parte da execução do próprio projeto (`.logs/.pids` e os seus filhos) é um **conflito de porta**: o log do serviço recebe `port N held by PID P (cmd, cwd …)`, e a nota da barra lateral, o `atlas dev --status`, o Atlas Desktop e o `port_conflict: {pid, cmd, cwd}` do arquivo de status e do MCP dizem o mesmo; o spawn continua mesmo assim (a mensagem do próprio framework fica no log; algumas ferramentas escolhem outra porta). O tick de health olha de novo enquanto o conflito durar e o descarta quando o dono sumir ou a porta passar a ser do projeto. O Atlas nunca encerra um processo por causa da porta: a mensagem diz o que você pode fazer (`stop it yourself (kill P) or change the service port`).
- Serviços sem `command` só têm a porta observada.

### Unhealthy e degraded

Uma vez por segundo, para todo serviço com filho vivo que está `running`, `degraded` ou `unhealthy`:

1. O **liveness** roda (URL de `health`, senão `port`, senão o processo estar vivo), limitado por `health_timeout` (padrão 2 s).
2. Se ele respondeu **e** o serviço tem um [probe `ready:`](configuration.md#saúde-e-prontidão), esse probe roda (limitado pelo próprio `timeout`, padrão 5 s). Ele é pulado enquanto o liveness falha.
3. O veredito é decidido em um só lugar:

| Resultado | Estado |
|---|---|
| respondeu, `ready` passa ou não existe, sem erro repetido | `running` |
| respondeu, `ready` falha | `degraded`, motivo `ready probe failed: <error>` (`GET /ready → 503`, `connection refused`, `timed out after 5s`, `` `cmd` exited 1: <last stderr line> ``) |
| respondeu, a mesma linha de erro ≥ 3 vezes em 60 s | `degraded`, motivo `repeating error: <line>` |
| falhou, menos de `failure_threshold` (padrão 3) seguidas | sem mudança (tolerância: o hot restart de um dev server fecha a porta por um instante) |
| falhou `failure_threshold` seguidas, depois de já ter respondido uma vez | `unhealthy` |
| falhou, ainda nunca respondeu | sem mudança: inicialização, veja [Timeout de inicialização](#timeout-de-inicialização) |

- `unhealthy` vence `degraded`; um probe `ready` falhando vence uma linha de log repetida, que vence um estouro de memória (`memory_warn`, ou `max_memory` com `on_memory_max: notify`): o motivo de memória só aparece quando nada mais específico está errado, e some sozinho dois samples depois de a memória cair. Um serviço `degraded` por memória não satisfaz uma dependência `healthy`, como qualquer `degraded`.
- Os **erros repetidos** contam só linhas de nível error (a mesma detecção do filtro de nível de log). Uma linha é comparada depois que ANSI, timestamps, UUIDs, ids hexadecimais e números são trocados por placeholders e os espaços em branco são colapsados, então `db timeout after 31ms` e `db timeout after 87ms` são o mesmo erro. No máximo 64 formatos distintos são rastreados por serviço. O estado some quando as ocorrências saem da janela de 60 s.
- **Ao entrar em `unhealthy`**, o Atlas captura as últimas 20 linhas de saída do serviço e a hora, gera um alerta crítico (`unhealthy: no answer from the liveness probe (3 misses × 2s timeout)`, com `, restarting` no fim sob `on_unhealthy: restart`) e narra o mesmo texto como nota do scheduler (como `still starting after Ns: …`). Os ticks seguintes mantêm a hora e as linhas originais. O estado some quando o serviço volta a responder.
- **`on_unhealthy: restart`** também reinicia o serviço na primeira vez que ele vira `unhealthy`, com os mesmos atrasos e o mesmo limite de `max_restarts` (padrão 5, `0` = ilimitado); o limite zera depois que o serviço fica no ar por 60 s. Quando o limite acaba, o serviço fica rodando como `unhealthy` com a nota `unhealthy again after its restarts: left running`. O padrão `notify` nunca reinicia.
- **Só um processo vivo** pode ser `unhealthy`. Um processo que sai fica `failed` (ou reinicia conforme a sua [política de reinício](configuration.md#política-de-reinício)); um serviço que ainda não está `running` (ainda `starting`, ou um launcher desacoplado sem filho) mantém a regra antiga: um probe que falha significa `failed`.
- **Dependentes.** Uma condição `healthy` de [`depends_on`](configuration.md#depends_on) só é atendida por `running`; uma dependência `degraded` ou `unhealthy` não a satisfaz. Dependentes que já iniciaram não são parados.
- **O contexto de health é por spawn.** As falhas, o histórico de erros repetidos e as linhas capturadas começam vazios a cada (re)spawn.
- A **nota** do serviço (barra lateral, `note` do MCP, `"reason"` do `atlas dev --ci`) é `unhealthy for 42s: 3 missed probes` para `unhealthy`, e o motivo acima para `degraded`.

### Timeout de inicialização

Um serviço iniciado que espera por uma porta, uma URL de `health` ou o `ready_log` mostra na barra lateral a nota `waiting for :3001 (12s)` / `waiting for ready_log (12s)`. Depois de `start_timeout` segundos (padrão 60), ele registra uma única nota `still starting after Ns: waiting for …` no log, a nota da barra lateral vira `start timeout: still waiting for …` e um serviço `critical` gera o alerta crítico. Nada é encerrado; o serviço continua tentando e vira `running` assim que o probe passa.

### Dados por serviço

Para cada serviço o Atlas registra o último **exit code** (nenhum enquanto roda ou quando o serviço foi morto por um sinal), a **contagem de reinícios** da sequência atual, o **uptime** (enquanto `running`) e a hora do spawn. Eles ficam em `.logs/.tui-status.json` (`exit_code`, `restarts`, `uptime_s`, `started_at`, `note`, `autostart`, `deselected`, mais os [dados de health](#status-file-health-facts) abaixo), nas linhas `atlas_status` do MCP e (`exit_code`, `restarts`) no JSON do `atlas dev --ci`.

### Execuções com seleção (`SERVICE…`, `--profile`)

`atlas dev web` / `--profile backend` iniciam a seleção e os seus `depends_on` transitivos (veja [Selecionar serviços](cli.md#selecionar-serviços)). Todo o resto é rastreado desde o início como `disabled` com `deselected: true` em `.logs/.tui-status.json` (`off` no `atlas dev --status`, na barra lateral e nas linhas do MCP; `"off": true` no JSON do `--ci`): nunca é iniciado no boot nem em *restart all*, é ignorado pelos health checks e pelo veredito do `--ci`, e nunca bloqueia um dependente (com `--no-deps`, um serviço selecionado inicia sem esperar as suas dependências não selecionadas). Iniciar um à mão (menu do serviço, paleta, `atlas_start_service`) remove a marca e, a partir daí, ele se comporta como qualquer outro serviço. Infra: um serviço sem `command` de que nenhum serviço selecionado depende não é consultado, e quando algo está desmarcado e o `atlas.yaml` modela a infra como serviços, o `docker compose up -d` só roda se um serviço selecionado precisar de um; um projeto que não modela os seus serviços do compose como serviços do Atlas continua rodando o compose como antes.

### Início sob demanda (`autostart: false`)

Um serviço assim é pulado quando a stack sobe (e em *restart all*) e fica `stopped` até ser iniciado pelo menu do serviço, pela paleta de comandos ou por `atlas_start_service`.

## Reinícios

Veja [Política de reinício](configuration.md#política-de-reinício) para a configuração. Em runtime: a saída é decidida em um único lugar (o tick de health que recolhe o filho), então um crash nunca gera spawn duplo; o backoff é de 1 s, 2 s, 4 s … limitado a 30 s; o contador zera depois de 60 s de `running` contínuo; stop, restart e quit do usuário cancelam qualquer reinício agendado e zeram o histórico. Enquanto um reinício está pendente, o serviço aparece como `starting`, sem alerta crítico.

## Ações

As ações sem sessão rodam `sh -c` na raiz do projeto, uma de cada vez por ação. O stdout/stderr delas é **persistido**: gravado em `.logs/action-<slug>.log` (`<slug>` = o rótulo em minúsculas, com os caracteres não alfanuméricos colapsados em `-`, por exemplo `Run checks` → `action-run-checks.log`; a execução anterior é rotacionada para `.prev.log`) e enviado ao buffer de logs com o nome de serviço `action:<label>`, então as linhas aparecem na aba **All** e sobrevivem ao desaparecimento da linha de operação. O `atlas_logs` / `atlas_logs_search` do MCP lê esses logs com `service: "action:Run checks"` (ou `action-run-checks`) e, sem `service`, eles são mesclados aos logs dos serviços. Com `--headless`, a saída da ação é impressa como `[action <label>] <line>` e a conclusão como `✓ action <label>: done` / `✗ … failed` no stderr. Os `pause_services` que estão `running` ou `starting` são parados primeiro (estado `suspended`, sem reinício automático) e exatamente esses são reiniciados quando o comando termina, mesmo que ele tenha falhado; um serviço listado que já estava parado, com falha ou desabilitado não é tocado e continua assim. Uma ação `open_tunnel` que precisou abrir o túnel por conta própria o fecha de novo quando termina (veja [Túnel SSH](#túnel-ssh)). As ações de sessão ainda esperam até 5 s para as portas dos serviços pausados ficarem livres antes do spawn. As sessões e os seus filhos são parados ao sair. As ações de sessão com `platform` também recebem `TAURI_DEV_HOST` / `ATLAS_DEV_HOST` no ambiente, resolvidos a partir de [`host_mode`](configuration.md#host_mode) fora do runtime assíncrono (sem nunca sobrescrever um valor que você definiu).

## Túnel SSH

Configurado por [`tunnel:`](configuration.md#tunnel). Um filho `ssh -N -L local:remote_host:remote_port ssh_host` (com `ExitOnForwardFailure`, as opções de keepalive configuradas e `BatchMode=yes`), iniciado na própria sessão com stdin em `/dev/null`. Uma tarefa dedicada cuida dele, então o spawn, a espera de prontidão (a porta local aceita TCP, ≤ 15 s) e as reconexões nunca bloqueiam a UI nem o tick de health de 1 s.

| Situação | Comportamento |
|---|---|
| `enabled` (padrão) | iniciado no boot; o **TUNNEL** da barra lateral mostra `starting` → `running` |
| `enabled: false` | não é iniciado; uma ação com `open_tunnel: true` o inicia sob demanda (o estado aparece do mesmo jeito) e espera até 15 s antes de rodar o comando. Quando essa ação termina, com sucesso ou falha, o túnel é **fechado de novo** (SIGTERM no ssh do próprio Atlas, nenhum `.tunnel.pid` fica). Se várias ações `open_tunnel` se sobrepõem, elas compartilham o túnel e a última a terminar o fecha. Um túnel que já estava no ar (habilitado no boot, ou aberto antes e ainda em uso por outra ação em execução) nunca é fechado por uma ação |
| o ssh sai ou a porta para de responder | estado `failed`, reconexão com backoff de 5 s, 10 s, 20 s … limitado a `reconnect_cooldown`; zera depois de um sucesso |
| o gateway padrão muda (macOS) | reconexão imediata, ignorando o backoff |
| falha ao iniciar | um alerta crítico `tunnel` por episódio de falha, com a mensagem terminando na última linha de stderr do ssh; o stderr do ssh também vai para a aba de log `tunnel` e para `.logs/tunnel.log` |
| porta já atendida por um processo que o Atlas não iniciou | usada como está (estado `running`), nunca encerrada |

O Atlas só envia sinal a um pid que ele mesmo iniciou, ou a um pid de `.logs/.tunnel.pid` que passe pelas verificações abaixo. Ele nunca encerra processos por porta nem por padrão de linha de comando.

**Ao sair** (`tunnel_kill_on_exit`, padrão ligado, `t` na TUI): ligado → SIGTERM no ssh que o Atlas controla. Desligado → o ssh continua rodando e o seu pid, o `ssh_host` e a `local_port` são gravados em `.logs/.tunnel.pid`; ele não entra na limpeza de órfãos do `.pids`. `--headless` e `--ci` sempre encerram o túnel ao sair.

**Reaproveitamento na próxima inicialização.** O `.logs/.tunnel.pid` só é aceito se o `ssh_host` e a `local_port` baterem com a configuração atual, o pid estiver vivo, o processo for `ssh` e a porta local aceitar conexões. Caso contrário, o registro é apagado (nada é encerrado) e um túnel novo é iniciado.

**Testes sem um host real.** `ATLAS_SSH_BIN=/path/to/fake-ssh` substitui o executável `ssh` (os testes de integração usam um script que interpreta o `-L` e escuta na porta local). É só para testes; normalmente deixe sem definir.

## Encerramento

| Gatilho | O que acontece |
|---|---|
| `s` / stop pelo MCP / **Stop** no menu | SIGTERM em toda a [árvore de tarefas](#service-processes) daquele serviço, verificada a cada 50 ms; SIGKILL após 2 s se algo sobrar. O estado vira `disabled`. O histórico de reinícios é zerado. |
| restart (`S`, `r` em uma aba, restart/start pelo MCP) | stop (como acima, estado `stopped`), depois spawn de novo. |
| `r` em **All** | para tudo e depois roda a sequência de inicialização inteira de novo (limpeza de órfãos, infra, scheduler). |
| quit (`q` + confirmação, `Ctrl+C`, SIGTERM, SIGHUP; `--headless`: Ctrl-C/SIGTERM/SIGHUP) | as sessões abertas são paradas; todos os reinícios e inícios na fila são cancelados; toda a [árvore de tarefas](#service-processes) de cada serviço recebe SIGTERM de uma vez, um período de tolerância compartilhado de 800 ms e depois SIGKILL nos sobreviventes; o `.logs/.pids` é removido. Então vale a política de saída do [túnel](#túnel-ssh). O socket de controle é removido ao sair. |

Sair **nunca roda `docker compose down`** (e nunca reinicia o daemon do docker): os containers de infra continuam rodando, e isso é intencional (containers `restart: unless-stopped` sobrevivem entre sessões). Pare-os você mesmo com `docker compose down` quando quiser que eles sumam.

## Limpeza de órfãos

Se o Atlas é morto à força (SIGKILL, crash, queda de energia), os seus filhos podem sobreviver. Na próxima inicialização o Atlas limpa de duas formas, e só essas duas:

1. O **`.logs/.pids`** lista os PIDs de todos os filhos que o Atlas iniciou (reescrito a cada spawn/stop). Na inicialização, cada PID é finalizado **por PID exato** (nunca o grupo de processos da raiz, nunca por porta), com SIGTERM e depois SIGKILL após 300 ms, mas só se:
   - não for o próprio Atlas nem o seu pai, e não for o PID 1;
   - o nome do executável não estiver numa lista de bloqueio de coisas que um PID reciclado poderia ser: `fish`, `zsh`, `dash`, `orb*`, `docker`, `containerd`, `launchd`, `systemd`, `atlas`.
   Para cada raiz verificada, os seus **descendentes** (encontrados percorrendo a árvore de processos pai/filho a partir de um snapshot do `ps`) também são finalizados, então uma tarefa do `turbo` que foi para o próprio grupo de processos não sobrevive segurando a sua porta. Os descendentes só são coletados abaixo de uma raiz verificada; nada é selecionado por porta.
   Depois o arquivo é apagado.
2. O **`lock_file`**, veja abaixo.

O Atlas deliberadamente nunca mata "o que estiver escutando na porta configurada": no macOS com OrbStack isso atingiria o port forwarder do runtime de containers.

### Sessões e higiene

O `.pids` e a cadeia de pais não pegam um processo que **virou daemon** (um navegador aberto por uma execução de testes, um dev server que se separou por fork: ele é reatribuído ao init/launchd e deixa de descender do serviço). Por isso tudo que o Atlas inicia pertence a uma **sessão**: um serviço (`<run>:<service>`), uma ação (`<run>:action/<label>`), uma sessão interativa (`<run>:session/<label>`), um terminal do hostd (o seu id), uma instância `atlas desktop --agent`. Cada raiz iniciada é registrada em `~/.atlas/hygiene/registry.json` com o seu dono `(pid, start time)` (um pid sozinho é reutilizado; o start time identifica um processo para sempre). Como o resto da árvore de uma sessão é encontrado depende do SO:

- **Linux** lê `ATLAS_SESSION=<session>` do ambiente de cada processo (definido por último em todo spawn, então o `env:` de um serviço não consegue reetiquetar a árvore). Ele é herdado por todo descendente, inclusive com `setsid` e fork duplo, então um neto que virou daemon é encontrado mesmo que ninguém o tenha visto.
- **macOS e Windows não conseguem ler o ambiente de outro processo** (o macOS com SIP devolve o argv mas não o ambiente, mesmo para o mesmo usuário; o Windows não tem API para isso). Lá o dono **registra a participação enquanto vive**: no spawn e a cada 2 s (o `atlas dev` no seu tick de health, o hostd no seu próprio tick, uma ação a cada segundo) ele tira um snapshot da tabela de processos e registra todo descendente de cada raiz viva como `(pid, start)`. Um neto que depois vira daemon e é reatribuído ao launchd continua conhecido, porque foi visto antes sob a sua raiz. **A janela:** um processo que faz fork e se separa entre dois snapshots (em menos de cerca de 2 s) nunca é visto e escapa no macOS e no Windows. É um limite do SO, não algo para ajustar; o `atlas doctor --hygiene` avisa isso.

Então:

- **Stop, fim de ação, fechamento de terminal:** depois de encerrar a árvore, todo processo vivo da sessão (tag, membro registrado, árvore atual da raiz) é finalizado: SIGTERM, 2 a 3 s de tolerância, SIGKILL, em **pids exatos**, conferindo de novo o start time de cada pid logo antes de cada sinal. Nunca um grupo de processos, nunca um pid negativo, nunca por porta; um pid registrado cujo start time difere é um estranho e nunca recebe sinal.
- **O dono morreu** (`atlas dev` com SIGKILL, hostd caiu): o hostd registra e confere a cada 2 s (uma leitura da tabela de processos e uma do registro) e recolhe as sessões órfãs, então nada registrado sobra **em até 10 s** (2 s + os 3 s de tolerância). Um `atlas dev` isolado, sem hostd, faz a varredura na próxima inicialização.
- **Uma tag que ninguém registrou** (Linux) só é recolhida quando o processo tem mais de 60 s (um daemon inicia primeiro e se registra alguns milissegundos depois), pela varredura completa periódica.
- Os **diretórios registrados** (diretórios temporários de agentes, perfis de navegador, caches e o diretório de cada instância `atlas desktop --agent`) têm um limite de tamanho (padrão 512 MiB): o diretório de uma sessão morta é apagado inteiro, e um diretório acima do limite perde primeiro as entradas mais antigas (o diretório de uma instância de agente é o seu HOME e nunca é aparado enquanto ela roda, só é removido inteiro depois que ela some). Só caminhos que estão no registro são tocados, e o registro é compartilhado por todo `atlas dev`, pelo hostd e pelo desktop, então uma varredura nunca remove um diretório com menos de 60 s (ele é registrado antes do primeiro processo da sua sessão) nem um cujo dono ainda roda; ela se recusa a registrar um caminho relativo, `/`, uma pasta de sistema de primeiro nível ou a sua home.
- **Spotlight (macOS):** toda raiz de worktree que o Atlas cria (`~/.atlas/worktrees`, a pasta por repositório) e todo diretório que ele registra recebem um arquivo `.metadata_never_index` vazio, para o `mdworker` não indexar as árvores `target/` e `node_modules` (um Mac travou enquanto o Spotlight indexava várias worktrees novas ao mesmo tempo). Nada é apagado nem alterado para isso.

O `atlas doctor --hygiene` mostra tudo isso e o `--fix` aplica ([cli](cli.md#atlas-doctor)); o desktop tem o mesmo em Configurações > Workspaces > Hygiene; os agentes leem com a ferramenta MCP `atlas_hygiene` (somente leitura).

### Arquivos de lock (`lock_file`)

Alguns dev servers mantêm um lock JSON `{"pid": N}` (por exemplo `apps/ai/.mastra/dev.lock`). Antes de o serviço iniciar (e durante a limpeza de órfãos), o Atlas o lê e:

| Dono do lock | Ação |
|---|---|
| ilegível, ou processo morto | apaga o arquivo de lock |
| vivo, diretório de trabalho dentro do projeto, executável fora da lista de bloqueio, **e** iniciado antes ou no mesmo momento em que o arquivo de lock foi gravado | SIGTERM, espera até 2 s, SIGKILL, apaga o arquivo |
| vivo mas fora do projeto, ou o próprio Atlas / o seu pai | não mexe em nada |
| vivo, dentro do projeto, mas parece um PID reciclado (nome na lista de bloqueio, ou iniciado depois de o lock ser gravado) | apaga só o lock obsoleto; o processo não é tocado |

## Singleton e o socket de controle

Um `atlas dev` por diretório de projeto, garantido por um `flock` exclusivo em `.logs/.tui.lock` (o arquivo guarda o PID do dono).

| Modo | Outra instância está rodando |
|---|---|
| TUI, a outra instância é um **runner** (`--detach`, `--headless`) | se **anexa** a ela, veja [Runner e anexar](#runner-e-anexar); nada é encerrado |
| TUI, a outra instância é uma **TUI** | pergunta `atlas dev already running (PID N). Kill it and restart? [y/N]` *antes* de entrar no modo raw. `y`: SIGTERM, até 3 s, depois SIGKILL, e assume. Qualquer outra resposta: sai, mantendo a existente. O PID só é morto se for mesmo um executável `atlas`. |
| `--headless`, `--ci` | imprimem `atlas dev is already running (pid N); use 'atlas dev --status'` e saem com `1`; nada é iniciado e nada é encerrado |
| `--detach` | runner já no ar: imprime `atlas runner already running (pid N) …`, saída `0`. Se uma TUI é dona do projeto: saída `1` |

O lock também é o que torna a limpeza de órfãos segura: só quem tem o lock a executa, então uma segunda execução (CI, headless) nunca pode matar os serviços de uma instância viva.

O **socket de controle** `.logs/atlas.sock` (modo `0600`) só recebe o bind depois que o lock é obtido, substituindo qualquer arquivo obsoleto, e é removido ao sair. A TUI, o runner e o `--headless` o servem; o `--ci` não. Protocolo: um objeto JSON por linha, `{"cmd": "...", "args": {...}}` → `{"ok": bool, "data"?, "error"?}`, com `restart_service`, `start_service`, `stop_service` (argumento `service`) e `run_action` (argumentos `action`, `confirmed`) para o [`atlas mcp`](mcp.md) e as TUIs conectadas, além de `info`, `shutdown` e `subscribe` (veja [Runner e anexar](#runner-e-anexar)). Uma requisição maior que 64 KiB é rejeitada, e uma que não é respondida em 30 s recebe um erro de timeout.

O `--headless` também observa o daemon do docker a cada 5 s e imprime/gera um alerta crítico `docker` quando ele fica inacessível e de novo quando se recupera. Ele não tenta reiniciar o docker.

## Runner e anexar

Os serviços podem sobreviver ao terminal. O **runner** é o engine headless (serviços, scheduler, health checks, socket de controle, heartbeat de status, logs em disco) rodando como daemon; uma TUI passa a ser só um cliente dele.

```text
atlas dev --detach     start the runner and return        (prints pid, exit 0)
atlas dev              attach a TUI to the runner          (same dashboard)
  D / q → Detach       leave; services keep running
atlas dev              ...reattach any time, from any terminal
atlas down             stop everything gracefully         (waits up to 15 s)
```

**Início.** O `--detach` carrega e valida o `atlas.yaml` e a seleção em primeiro plano (os erros são impressos no seu terminal e nada inicia), depois executa de novo `atlas dev --runner` numa sessão nova (`setsid`: sem terminal de controle, então fechar o seu shell nunca pode enviar SIGHUP a ele nem pará-lo com SIGTTIN/SIGTTOU) com o stdin em `/dev/null` e o stdout/stderr em `.logs/runner.log` (truncado a cada início; ele guarda mensagens de ciclo de vida como `● api: Running`, não a saída dos serviços, que fica nos logs em disco de cada serviço). O `--detach` retorna quando o runner responde no socket, depois que o lock é obtido e antes de a instalação de dependências e a infra terminarem (elas seguem no runner). Se o runner sai durante a inicialização, as últimas linhas do `runner.log` são impressas e a saída é `1`. O `--detach` nunca passa pelo caminho de kill da instância anterior: ele é recusado ou não faz nada quando já existe uma instância.

**Quem é um runner.** O `--detach` e uma execução `--headless` em primeiro plano são o mesmo engine; os dois informam `mode: runner` no arquivo de status e em `info`, e em ambos é possível anexar-se e parar com `atlas down`. A única diferença é o stdout: o `--headless` ecoa a saída dos serviços, o daemon não. Uma TUI dona informa `mode: tui` e recusa `subscribe`.

**Anexar.** O `atlas dev` simples primeiro pergunta a quem tem o lock quem ele é (`info`). Um runner aceita clientes anexados: o cliente mantém o próprio terminal, nunca obtém o lock, nunca roda `kill_orphans`, nunca grava o arquivo de status nem o `.pids`, nunca para os serviços quando sai e não carrega serviço nenhum por conta própria. Uma TUI dona mantém a pergunta de matar/manter. O cliente abre uma conexão `subscribe` (stream somente leitura) e envia as suas ações (`restart_service`, `stop_service`, `run_action` com `confirmed: true` depois do próprio modal de confirmação) em conexões curtas separadas, as mesmas requisições que o MCP usa, então toda proteção (`already running`, retomada de `pause_services` pelo runner) vale.

**Protocolo (`subscribe`).** A resposta é `{ok: true, data: <snapshot>}`; depois dela a conexão envia uma linha JSON por evento até um dos lados fechar. O runner monta o snapshot e registra o stream no mesmo turno do loop, então nenhum evento se perde nem se repete entre os dois, e os eventos mantêm a ordem de chegada.

| Campo do snapshot | Significado |
|---|---|
| `project`, `pid`, `mode`, `version`, `started` | quem é e se os serviços de app foram enfileirados |
| `services` | uma linha por serviço: `name`, `state`, `exit_code`, `restarts`, `note`, `completed_ok`, `paused_by`, `deselected` |
| `tunnel` | objeto do túnel ou `null` |
| `logs` | o tail em memória do runner, até 1000 linhas **por serviço**, da mais antiga para a mais nova (`name`, `line`, `stream`, `ts`) |
| `events` | os últimos 50 eventos laterais (linhas `state`, `note`, `alert`, `tunnel` como abaixo), reenviados para reconstruir o bloco EVENTS |
| `operation` | a ação em andamento, se houver |

| Evento (`t`) | Conteúdo |
|---|---|
| `log` | `name`, `line`, `stream` |
| `state` | `name`, `state` + os mesmos dados de uma linha de serviço, no momento da transição |
| `rows` | todas as linhas de serviço + `tunnel`; enviado quando algo que uma linha mostra mudou (no máximo uma vez por segundo, por exemplo a nota da barra lateral que muda, `waiting for :3001 (12s)`) |
| `note`, `alert` | narração do scheduler / alerta crítico |
| `tunnel` | `state` + objeto do túnel |
| `action_progress`, `action_done` | linha de saída / conclusão de uma ação sem sessão (o runner retoma os serviços pausados por conta própria) |
| `started` | todos os serviços de app foram enfileirados |
| `bye` | o runner está encerrando; o stream fecha em seguida |

Um cliente que desconecta (de forma limpa ou não) é descartado na próxima escrita ou na manutenção de um segundo e nunca afeta o runner nem os outros clientes; o `atlas dev --status` mostra a contagem ao vivo. O mesmo código `LogBuffer`/`apply_manager_event` que renderiza uma TUI dona renderiza uma TUI anexada: os eventos do stream entram no App como eventos comuns do manager e o manager é um *espelho* (dados derivados da configuração reais, estado e dados vindos do runner, nada é iniciado).

**Limites.** As sessões interativas (`session: true`) e o seletor de dispositivo móvel só funcionam no dono (o runner as recusa com um erro claro; a TUI mostra um toast). O runner sempre encerra o túnel ao sair. Uma TUI que é dona dos seus serviços não consegue passá-los a um runner enquanto roda: inicie o runner primeiro (`atlas dev --detach`).

**Encerramento.** `atlas down`, a opção **Stop everything** de uma TUI anexada, e SIGTERM/SIGHUP no runner (e Ctrl-C de um `--headless` em primeiro plano) executam a [sequência de encerramento](#encerramento) normal; os clientes recebem `bye`.

## Arquivos em `.logs/`

O `.logs/` é criado na raiz do projeto com o seu próprio `.gitignore` (`*`), então nenhum dos arquivos dele pode ser staged nem commitado; o `.gitignore` do seu projeto nunca é tocado, e um `.logs/.gitignore` que você mesmo escreveu é mantido.

| Arquivo | Gravado por | Conteúdo |
|---|---|---|
| `<service>.log` | todo serviço iniciado | `HH:MM:SS [stdout\|stderr] text`, ANSI removido, uma linha por entrada |
| `<service>.prev.log` | rotação | o log da execução anterior (ou o segmento anterior de 50 MB) |
| `.tui-status.json` | manager, a cada segundo | snapshot de status (abaixo). Gravado de forma atômica (arquivo temporário + rename). |
| `.tui-metrics.json` | manager, a cada 5 s | série de memória por serviço (última hora) e dados, para o `atlas_metrics` do MCP ([dados de memória](#status-file-memory-facts)). Gravado de forma atômica. |
| `.tui.lock` | o atlas em execução | `flock` singleton, guarda o PID |
| `.pids` | manager | PIDs dos filhos vivos, um por linha; removido na saída limpa |
| `tunnel.log` | tarefa do túnel | stderr do ssh e notas de reconexão (mesmo formato de linha; rotacionado para `tunnel.prev.log` a cada execução) |
| `.tunnel.pid` | tarefa do túnel | só quando um túnel é deixado rodando na saída: pid, `ssh_host`, `local_port` (um por linha) |
| `atlas.sock` | TUI / runner / `--headless` | socket de controle |
| `atlas-audit.log` | servidor do socket de controle, TUI | trilha de auditoria das chamadas de controle (restart/stop/start de serviço, executar ação): um objeto JSON por linha com `ts`, `caller` (`human` ou `agent`), `client`, `session`, `tool`, `target`, `args`, `result` (`allowed`, `denied`, `error`). O arquivo da execução anterior é `atlas-audit.prev.log`; ele também é rotacionado acima de 5 MB. Leia com `atlas_logs` `service: "audit"`. Veja [MCP › Permissões de agentes e auditoria](mcp.md#permissões-do-agente-e-auditoria) |
| `runner.log` | `atlas dev --detach` | stdout/stderr do runner que virou daemon (mensagens de ciclo de vida, não a saída dos serviços); truncado a cada início |
| `.atlas-ui.json` | TUI | memória de UI por projeto: aba ativa (por nome), zoom, alternâncias de quebra de linha/timestamps/JSON, filtro de nível, comandos recentes da paleta. Melhor esforço; um arquivo ausente ou corrompido vira um início do zero |

**Rotação.** Quando um serviço é (re)iniciado, um `<service>.log` existente é renomeado para `<service>.prev.log` (substituindo o antigo) e um arquivo novo é começado. Enquanto o serviço roda, um log que passa de 50 MB é rotacionado do mesmo jeito. Não há mais histórico além disso: só existem o arquivo atual e o anterior. Serviços sem `command` e sessões não gravam arquivos de log (as ações sem sessão gravam: `.logs/action-<slug>.log`, veja [Ações](#ações)).

**Arquivo de status** (`.tui-status.json`):

```json
{
  "pid": 4242,
  "mode": "runner",
  "clients": 1,
  "session": "4242-1767225600",
  "started_at": 1767225600,
  "updated_at": 1767225660,
  "log_dir": ".logs",
  "services": {
    "api": { "state": "running", "port": 3001, "health": "http://localhost:3001/health" },
    "web": { "state": "starting", "port": 3002, "health": null }
  },
  "tunnel": { "state": "running", "local_port": 54320, "remote": "localhost:5432", "ssh_host": "prod-db" }
}
```

`mode` é `runner` (`--detach` / `--headless`, aceita anexar) ou `tui`; `clients` é o número de TUIs conectadas. `tunnel` é `null` quando nenhum túnel está rodando (nenhum configurado, ou `enabled: false` e ainda não aberto sob demanda).

`state` é em minúsculas (`starting`, `running`, `degraded`, `unhealthy`, `failed`, `stopped`, `disabled`, `suspended`). `updated_at` é um timestamp Unix, o heartbeat: o [`atlas dev --status`](cli.md#--status) e o MCP só tratam a execução como viva se o `pid` existir e o heartbeat tiver menos de 120 s, então um arquivo deixado por um crash é corretamente informado como "not running".

<a id="status-file-health-facts"></a>**Dados de health.** Toda linha de serviço do arquivo de status também traz estas chaves (os eventos `state` e `rows` do runner trazem as mesmas, então uma TUI ou o app desktop conectado a um runner mostra exatamente o que o arquivo diz):

| Chave | Significado |
|---|---|
| `ready` | resultado do probe `ready:` do serviço: `true` enquanto `running`, `false` enquanto `degraded` por um probe que falha; `null` sem um probe `ready:`, enquanto o serviço não está no ar, ou quando `degraded` só por um erro repetido ou `unhealthy` |
| `probe_error` | por que um serviço `degraded` está degradado (a mensagem do probe, ou a linha repetida); `null` nos outros casos |
| `unhealthy_since` | segundos unix em que o serviço ficou `unhealthy`; `null` nos outros casos |
| `unhealthy_misses` | falhas de liveness seguidas naquele momento; `null` nos outros casos |
| `unhealthy_lines` | as últimas linhas de saída (até 20) naquele momento; `[]` nos outros casos |

O `atlas dev --status` acrescenta uma coluna `READY` (`ok` / `fail` / `-`) só quando algum serviço tem um probe `ready:`, então a saída não muda nos outros casos. Sob uma linha `degraded` ele imprime `↳ <probe_error>`, e sob uma `unhealthy`, `↳ unhealthy for 42s (3 missed probes); last output:` seguido das linhas capturadas.

<a id="status-file-memory-facts"></a>**Dados de memória** ([#131](https://github.com/codeatlasdev/atlas/issues/131); veja [Memória](configuration.md#memória)). Toda linha de serviço também traz estas chaves, do mesmo produtor para o arquivo e para o wire do runner:

| Chave | Significado |
|---|---|
| `memory_rss` | bytes de toda a árvore de processos (no macOS, o physical footprint), arredondados para MB inteiros; `null` enquanto o serviço não está rodando |
| `memory_trend` | `rising`, `flat` ou `falling` nos últimos 5 minutos (abaixo de ±2 MB/min é `flat`); `null` até existirem cerca de 30 s de samples |
| `memory_trend_mb_min` | MB por minuto, com sinal, por trás da tendência, arredondado; `null` junto com ela |
| `budget_state` | `ok`, `warn` (acima de `memory_warn`) ou `over` (acima de `max_memory`), depois de dois samples seguidos; `ok` sem orçamento |
| `memory_leak` | `leak suspected: +600 MB over 12 reloads` enquanto o RSS só cresce de um reload ou restart para o seguinte; `null` nos outros casos |

Uma linha de um atlas mais antigo não tem nenhuma dessas chaves e é lida como nada conhecido. O `atlas dev --status` imprime uma linha `↳ memory 2.1 GB · rising +42 MB/min · budget warn · leak suspected: …` sob um serviço só quando ele tem algo a dizer (acima de um orçamento, suspeita de vazamento ou tendência de alta), então a tabela de um projeto saudável não muda.

O `.tui-metrics.json` guarda a série para o [`atlas_metrics`](mcp.md#atlas_metrics) do MCP: por serviço, `samples` como pares `[unix_s, bytes]` (do mais antigo para o mais novo, um a cada `interval_s` = 5, os últimos 720 = uma hora), os dados acima e `cycles` (reloads e restarts que a vigilância de vazamento contou). Ele é reescrito com o temp-and-rename do arquivo de status e pertence ao `atlas dev` em execução; um cliente anexado nunca o grava.

## Resumo dos códigos de saída

| Comando | Saída |
|---|---|
| `atlas dev` encerrado normalmente / desacoplado | `0` |
| `atlas dev --detach` | `0` iniciado ou já rodando / `1` falhou |
| `atlas down` | `0` parado ou não estava rodando / `1` ainda vivo após 15 s |
| `atlas dev` sem terminal | `2` |
| `atlas dev --status` | `0` rodando / `3` não rodando |
| `atlas dev --ci` | `0` ok / `1` não ok / `130` interrompido |
| `--headless` / `--ci` com outra instância | `1` |
