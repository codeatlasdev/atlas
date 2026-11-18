---
title: "Quickstart"
source: quickstart.md
source_sha: "0da0e978c6945a1e232af30cd202e01766814c87"
translation: machine
---

# Quickstart

Você precisa do Atlas no seu `PATH` ([Instalação](installation.md)) e de um projeto com algo para rodar.

## 1. Criar o `atlas.yaml`

Na raiz do projeto:

```bash
atlas init
```

O Atlas detecta o tipo do projeto e escreve um `atlas.yaml` mínimo. Ele nunca sobrescreve um arquivo existente. Edite o arquivo para descrever o que você roda:

```yaml
# yaml-language-server: $schema=.atlas.schema.json
name: myapp

services:
  database:             # sem command: gerenciado por fora (docker), só a porta é observada
    port: 5432

  api:
    command: bun run dev:api
    port: 3000
    health: http://localhost:3000/health
    depends_on: [database]
    open_key: a         # aperte `a` no dashboard para abrir http://localhost:3000

  web:
    command: bun run dev:web
    port: 3001
    depends_on: [api]
    open_key: w

infra:
  compose_file: docker-compose.yml   # `docker compose up -d` roda antes dos serviços da aplicação
```

Valide o arquivo e ative o autocompletar no editor:

```bash
atlas config check                       # imprime OK, ou todos os erros de uma vez
atlas config schema > .atlas.schema.json
```

Os erros de digitação aparecem antes de qualquer coisa iniciar: `services.web.helth: unknown field (did you mean 'health'?)`.

## 2. Rodar

```bash
atlas dev
```

Você ganha um dashboard com uma aba por serviço, logs combinados em **All** e o estado dos serviços na barra lateral. Aperte `?` para ver as teclas, `/` para pesquisar nos logs e `q` para sair (todos os serviços são parados). Veja o [guia do TUI](tui.md).

Outras formas de rodar o mesmo ambiente:

| Comando | Uso |
|---|---|
| `atlas dev web` | inicia o `web` e o que ele tem em `depends_on`; o resto aparece como `off` |
| `atlas dev --detach` | inicia em um runner em segundo plano e volta ao prompt; rode `atlas dev` para anexar e `atlas down` para parar |
| `atlas dev --headless` | sem TUI, logs na saída padrão, ainda controlável por MCP |
| `atlas dev --ci` | inicia tudo, espera a saúde dos serviços, imprime JSON e sai com `0` ou `1` |
| `atlas dev --status` | imprime o estado de uma stack em execução (sai com `0` se está rodando, `3` se não) |

## 3. Ajustes por máquina

Crie o `atlas.local.yaml` ao lado do `atlas.yaml` e coloque-o no gitignore. Ele sobrescreve portas, desativa serviços ou pula a instalação de dependências em uma única máquina. Veja [Overrides locais](configuration.md#local-overrides-atlaslocalyaml).

## 4. Conectar um agente de IA

```bash
atlas setup                        # detecta omp, Claude Code e Codex e mostra o que rodar
atlas setup claude-code --apply    # servidor MCP + skills + hooks
```

Reinicie o agente, abra-o no projeto e pergunte *"por que o api está falhando?"* ou *"reinicie o web e espere até ele ficar pronto"*. Veja [Configuração dos agentes](agents.md) e [MCP](mcp.md).

## 5. Experimentar o app desktop

```bash
atlas desktop
```

Adicione o projeto, aperte **Iniciar** e abra um terminal com ⌘T. Uma git worktree por tarefa, um agente em cada uma e a sua stack rodando ao lado do checkout principal: veja o [App desktop](desktop.md).

## Em seguida

- [Conceitos](concepts.md): o vocabulário em uma página.
- [Configuração](configuration.md): condições de dependência, política de reinício, ações, profiles.
- [Runtime](runtime.md): o que acontece ao iniciar e ao sair.
