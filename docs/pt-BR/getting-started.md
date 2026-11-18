---
title: "Primeiros passos"
source: getting-started.md
source_sha: "3991964212b8c09e642ad8804de09d262ceeb106"
---

# Primeiros passos

Três passos. Cada um leva à página completa quando você quiser os detalhes.

## 1. Instalar

```bash
curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh
atlas version
```

O instalador verifica o checksum da versão, coloca o `atlas` em `~/.atlas/bin` e configura o seu `PATH` (abra um novo shell depois). As versões prontas cobrem macOS em Apple silicon, Linux x86_64 e Windows x64. No Linux você também pode instalar um [.deb, .rpm, pacote Arch ou AppImage](../installation.md#pacotes-para-linux), e no Windows você roda `irm https://atlas.codeatlas.com.br/install.ps1 | iex` no PowerShell ([Windows](../installation.md#windows)). Instalação manual, atualizações e desinstalação: [Installation](../installation.md).

## 2. Descrever a sua stack

Na raiz do projeto:

```bash
atlas init                              # cria o atlas.yaml
atlas config schema > .atlas.schema.json   # autocompletar no editor
```

```yaml
# yaml-language-server: $schema=.atlas.schema.json
name: myapp

services:
  database:
    port: 5432            # sem command: gerenciado por fora (docker), só a porta é observada

  api:
    command: bun run dev:api
    port: 3000
    health: http://localhost:3000/health
    depends_on: [database]
    open_key: a

  web:
    command: bun run dev:web
    port: 3001
    depends_on: [api]

infra:
  compose_file: docker-compose.yml
```

`atlas config check` valida o arquivo e mostra todos os erros de uma vez. Cada campo está documentado em [Configuration](../configuration.md).

## 3. Rodar

```bash
atlas dev
```

Uma aba por serviço, logs combinados em **All** e o estado dos serviços na barra lateral. `?` mostra as teclas, `/` busca nos logs e `q` sai e para tudo. Veja o [guia do TUI](../tui.md).

Para deixar a stack rodando em segundo plano: `atlas dev --detach`, depois `atlas dev` para anexar e `atlas down` para parar.

## Próximos passos

| Quero… | Leia |
|---|---|
| percorrer o fluxo completo com mais detalhes | [Quickstart](../quickstart.md) |
| entender o vocabulário | [Concepts](../concepts.md) |
| usar o app nativo | [Desktop app](../desktop.md) |
| deixar um agente de IA ler logs e reiniciar serviços | [Agent setup](../agents.md), [MCP](../mcp.md) |
| achar uma flag ou um código de saída | [CLI reference](../cli.md) |
| consertar algo que quebrou | [Troubleshooting](../troubleshooting.md) |
