---
title: "Logs"
source: logs.md
source_sha: "a7d23603033a98666e8f78e0abca485a307ba2e5"
translation: machine
---

# Logs

Cada linha que um serviço imprime é montada, classificada uma única vez, mantida em um buffer limitado na memória, gravada em disco e transmitida a quem estiver olhando: o [TUI](tui.md), o [Atlas Desktop](desktop.md) e os clientes [MCP](mcp.md).

## Do pipe à linha

- As linhas reais são montadas a partir do pipe bruto. Uma linha parcial (um prompt como `Password: `) aparece depois de 150 ms de silêncio; `\r\n` é uma única quebra de linha; um `\r` isolado mantém só o último estado de uma barra de progresso; UTF-8 inválido é trocado por `�`; uma linha com mais de 16 KiB é cortada e termina com ` … [truncated]`.
- O nível (`error`, `warn`, `info`, `debug`), o horário e um número de sequência são calculados uma única vez, quando a linha chega.
- Quando um serviço inicia de novo depois de ter impresso saída, uma linha discreta `──── HH:MM:SS restarted ────` separa as execuções. Ela nunca entra na pesquisa nem na contagem de saída.

Os níveis são inferidos do texto: `error`, `err `, `fatal`, `panic` ou ANSI vermelho são *error*; `warn` ou amarelo são *warn*; `debug`, `trace` ou cinza são *debug*; o resto é *info*.

## Buffers

Cada serviço mantém o próprio anel na memória: as últimas 50.000 linhas e no máximo 8 MiB, o que vier primeiro. Um serviço muito falante não empurra para fora o histórico de outro. **Tudo** junta os anéis na ordem de chegada. O histórico completo está em disco.

## Em disco

`<project>/.logs/<service>.log` contém `HH:MM:SS [stdout|stderr] text` sem ANSI. A execução anterior é rotacionada para `<service>.prev.log` quando um serviço é iniciado de novo, e um log que passa de 50 MB rotaciona do mesmo jeito. A saída das ações vai para `.logs/action-<slug>.log`. A lista completa de arquivos está em [Runtime › Arquivos em `.logs/`](runtime.md#arquivos-em-logs).

## Ver os logs

| Onde | Como |
|---|---|
| TUI | `/` pesquisa, `Tab` alterna entre localizar e filtrar, `Ctrl+R` regex, `v` nível, `V` período, `]e` / `]w` saltam entre erros e avisos, `}` / `{` e `o` para links. Veja [TUI › Visualizador de logs](tui.md#visualizador-de-logs) |
| Desktop | A aba **Logs**: ⌘F busca com alternância de regex e de maiúsculas, chips de serviço com a contagem de erros, filtro de nível, intervalo de tempo, saltos com `e` / `w`, ⌘-clique em URLs e `path:line`. Veja [Desktop › Logs](desktop.md#painel-do-projeto) |
| MCP | `atlas_logs` (tail, `level`, `since_minutes`) e `atlas_logs_search` (substring ou `/regex/`); `service: "audit"` mostra quem reiniciou, parou ou iniciou o quê. Veja [MCP](mcp.md#atlas_logs) |
| Shell | `tail -f .logs/api.log`, ou `atlas dev --headless` para transmitir todos os serviços na saída padrão |

## Pesquisa

A busca e o filtro compartilham o mesmo comparador: uma consulta em minúsculas ignora a caixa, uma letra maiúscula passa a diferenciá-la (*smartcase*), e o regex usa a sintaxe do crate `regex` do Rust. A pesquisa do MCP sempre ignora a caixa e trata uma consulta entre barras (`/ECONNREFUSED|timeout/`) como regex.

## Logs JSON

No TUI, `J` liga a compactação de JSON: uma linha que é exatamente um objeto JSON aparece como `LEVEL message key=value …`, colorida pelo nível. Os níveis numéricos do Pino e do bunyan (`30`, `40`, `50`) e os níveis em texto são reconhecidos. A linha original não é alterada em disco.

## Rajadas

O runner nunca desconecta um cliente lento. Um cliente que fica para trás pula linhas e vê um marcador `N lines skipped`; a saída completa continua em `.logs/`. O app desktop recebe a saída em lotes de cerca de 16 ms. O custo de renderização depende das linhas visíveis, não do tamanho do buffer.

## Esconder serviços quietos

`hide_logs: true` em um serviço remove a aba dele e mantém as linhas fora de **Tudo**. O arquivo em `.logs/` continua sendo gravado. Use isso em bancos de dados e em outra infraestrutura muito falante.
