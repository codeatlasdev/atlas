---
title: "Documentação do Atlas"
source: README.md
source_sha: "4ec68703f6dbf2f2380277a822ac5f34e24a13a5"
translation: machine
---

# Documentação do Atlas

O Atlas sobe o ambiente de desenvolvimento inteiro de um projeto a partir de um único `atlas.yaml`: infraestrutura, serviços, ordem de dependência, reinícios e logs. Você controla tudo por uma TUI, por um app desktop nativo ou por um agente de IA via MCP. Estas páginas também estão publicadas em [atlas.codeatlas.com.br/pt-br/docs](https://atlas.codeatlas.com.br/pt-br/docs/).

## Comece aqui

| Página | Leia quando você quiser… |
|---|---|
| [Introdução](introduction.md) | saber o que é o Atlas e o que vem na caixa |
| [Primeiros passos](getting-started.md) | instalar, escrever um `atlas.yaml` e rodar, em três passos |
| [Instalação](installation.md) | instalar a partir de uma versão, atualizar, compilar do código-fonte, desinstalar |
| [Quickstart](quickstart.md) | o mesmo fluxo com mais detalhes, além de agentes e do app desktop |
| [Conceitos](concepts.md) | aprender o vocabulário: projetos, spaces, worktrees, serviços, ações, runner, hostd |

## Guias

| Página | Leia quando você quiser… |
|---|---|
| [App desktop](desktop.md) | trabalhar com spaces, worktrees, agentes, revisão, o navegador e hosts SSH |
| [Terminal GPU](terminal.md) | saber como o terminal nativo desenha, o que ele aceita e qual é o desempenho |
| [TUI](tui.md) | conhecer o dashboard, todos os atalhos de teclado e o visualizador de logs |
| [Logs](logs.md) | entender como os logs são coletados, armazenados, pesquisados e expostos |
| [Configuração dos agentes](agents.md) | rodar `atlas setup omp\|claude-code\|codex` |
| [Atlas Crew](crew.md) | rodar agentes com nome 24 h por dia no seu servidor Linux e controlá-los pelo desktop, pelo celular ou pelo Telegram |
| [MCP](mcp.md) | conectar qualquer cliente MCP ao seu ambiente em execução |
| [Deploy](deploy.md) | enviar releases a um servidor com troca atômica e rollback automático |

## Referência

| Página | Leia quando você quiser… |
|---|---|
| [CLI](cli.md) | consultar um comando, uma flag ou um código de saída |
| [Configuração](configuration.md) | consultar qualquer campo do `atlas.yaml`, overrides locais, validação, JSON Schema |
| [Configurações do desktop](desktop-settings.md) | editar `~/.atlas/desktop/settings.json` à mão |
| [Runtime](runtime.md) | entender a ordem de inicialização, reinícios, saúde dos serviços, encerramento e os arquivos de `.logs/` |

## Ajuda

| Página | Leia quando… |
|---|---|
| [Solução de problemas](troubleshooting.md) | algo não funciona |
| [FAQ](faq.md) | você tem uma pergunta rápida |

## Projeto

| Página | Leia quando você quiser… |
|---|---|
| [Arquitetura](architecture.md) | ler ou mudar o código: crates, processos, sockets |
| [Contribuir](contributing.md) | montar um ambiente de desenvolvimento, rodar as verificações, enviar um pull request |
| [Publicar uma versão](release.md) | (mantenedores) criar e publicar uma versão |

As skills de agente que acompanham o Atlas ficam em [`skills/`](../../skills/atlas/SKILL.md); elas resumem estas docs para LLMs. A ordem da barra lateral do site é definida em [`_nav.json`](../_nav.json).
