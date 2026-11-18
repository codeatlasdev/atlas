---
title: "FAQ"
source: faq.md
source_sha: "102309b59ffd8391e4c3af8f36c8f2aaca1e2119"
translation: machine
---

# FAQ

## O que é o Atlas, em uma frase?

Um orquestrador de ambiente de desenvolvimento: um `atlas.yaml` inicia a infraestrutura e os serviços em ordem de dependência, supervisiona tudo e expõe o ambiente a uma TUI, a um app desktop e a agentes de IA.

## Qual é a diferença para `docker compose`, `foreman` ou `process-compose`?

O `docker compose` roda containers; o Atlas roda os seus *processos* (e chama `docker compose up -d` para a infraestrutura). Em relação aos runners de Procfile, ele acrescenta condições de dependência (`healthy`, `completed_successfully`, `log_ready`), backoff de reinício, sondas de saúde, pesquisa nos logs, validação da configuração, um servidor MCP e um app desktop. Ele não reinicia serviços ao detectar mudança em arquivos, não escala réplicas e (ainda) não roda no Windows.

## Ele roda no Linux ou no Windows?

O Atlas é para todo mundo, e estamos liberando plataforma por plataforma. Disponível agora: macOS em Apple silicon, Linux x86_64 e Windows 10/11 em x64 (a CLI, a TUI, o servidor MCP e o Atlas Desktop). No Linux você pode usar o script de instalação ou um [.deb, .rpm, pacote Arch ou AppImage](installation.md#pacotes-para-linux); no Windows, rode `irm https://atlas.codeatlas.com.br/install.ps1 | iex` no PowerShell ou use o `.exe` de instalação da página de releases ([o que muda no Windows](installation.md#windows)). Em breve: Macs Intel e Linux em ARM; você pode pedir um aviso na [página de download](https://atlas.codeatlas.com.br/download/). No Linux e no Windows, o Atlas Desktop desenha os terminais com xterm.js (o terminal nativo na GPU usa Metal, então só existe no macOS), e o runtime ainda é testado principalmente no macOS: [relate o que quebrar](https://github.com/codeatlasdev/atlas/issues/new).

## O Atlas envia meus dados para algum lugar?

Não há telemetria. A CLI fala com o GitHub para verificar e baixar versões, e o `gh` (se você usa os recursos de pull request no app desktop) fala com o GitHub em seu nome. Os hooks dos agentes enviam dados a um servidor de loopback dentro do app desktop, protegido por token e restrito a 127.0.0.1. O Atlas nunca envia nem lê os seus arquivos de variáveis de ambiente, a não ser para injetá-las nos seus próprios serviços.

## Onde estão meus logs?

Em `<project>/.logs/<service>.log`, sem ANSI. Veja [Logs](logs.md).

## Por que sair não roda `docker compose down`?

Parar e iniciar containers a cada sessão é lento, e em alguns Macs reiniciar o runtime de containers já o derrubou. Os containers de infraestrutura continuam rodando (containers com `restart: unless-stopped` sobrevivem entre sessões). Rode `docker compose down` quando quiser removê-los.

## Por que o Atlas não matou o processo que está na minha porta?

O Atlas só envia sinais para PIDs que ele mesmo iniciou (registrados em `.logs/.pids`) e para os descendentes deles. Matar "o que estiver escutando uma porta" pode atingir o encaminhador de portas de um runtime de containers ou uma ferramenta sem relação, então isso não é feito de propósito.

## Os serviços sobrevivem ao fechamento do meu terminal?

Sim: `atlas dev --detach` inicia um runner em segundo plano. `atlas dev` anexa uma TUI, `D` desanexa e `atlas down` para tudo.

## Duas worktrees podem rodar a mesma stack?

Sim, no Atlas Desktop. Cada worktree recebe um port offset (`ATLAS_PORT_OFFSET`, 100, 200, …) somado à porta de cada serviço, à porta de cada túnel e às URLs correspondentes. Teste: `ATLAS_PORT_OFFSET=100 atlas config check`.

## Quais agentes têm suporte?

O omp, o Claude Code e o Codex têm configuração com um comando (`atlas setup <agent> --apply`). Qualquer cliente MCP, como o Cursor, pode usar o `atlas mcp` com uma entrada manual. Veja [Configuração dos agentes](agents.md) e [MCP](mcp.md).

## Por que o `atlas dev` se recusa a rodar quando um agente o inicia?

Os hooks que o `atlas setup` instala impedem que os agentes abram a TUI interativa, o que travaria o shell deles. Em vez disso, eles apontam para `atlas dev --status` e para as ferramentas MCP. Para um agente, `atlas dev --detach` ou `--headless` funcionam.

## O `atlas.yaml` é compatível entre versões?

Dentro de uma versão major, sim. Os campos novos são aditivos; campos desconhecidos são erros, então gere o JSON Schema de novo depois de atualizar (`atlas config schema > .atlas.schema.json`). Veja o [changelog](https://github.com/codeatlasdev/atlas/blob/main/CHANGELOG.md).

## Posso usar meu próprio tema de terminal e meus atalhos no app desktop?

Sim. Configurações → Aparência mostra cada tema como um card (Atlas Dark e Atlas Light por padrão, com Dracula, Alucard e One), importa o seu tema do Ghostty, iTerm, VS Code, Alacritty ou Base16 (ou um zip ou uma pasta de temas seus) e dá ao terminal um tema próprio. Os temas usam o formato de tema do Zed (coloque os arquivos em `~/.atlas/themes/`; eles são recarregados quando você salva), e todo atalho é um comando nomeado que você pode reatribuir em Configurações → Atalhos de teclado ou em `keymap`, no `~/.atlas/desktop/settings.json`.

## Como contribuo?

Leia [Contribuir](contributing.md). Bugs e ideias vão para o [issue tracker](https://github.com/codeatlasdev/atlas/issues).
