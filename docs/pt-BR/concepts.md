---
title: "Conceitos"
source: concepts.md
source_sha: "1924d530a7c8609a9cf8ef47e4029044a321d355"
translation: machine
---

# Conceitos

## Projeto

Um diretório com um `atlas.yaml`. O arquivo lista os serviços que rodam e tudo o que fica em volta deles. O `atlas.local.yaml`, ao lado dele, sobrescreve um subconjunto por máquina. O Atlas mantém os arquivos de runtime no diretório `.logs/` do projeto (adicione-o ao `.gitignore`).

## Serviço

Uma coisa que a stack roda. Um serviço com `command` é iniciado pelo Atlas (`sh -c`, em uma sessão própria, com stdin em `/dev/null`). Um serviço sem `command` é **gerenciado por fora** (em geral, um container docker do seu arquivo compose): o Atlas nunca o inicia e só observa a `port` dele.

Um serviço está sempre em um destes estados:

| Estado | Significado |
|---|---|
| `starting` | iniciado, ainda não está pronto |
| `running` | pronto: a porta ou a sonda `health` passou, ou o `ready_log` bateu (e a sonda `ready:`, quando existe) |
| `degraded` | responde, mas não está utilizável: a sonda `ready:` falha, ou o mesmo erro se repete no log |
| `unhealthy` | o processo está vivo, mas parou de responder à sonda de `health`/`port` (travou) |
| `failed` | saiu de forma anormal, uma dependência falhou ou o orçamento de reinícios acabou |
| `stopped` | não está rodando; também é o estado de um one-shot que terminou |
| `disabled` | parado por você, ou deixado de fora desta execução por `atlas dev SERVICE…` |
| `suspended` | pausado pelo `pause_services` de uma ação |

`failed`, `unhealthy` e `degraded` são problemas diferentes. **`failed`**: o processo sumiu (ou nunca subiu), e o Atlas só o reinicia conforme a política de reinício. **`unhealthy`**: nada saiu, o processo só deixou de responder (um servidor SSR travado, um loop em deadlock); ele volta a `running` sozinho se voltar a responder, e `on_unhealthy: restart` pode reiniciá-lo. **`degraded`**: ele responde à sonda de liveness, mas uma verificação mais profunda diz que não está utilizável; o estado some quando a verificação passa ou o erro para de se repetir. `degraded` e `unhealthy` vêm com um motivo e, no caso de um serviço travado, com as últimas linhas de saída dele. Configure-os com [`health_timeout`, `failure_threshold`, `ready` e `on_unhealthy`](configuration.md#health-and-readiness).

Os serviços iniciam quando todas as condições de [`depends_on`](configuration.md#depends_on) são atendidas (`started`, `healthy`, `completed_successfully`, `log_ready`) e reiniciam conforme a [política de reinício](configuration.md#restart-policy). Detalhes em [Runtime](runtime.md).

## Profile

Um grupo nomeado de serviços (`profiles:` no `atlas.yaml`) que você inicia com `atlas dev --profile backend`. A seleção sempre inclui as dependências transitivas.

## Ação

Um comando ligado a uma tecla (`actions:`), para o que você faz ao lado da stack: zerar o banco de dados, rodar as verificações, abrir um simulador de iOS. Uma ação pode pedir `confirm` antes, usar `pause_services` enquanto roda, abrir uma `session` interativa ou mirar uma `platform` mobile. As ações aparecem na ajuda do TUI, na paleta de comandos, no menu **Ações** do desktop e via MCP, onde o `confirm` vira uma trava que exige `confirmed: true`.

## Runner, TUI e attach

O **runner** é o motor que controla os serviços: agendador, health checks, socket de controle, heartbeat de status e logs em disco. Um TUI pode controlá-lo (o `atlas dev` simples), ou o runner pode viver sozinho como daemon (`atlas dev --detach`, ou `--headless` em primeiro plano). O TUI vira então só um cliente que anexa e desanexa sem tocar nos serviços. O app desktop, o TUI e o MCP conversam com o mesmo runner por `.logs/atlas.sock`.

```text
atlas dev --detach   start the runner and return
atlas dev            attach a TUI (D detaches; services keep running)
atlas down           stop everything gracefully
```

## Vocabulário do Atlas Desktop

**Projeto.** Um checkout git principal que você adicionou ao app. Fica em `~/.atlas/desktop/projects.json`.

**Space.** A visão que o app tem de um projeto. Ele mantém as próprias worktrees, abas, painéis e docks, e tem uma cor de destaque própria para você saber sempre em que projeto está. Troque de space com ⌥⌘← / ⌥⌘→ ou ⌃1…⌃9.

**Worktree.** Um `git worktree` do projeto, criado pelo Atlas em `~/.atlas/worktrees/<repo>/<name>`, em uma branch própria. Uma worktree é uma tarefa: tem terminais, sessões de agente, diff e (opcionalmente) uma cópia própria da stack. Cada uma recebe um **port offset** (100, 200, …) para que os serviços rodem ao lado dos do checkout principal sem conflito de portas (`ATLAS_PORT_OFFSET`).

**Painel.** A primeira aba de toda worktree: saúde da stack, CPU e memória ao vivo por serviço, a faixa de setup e os cards dos serviços.

**Agente.** Uma CLI de agente de código (omp, Claude Code, Codex) rodando em um terminal da worktree. Os hooks informam o estado dele ao app: trabalhando, precisa de você, concluído.

**Dock.** Uma região lateral ou inferior em volta dos painéis: o dock da esquerda reúne o explorador, a pesquisa e o controle de versão; o da direita reúne o histórico dos agentes e as portas em escuta; o inferior é um grupo de abas de terminal.

## hostd

O `atlas hostd` é um daemon por usuário que o Atlas Desktop inicia. Ele controla os PTYs, a execução de comandos one-shot, o acesso a arquivos e o scrollback dos terminais, por isso os terminais e os agentes continuam rodando quando você fecha, derruba ou atualiza o app. Ele escuta em `~/.atlas/hostd.sock` (token em `~/.atlas/hostd.token`) e também roda em um host SSH, então uma worktree remota se comporta como uma local. Veja a [Arquitetura](architecture.md#hostd).

## Skill

Instruções para agentes de IA, compiladas dentro do binário `atlas` para sempre corresponderem à versão que você roda. O `atlas setup` ou o `atlas install` as copia para `~/.agents/skills/` (e para `~/.claude/skills/`, no caso do Claude Code). As skills são `atlas` (o ambiente de desenvolvimento), `atlas-desktop` (controlar o app com `atlas wt|term|browser|file`) e `atlas-install`.

## Onde o Atlas mantém as coisas

| Onde | O quê |
|---|---|
| `<project>/.logs/` | logs por serviço, arquivo de status, socket de controle, locks. Veja [Arquivos em `.logs/`](runtime.md#arquivos-em-logs) |
| `~/.atlas/bin/atlas` | a CLI instalada |
| `~/.atlas/desktop/` | configurações do desktop (`settings.json`), projetos, layout do workspace, temas |
| `~/.atlas/worktrees/` | worktrees criadas pelo app desktop |
| `~/.atlas/hostd.*`, `~/.atlas/hostd/` | o host de terminais: socket, token, pid, log, scrollback |
| `~/.atlas/desktop.sock` | socket de controle do app desktop em execução (`atlas wt`, `atlas term`…) |
| `~/.atlas/omp-plugin/` | o plugin do omp escrito por `atlas setup omp` |
| `~/.agents/skills/` | skills incluídas para o omp e o Codex |
