---
title: "Introdução"
source: introduction.md
source_sha: "a023656cb558dfcbab21d68dad3df221296fc7c5"
translation: machine
---

# Introdução

O Atlas é uma plataforma de desenvolvimento para todo mundo: o caminho direto do `git clone` até uma stack rodando e observável, sem precisar de uma equipe de plataforma. A partir de um único `atlas.yaml`, ele sobe todo o ambiente de desenvolvimento do projeto: infraestrutura em docker, serviços da aplicação em ordem de dependência, reinícios com backoff, health checks e logs com busca. Você tem três formas de controlá-lo, e todas enxergam o mesmo estado ao vivo:

- **`atlas dev`**: um dashboard no terminal (o [TUI](tui.md)).
- **Atlas Desktop**: um app nativo para macOS, Windows e Linux com terminal (desenhado na GPU no macOS), git worktrees, agentes de código, revisão de diff e navegador embutido.
- **`atlas mcp`**: um servidor [Model Context Protocol](mcp.md). Um agente de IA lê o status e os logs, reinicia um serviço e espera até ele ficar saudável enquanto você continua trabalhando.

> **Status.** O Atlas é open source (MIT ou Apache-2.0) e feito para todo mundo. Disponível agora: macOS em Apple silicon, Windows 10/11 x64 e Linux x86_64 (a CLI, o TUI, o servidor MCP e o Atlas Desktop; [baixar](https://atlas.codeatlas.com.br/download/)). Em breve: Macs Intel, Windows em Arm e Linux em Arm.

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
| Logs | Montagem confiável das linhas, buffers limitados, saltos entre erros e avisos, URLs e `path:line` clicáveis | [Logs](logs.md) |
| Desktop | Um space por projeto, uma worktree por tarefa, terminais persistentes, status e notificações dos agentes, revisão, PRs, navegador | [App desktop](desktop.md) |
| Terminal GPU | Renderizador Metal a 120 Hz, gráficos Kitty e sixel, ligaduras, integração com o shell | [Terminal GPU](terminal.md) |
| MCP | Nove ferramentas, três recursos, três prompts | [MCP](mcp.md) |
| Configuração dos agentes | `atlas setup omp\|claude-code\|codex` configura MCP, skills e hooks | [Configuração dos agentes](agents.md) |
| Deploy | Releases atômicos, condicionados à saúde do serviço, com rollback automático | [Deploy](deploy.md) |

## Por onde começar

1. [Instale o Atlas](installation.md), a partir de uma release ou do código-fonte.
2. Siga o [Quickstart](quickstart.md): `atlas init`, edite, `atlas dev`.
3. Passe os olhos em [Conceitos](concepts.md) para aprender o vocabulário (projetos, spaces, worktrees, serviços, ações, hostd).
4. Consulte a [Configuração](configuration.md) e a [referência da CLI](cli.md).

Algo quebrou? Veja [Solução de problemas](troubleshooting.md) e o [FAQ](faq.md). Quer mexer no próprio Atlas? Veja a [Arquitetura](architecture.md) e [Contribuir](contributing.md).
