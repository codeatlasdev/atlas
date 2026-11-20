---
title: "Introdução"
source: introduction.md
source_sha: "7ab9690279705d152ca0e223e869f1954d760ece"
translation: machine
---

# Introdução

O Atlas é um espaço de trabalho gratuito e de código aberto para trabalhar com agentes de IA. Ele roda seus projetos, dá a cada agente a própria cópia do código (uma git worktree, com portas e terminal próprios) e mantém uma tripulação de agentes trabalhando num servidor Linux seu, de dia e de noite. O que for importante espera a aprovação de uma pessoa, e cada passo fica registrado.

A imagem que usamos: para atravessar o oceano, é preciso **mapa**, **navio** e **tripulação**, com um **capitão** que responde pelo resultado.

| | No Atlas | Leia |
|---|---|---|
| O mapa | Como cada projeto funciona, escrito uma vez no `atlas.yaml` (ou detectado sem ele), legível por pessoas e agentes | [Configuração](configuration.md) |
| Os navios | Seus computadores e servidores: o Atlas Desktop e a CLI na sua máquina, um crew server em Linux, o app de celular | [App desktop](desktop.md), [Atlas Crew](crew.md), [Mobile](mobile.md) |
| A tripulação | Os agentes que você já usa (Claude Code, Codex, omp, OpenCode), cada um na sua worktree, ou como crewmates com nome, função, fila e memória | [Configuração dos agentes](agents.md), [Atlas Crew](crew.md) |
| O capitão | Você: modos de permissão, cards de aprovação, um registro do que cada agente fez | [Atlas Crew](crew.md#tasks-and-approvals) |

Para onde ele vai se chama **Grand Line**: cada projeto uma ilha, cada computador um navio, cada tripulação com um capitão, na empresa inteira. O que já existe está abaixo; os planos ficam no [GitHub](https://github.com/codeatlasdev/atlas/issues).

Você controla o mesmo estado ao vivo de três lugares:

- **Atlas Desktop**: um app nativo para macOS, Windows e Linux com projetos, terminais (desenhados na GPU no macOS), git worktrees, agentes de código, Atlas Crew, revisão de diff e navegador embutido.
- **`atlas dev`**: um painel no terminal (o [TUI](tui.md)), e `--headless` / `--ci` para scripts.
- **`atlas mcp`**: um servidor [Model Context Protocol](mcp.md). Um agente lê o status e os logs, reinicia um serviço e espera até ele ficar saudável.

> **Status.** O Atlas é de código aberto (MIT ou Apache-2.0) e está em alfa. Disponível agora: macOS em Apple silicon, Windows 10/11 x64 e Linux x86_64 (a CLI, a TUI, o servidor MCP e o Atlas Desktop; [baixar](https://atlas.codeatlas.com.br/pt-br/download/)); crew servers em Linux x86_64 e aarch64. Em breve: desktops em Macs Intel, Windows em Arm e Linux em Arm.

## O problema que ele resolve

Um monorepo moderno precisa de Postgres, Redis, uma migration, uma API, um app web e um worker, iniciados na ordem certa, nas portas certas e reiniciados quando caem. As respostas de sempre são um monte de `sleep 10 && bun dev`, um `Procfile` sem health checks ou um README que ninguém lê.

Com o Atlas você descreve o grafo uma vez:

```yaml
name: myapp

infra:
  compose_file: docker-compose.yml

services:
  migrate:
    command: bun run db:migrate
    oneshot: true
  api:
    command: bun run dev:api
    port: 3000
    health: http://localhost:3000/health
    depends_on: { migrate: completed_successfully }
    restart: on_failure
  web:
    command: bun run dev:web
    port: 3001
    depends_on: { api: healthy }
```

e roda `atlas dev`. O Postgres e o Redis sobem, a migration roda primeiro, a API espera até o `/health` responder, o app web espera a API, e os serviços instáveis reiniciam com backoff exponencial. Cada linha de log vai para o dashboard e para o `.logs/` em disco.

## O que vem na caixa

| Peça | O que faz | Leia |
|---|---|---|
| Runtime | Condições de dependência (`started`, `healthy`, `completed_successfully`, `log_ready`), política de reinício, limpeza de órfãos pelo PID exato, túnel SSH, ações com confirmação | [Runtime](runtime.md) |
| TUI | Uma aba por serviço, busca nos logs com regex, filtros de nível e de tempo, compactação de logs JSON, suporte a mouse, paleta de comandos | [TUI](tui.md) |
| Logs | Montagem de linhas que aguenta rajadas, buffers limitados, saltos entre erros e avisos, URLs e `path:line` clicáveis | [Logs](logs.md) |
| Atlas Crew | Crewmates no seu servidor Linux: modos de permissão, aprovações, automações (horários, GitHub, webhooks), Telegram, voz | [Atlas Crew](crew.md) |
| Desktop | Um space por projeto, uma worktree por tarefa, terminais persistentes, status e notificações dos agentes, revisão, PRs, navegador | [App desktop](desktop.md) |
| Terminal GPU | Renderizador Metal a 120 Hz, gráficos Kitty e sixel, ligaduras, integração com o shell | [Terminal GPU](terminal.md) |
| MCP | Nove ferramentas, três recursos, três prompts | [MCP](mcp.md) |
| Configuração dos agentes | `atlas setup omp\|claude-code\|codex` configura MCP, skills e hooks | [Configuração dos agentes](agents.md) |
| Deploy | Releases atômicas, condicionadas à saúde do serviço, com rollback automático | [Deploy](deploy.md) |

## Por onde começar

1. [Instale o Atlas](installation.md), a partir de uma versão ou do código-fonte.
2. Siga o [Quickstart](quickstart.md): `atlas init`, edite, `atlas dev`.
3. Passe os olhos em [Conceitos](concepts.md) para aprender o vocabulário (projetos, spaces, worktrees, serviços, ações, hostd).
4. Consulte a [Configuração](configuration.md) e a [referência da CLI](cli.md).

Algo quebrou? Veja [Solução de problemas](troubleshooting.md) e o [FAQ](faq.md). Quer mexer no próprio Atlas? Veja a [Arquitetura](architecture.md) e [Contribuir](contributing.md).
