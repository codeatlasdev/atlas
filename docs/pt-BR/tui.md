---
title: "TUI"
source: tui.md
source_sha: "caae09f3c15d4e1ef808d0f3ceb2e3b9aa4932c3"
translation: machine
---

# A TUI (`atlas dev`)

`atlas dev` abre um painel em tela cheia e inicia os seus serviços. Ele exige um terminal interativo (mínimo de 60×12; abaixo disso mostra "Terminal too small" com o tamanho atual). Saia com `q`: todos os serviços são parados.

## Layout

```text
┌ tabs ───────────────────────────────────────────────────────────┐
│ All  api  web  worker                                           │
├──────────────────────────────────────────┬──────────────────────┤
│                                          │ ● All systems operational
│  log viewer                              │ SERVICES             │
│  12:01:03 [api] listening on :3001       │  ● api   :3001   ⋯   │
│  12:01:04 [web] ready in 312ms           │  ✗ web   :3002   ⋯   │
│                                          │ OPERATIONS / MOBILE  │
│                                          │ EVENTS               │
├──────────────────────────────────────────┴──────────────────────┤
│ ? Help  q Quit  r Restart all  / Search  : Commands  f Following │
└─────────────────────────────────────────────────────────────────┘
```

- **Abas**: `All` (todos os serviços mesclados), mais uma por serviço e uma por [sessão](#sessões-e-anexar) aberta. Cada rótulo começa com o dígito de salto (`0` a `9`) e a aba de um serviço mostra um ponto de estado. Serviços com `hide_logs: true` não têm aba e ficam fora de `All`. As abas são clicáveis. Quando não cabem todas, a barra mostra só rótulos inteiros e `‹ +N` / `› +N` nas pontas (N = abas ocultas; clicável).
- **Visualizador de logs**: as linhas têm o formato `HH:MM:SS [service] text`, coloridas por serviço, com as cores ANSI preservadas e linhas longas com quebra forçada. Cada serviço guarda até 50.000 linhas em memória (limitado a 8 MiB por serviço, o que vier primeiro); o histórico completo fica em disco, em `.logs/`.
- **Barra lateral** (aparece quando o terminal tem pelo menos 90 colunas mais a largura da barra lateral; a barra lateral tem de 34 a 44 colunas, crescendo com o maior nome de serviço, então algo entre 124 e 134 colunas. Em terminais mais estreitos, e com `z` (zoom) em qualquer largura, os logs ocupam a largura toda e uma **faixa de status** de uma linha aparece acima do rodapé: primeiro o pior problema (`✗ api gave up after 3 restarts · 5/7 running`), clicável para ir até essa aba, depois as contagens de saúde e a operação em andamento, se houver):
  - resumo de saúde e indicador: `5/5 running · 1 done · 1 stopped`. One-shots concluídos e serviços que você parou, desativou ou que uma ação suspendeu ficam fora do denominador; o ponto pulsa só enquanto algo está falhando ou iniciando;
  - **SERVICES**: ícone de estado, nome, porta e uma dica `⋯` (abra o menu de contexto com `m`). Uma segunda linha `↳ …` mostra notas do agendador: `waiting for migrate (completed_successfully)`, `restarting in 4s (2/5)`, `waiting for :3000 (12s)`, `gave up after 5 restarts`, `done`, `restarted ×2`. Um serviço com algo a dizer sobre [memória](configuration.md) (acima do orçamento, suspeita de vazamento ou tendência de alta) ganha mais uma linha `↳` por fato: `memory 2.1 GB · rising +42 MB/min`, `budget warn`, `leak suspected: +600 MB over 12 reloads`; um serviço estável e dentro do orçamento não ganha nenhuma. Acima de `memory_warn` (ou de `max_memory` com `on_memory_max: notify`), o ícone é `◐` (degradado) e a nota traz o motivo. Linhas com falha mostram também `exit <code>`, e serviços que reiniciaram mostram `↻<n>`;
  - **MOBILE**: fase de build ao vivo das sessões mobile ativas;
  - **OPERATIONS**: a ação em execução que não é sessão, com o tempo decorrido e a última linha de saída;
  - **`⚠ ISSUES`**: serviços `critical` com falha, com a última linha do log e o `critical_help`;
  - **TUNNEL** (só quando há um `tunnel` configurado): ícone de estado, `ssh :<local_port> → <ssh_host>` e uma segunda linha `→ <remote_host>:<remote_port>` (`off (opens on demand)` até ser iniciado), e a caixa de seleção **Kill tunnel on exit**. A saída do ssh tem a própria aba de log `tunnel`;
  - **EVENTS**: os últimos 50 eventos relevantes, do mais novo para o mais antigo, com um ícone e uma cor por tipo: `✗` falha (com o código de saída), `✓` concluído, `●` iniciado, `○` parado, `⏸ web paused by Reset DB`, `⧗` aguardando, `↻` reinício, `⚠` alerta crítico. A coluna de horário aparece só quando o minuto muda.

  A barra lateral dá a SERVICES, OPERATIONS e ISSUES a altura completa primeiro; EVENTS fica com o que sobrar, então o `critical_help` nunca é espremido para fora.
- **Rodapé**: dicas geradas da mesma tabela de teclas do `?` e filtradas pelo contexto: só aparecem as teclas que fazem algo ali (um serviço parado oferece `r Start`, não `s Stop`), com `/ Search` e `m Menu`. `f` mostra `Following` (verde) ou `Paused` (âmbar). Enquanto você digita uma pesquisa, as dicas viram `Enter keep · Esc clear · Ctrl+R regex · Tab find/filter`. Quando há uma operação em andamento, as dicas são removidas da direita para que nada se sobreponha; só as dicas desenhadas são clicáveis. Uma barra `ATTACHED` substitui as dicas enquanto você está anexado a uma sessão.
- A **linha da marca** e a tela de abertura mostram o `name` do projeto, para distinguir duas instâncias.
- Os **toasts** entram deslizando pela borda direita e somem após 3 segundos ("Restarting api", "Logs copied (3min)"). Toasts de progresso mostram um spinner; fatos (`Tunnel will close on exit`) mostram um `ℹ` fixo.
- **Modal de atualização**: quando a verificação em segundo plano encontra uma versão mais nova, um modal pergunta `Atlas v<new> available — update when you quit?` (**Enter** = sim, **Esc** = agora não). Ele aparece uma vez por versão por sessão, depois da tela de abertura, e espera qualquer outro modal aberto fechar. Aceitar coloca `⬆ on exit` na linha da marca; depois que a TUI sai, os serviços são parados e o terminal é restaurado, o Atlas roda a atualização e mostra o resultado. Anexado a um runner, o runner continua na versão antiga até `atlas down && atlas dev --detach`. Recusar deixa a marca `⬆` simples e o banner exibido depois que você sai. Veja [CLI, aviso de atualização](cli.md).

Ícones de estado dos serviços: `●` rodando, `◎`/spinner iniciando, `✗` com falha, `○` parado, `⊘` desativado (parado por você; um serviço deixado fora da execução por `atlas dev SERVICE…` / `--profile` também mostra um `off` esmaecido depois do nome), `⏸` suspenso (pausado por uma ação).

Na inicialização, uma tela de abertura lista cada serviço ao vivo (alinhada à esquerda em um bloco centralizado: `✓` rodando, `✗` com falha, spinner iniciando, `○` parado) até que todos os serviços do app tenham sido enfileirados para iniciar (ou até passar 1,5 s, o que vier primeiro).

## Atalhos de teclado

Teclas do painel (a aba **All** e as abas de serviço). `?` mostra a mesma lista no app (gerada da tabela de teclas em `tui/keymap.rs`, em duas colunas a partir de 100 colunas, rolável com `j`/`k`/`PgUp`/`PgDn` quando não cabe), mais as suas `open_key`s e uma seção **PROJECT ACTIONS**. Ações com `group:` aparecem sob um cabeçalho por grupo, depois das sem grupo; os grupos aparecem na ordem do primeiro uso.

### Geral

| Tecla | Ação |
|---|---|
| `?` | Ajuda (feche com `Esc` ou `?`) |
| `q` | Diálogo de saída (veja [abaixo](#sair)); anexado a um runner: Detach / Stop everything |
| `D` | Só [anexado](#modo-anexado): desanexa, e os serviços continuam rodando. Em uma TUI que é dona dos próprios serviços, mostra a dica `run atlas dev --detach to keep services running in the background` |
| `Ctrl+C` | Sai na hora, sem diálogo, de qualquer camada (também da paleta e da barra de pesquisa; nunca é digitado como texto) |
| `:` | [Paleta de comandos](#paleta-de-comandos) |
| `Ctrl+Z` | Suspende o Atlas e volta ao shell (`fg` retoma; os modos do terminal são restaurados) |
| `Ctrl+U` / `Ctrl+W` | Na paleta: limpa a consulta / apaga a última palavra. Outros atalhos com Ctrl/Alt são ignorados em campos de texto |
| `z` | Zoom: oculta a barra lateral em qualquer largura (alterna; o rodapé mostra `Zoom on`). Lembrado por projeto |
| `r` | Em **All**: pergunta `Restart N services? Completed oneshots will not re-run.` e depois reinicia todos os serviços que não são um one-shot concluído. Na aba de um serviço: reinicia esse serviço (inicia, se estiver parado) |
| `t` | Alterna "kill tunnel on exit"; só vale quando há um `tunnel:` configurado |

### Abas

| Tecla / gesto | Ação |
|---|---|
| `Tab` / `Shift+Tab` | Próxima / aba anterior |
| `0` a `9` | Vai para a aba pelo índice (`0` = All), se ela existir |
| **clique do mouse: chip da aba** | Muda para essa aba |
| **clique do mouse: `‹` / `›`** | Rola a barra de abas para a esquerda / direita quando há abas demais para caber |
| **clique do mouse: linha de serviço na barra lateral** | Muda para a aba de log desse serviço |
| **clique do mouse: `⋯` na barra lateral** | Abre o menu de contexto do serviço |
| **clique do mouse: dica de tecla no rodapé** | Equivale a pressionar essa tecla (só onde a tecla está ativa) |
| **clique do mouse: barra de pesquisa** | Inicia uma pesquisa nos logs (igual a `/`) |
| **clique do mouse: item da paleta de comandos** | Seleciona e executa (igual a Enter) |
| **clique do mouse: item do seletor de dispositivo** | Seleciona e confirma (igual a Enter) |
| **clique do mouse: botões do modal de saída** | Cancel ou Quit (anexado: Detach ou Stop everything) |
| **clique do mouse: botões do modal de confirmação** | Cancel (Esc) ou Accept (Enter) |
| **clique do mouse: item do menu do serviço** | Executa essa ação |
| **clique do mouse: modal de ajuda** | Fecha (igual a `?` / Esc) |
| **clique do mouse: toast** | Dispensa o toast |
| **clique do mouse: fundo do modal** | Fecha o modal (igual a Esc) |
| **roda do mouse: logs** | Rola 3 linhas |
| **roda do mouse: paleta de comandos** | Move a seleção para cima / para baixo |
| **roda do mouse: seletor de dispositivo** | Move a seleção para cima / para baixo |
| **barra de rolagem do log: clique** | Salta para essa posição no log |
| **barra de rolagem do log: arrastar** | Rolagem contínua |
| `M` | Alterna a captura do mouse. Desligada, o terminal seleciona texto de forma nativa; com ela ligada, segure `⌥` (Option) ao arrastar. |

### Rolagem

| Tecla | Ação |
|---|---|
| `j` / `k`, `↓` / `↑` | Rola uma linha |
| `PgUp` / `PgDn` | Rola uma página |
| `g` / `Home` | Linha mais antiga |
| `G` / `End` | Linha mais nova, e liga o follow de novo |
| `f` | Liga / desliga o follow |

### Serviços

| Tecla | Ação |
|---|---|
| `r` | Na aba de um serviço: reinicia (inicia, se estiver parado). Em **All**: reinicia tudo, perguntando antes |
| `s` | Para este serviço (ele passa a `disabled`: sem health checks, nunca reiniciado automaticamente) |
| `S` | Reinicia este serviço (só nas abas de serviço; em **All** não faz nada, use `r`) |
| `m` | [Menu do serviço](#menu-do-serviço) |
| `t` | Mantém o túnel SSH aberto na saída (só com um bloco `tunnel:`) |
| `i` | Anexa a uma [sessão](#sessões-e-anexar) |

### Copiar e limpar

As teclas de cópia agem na aba atual (tudo em **All**) e usam a área de transferência do sistema; um toast confirma.

| Tecla | Ação |
|---|---|
| `L` | Copia os logs |
| `P` | Copia os logs como um prompt para um agente de IA |
| `E` | Copia só os erros, agrupados por serviço |
| `Ctrl+L` | Limpa o buffer de logs em memória de **todos** os serviços, só nesta TUI (os arquivos em `.logs/` não são alterados) |

## Visualizador de logs

- **Follow**: linhas novas rolam automaticamente enquanto o follow está ligado. Rolar para cima o desliga, e a visualização fica parada enquanto chegam linhas novas; chegar ao fim (`G`, ou rolar para baixo até lá) o liga de novo. Com o follow pausado, a última linha do log mostra um chip clicável `↓ N new · G` (N conta as linhas que chegaram na visualização atual desde a pausa; clicar ou usar `G` salta para o fim). Trocar de aba, pesquisar e mudar o filtro de nível também saltam para o fim e ligam o follow.
- **Quebra de linha e rolagem** funcionam sobre as linhas da tela já quebradas, então o que você rola é o que você vê. As linhas de continuação ficam alinhadas sob a coluna da mensagem (depois do prefixo de horário / `[service]`) e aparecem esmaecidas.
- **Abas de um só serviço** omitem o prefixo `[service]` (nas outras ele está em toda linha); **All** o mantém. As cores dos prefixos evitam âmbar e vermelho, para nunca serem confundidas com linhas de warn / error.
- As teclas de **cópia** usam a aba atual (tudo em **All**) e a área de transferência do sistema; um toast confirma o resultado. `E` (erros) agrupa as linhas sob um cabeçalho `── service ──` por serviço.
- **Buffers**: cada serviço guarda o próprio buffer circular (últimas 50.000 linhas **e** no máximo 8 MiB), então um serviço muito falante não empurra para fora o histórico de outro. **All** mescla os buffers na ordem de chegada. A memória continua limitada, por mais que um serviço imprima.
- **Leitura da saída** monta linhas de verdade a partir do pipe bruto: uma linha parcial (um prompt, `Password: `) aparece após 150 ms de silêncio, `\r\n` é uma quebra de linha, e um `\r` sozinho volta ao início da linha, de modo que uma barra de progresso (`10%\r20%\r…`) mostra só o último estado. UTF-8 incorreto é substituído por `�`. Uma linha única maior que 16 KiB é cortada e termina com ` … [truncated]`. O nível, o horário e um número de sequência monotônico são calculados uma vez por linha, quando ela chega, nunca por quadro.
- **Separadores de reinício**: quando um serviço inicia de novo depois de ter impresso saída, uma linha esmaecida `──── HH:MM:SS restarted ────` separa as execuções (ela nunca entra na pesquisa, na contagem de saída nem na contagem de erros).
- **Desempenho**: a visualização é um índice incremental sobre o buffer. Uma linha nova é classificada, filtrada e pesquisada uma só vez; um quadro ou uma tecla custa O(linhas visíveis), não O(buffer). As alturas das linhas quebradas ficam em cache por largura/layout. Benchmark de release (1.000.000 de linhas por 8 serviços): ingestão de 4 M linhas/s, um find com regex sobre as ~437 mil linhas retidas em 45 ms, adicionar 10 mil linhas a um find ativo em 0,6 ms (`cargo test -p atlas-tui --release -- --ignored --nocapture bench_one_million`).
- **Cores**: SGR negrito / esmaecido / itálico / sublinhado (simples, duplo, estilos `4:3` e a cor de sublinhado `58`) / tachado / reverso, e primeiro plano e fundo em 16 / 256 / truecolor (nas formas com `;` e com `:`) seguem a paleta do tema. Hyperlinks OSC 8 são respeitados.

### Alternâncias de visualização

| Tecla | Ação |
|---|---|
| `W` | Liga/desliga a quebra de linha. Desligada, trunca cada linha com `→`; role na horizontal com `←`/`→` (o chip no canto superior direito mostra `← +N`). |
| `T` | Liga/desliga os timestamps. |
| `J` | Liga/desliga a compactação de JSON. Uma linha que é exatamente um objeto JSON aparece como `LEVEL message key=value …`, colorida pelo nível (níveis numéricos do pino / bunyan `30`/`40`/`50` e níveis em texto `info`, `warn`, `error`, `fatal`, …). Desligada, mostra a linha bruta. Find e filter pesquisam o texto renderizado. |
| `-` | Insere uma linha `──── HH:MM:SS mark ────` no fluxo da aba atual (e no `.logs/<name>.log` desse serviço). Em **All**, a marca vai só para o fluxo `atlas` em memória. |
| `z` | Zoom: oculta a barra lateral e dá aos logs a largura toda. |

As alternâncias diferentes do padrão aparecem na linha de pesquisa/status (`wrap off · no ts · json`). `W`, `T` e `J` persistem entre execuções em `.logs/.atlas-ui.json`.

### Pesquisar e filtrar

- `/` abre a barra de pesquisa na parte de baixo e inicia o modo **find**: todas as linhas continuam visíveis, todas as ocorrências na visualização ficam destacadas e a atual fica mais forte. A barra mostra `3/41` (ocorrência atual / total na aba atual; o total cobre o buffer inteiro, não só a tela). `n` / `N` saltam para a próxima / anterior (voltando ao início ao chegar no fim, rolando a ocorrência para a vista e pausando o follow); só ficam ativas enquanto existe uma consulta de find, então nunca ocupam o lugar de `open_key`/teclas de ação.
- `Tab` dentro da barra alterna entre **find** e **filter**. Filter mostra só as linhas que combinam (o comportamento anterior); a barra mostra o número de linhas mantidas.
- `Ctrl+R` alterna **regex** (sintaxe `regex` do Rust; um padrão incorreto mostra `invalid regex` e não encontra nada). A comparação é **smartcase**: uma consulta toda em minúsculas ignora maiúsculas e minúsculas, e qualquer letra maiúscula a torna sensível a elas.
- `Enter` mantém a consulta e volta às teclas normais (`n`/`N` continuam funcionando); `Esc` a limpa. Sem ocorrências, o painel mostra centralizado `0 matches in <tab>`.
- `v` define o nível mínimo exibido. Os níveis são inferidos do texto: linhas com `error`, `err `, `fatal`, `panic` ou ANSI vermelho são *error*; `warn` ou amarelo são *warn*; `debug`, `trace` ou cinza são *debug*; o resto é *info*. `warn+` mostra warn e error; `error` mostra só erros. Um nível ativo aparece como um selo ao lado da barra de pesquisa.
- `V` define o **intervalo de tempo** da visualização, combinado com a aba, o nível, o find e o filter. Ele percorre `last 1m → 5m → 15m → 1h → since restart → since start`, depois abre um pequeno prompt para um intervalo **personalizado** (horários de hoje: `14:05` para "a partir de", `14:05–14:20` para "entre", com os dois extremos incluídos; `-`, `–`, `—` ou `..` separam; um horário fora do formato é explicado no prompt e nada muda) e então volta para todo o período. `Enter` aplica, `Esc` volta para todo o período. O intervalo ativo aparece como um selo `time: …` ao lado do selo de nível, e o `V Time` do rodapé é clicável. As janelas `last N` são relativas e deslizam: são reavaliadas no máximo uma vez por segundo, e as linhas que saem da janela são descartadas do início da visualização sem reescanear o buffer, então uma janela parada se esvazia sozinha. `since restart` começa no separador `restarted` mais novo do serviço, na aba de um serviço (de qualquer serviço em All; sem nenhum ainda, é o buffer inteiro); `since start` começa na linha retida mais antiga. Mudar o intervalo refiltra a visualização uma vez e, como o filtro de nível, salta para a linha mais nova. O intervalo não é lembrado entre execuções (uma janela relativa estaria desatualizada na próxima inicialização).

### Links, saltos para erros e abrir

- Os **links** em uma linha ficam sublinhados: URLs (`http(s)://`, `file://`, hyperlinks OSC 8) e referências `path:line[:col]` de compiladores, stack traces e saída de testes (`src/main.rs:12:5`, `/app/a.ts:30:9`, `util.py:10`). Números como `12:30:45` ou `localhost:3000` não são links.
- `}` / `{` movem o cursor de link para o próximo / anterior (começando pela linha visível mais nova; o link com foco fica destacado e rolado para a vista). `o` o abre: uma URL no navegador do sistema; um caminho no `$VISUAL` / `$EDITOR` quando for um editor gráfico (`code`, `cursor`, `zed`, `subl` e os launchers da JetBrains recebem suas flags `file:line`), senão `code` / `zed` / `cursor` do `PATH`, resolvido a partir da raiz do projeto. Um editor de terminal (`vim`) nunca é iniciado dentro da TUI; quando nenhum editor se aplica, ou o arquivo não existe, o `path:line` é copiado. `y` copia o link com foco.
  Nada bloqueia o loop de eventos: os launchers rodam desvinculados e são recolhidos em segundo plano.
- `]e` / `[e` saltam para a próxima / anterior linha de **erro**, e `]w` / `[w` para a próxima / anterior linha de **warning ou erro**, voltando ao início da visualização atual (aba + filtro de nível). O primeiro salto parte do topo da área visível; os saltos seguintes continuam do último destino. O salto pausa o follow para a linha continuar na tela; `No error lines` aparece em um toast quando a visualização não tem nenhuma.

### Indicadores nas abas

Uma aba sem foco mostra `◆` quando tem saída que você ainda não viu e um `✗N` vermelho com o número de linhas de erro desde a última vez que você a viu. Abrir a aba limpa os dois. Clicar no indicador `✗N` abre a aba com o filtro de nível em `error`.

## Paleta de comandos

`:` abre uma lista de comandos com busca aproximada. A consulta é uma correspondência de **subsequência**, sem diferenciar maiúsculas de minúsculas (`rsa` encontra `Restart api`); os caracteres que combinam ficam destacados, e correspondências no nome ficam acima das que só aparecem na descrição. `↑`/`↓`/`PgUp`/`PgDn`/`Ctrl+N`/`Ctrl+P` selecionam, `Enter` executa, `Esc` fecha, `Ctrl+U`/`Ctrl+W` editam a consulta. A lista rola para manter a seleção visível; o separador mostra `↑ n/total ↓` e cada linha mostra a sua tecla à direita.

Ordem com a consulta vazia: **comandos recentes primeiro** (salvos em memória e em `.logs/.atlas-ui.json`), depois as [ações](configuration.md) do projeto (primeiro as sem grupo, depois por grupo com um prefixo esmaecido `Group ›`; pesquisar `android` encontra todas as ações do grupo `Android`) e as entradas `Open <service>`, depois as nativas (Restart All, Toggle Zoom, Help, Scroll to Bottom/Top, Copy Logs / for AI / Errors, Quit) e por fim as entradas geradas por serviço. As entradas por serviço seguem as regras do menu do serviço: `Restart`/`Stop` com o serviço rodando ou iniciando, `Restart` para um serviço com falha, `Start` para os parados e suspensos (serviços que você mesmo parou não recebem nenhuma; serviços deixados `off` por `atlas dev SERVICE…` / `--profile` recebem `Start`). Colar texto funciona na paleta.

Os modais (paleta, ajuda, confirmação, menus) esmaecem o painel ao fundo em vez de escondê-lo.

## Menu do serviço

`m` na aba de um serviço abre um pequeno menu que só oferece o que faz sentido para o estado do serviço:

| Estado | Entradas |
|---|---|
| running, starting | `r` Restart, `s` Stop |
| failed | `r` Restart |
| stopped, disabled, suspended | `t` Start |

`1`, `2`, `3` são atalhos para reiniciar, parar e iniciar (quando essa entrada é oferecida). `Esc` cancela.

## Modais de confirmação

Uma ação com `confirm:` abre um modal com o rótulo da ação como título e o texto de confirmação. `Enter` ou `y` executa; `Esc` ou `n` cancela. Isso vale tanto ao pressionar a tecla da ação quanto ao escolhê-la na paleta. Quando a ação tem `pause_services`, o modal acrescenta uma linha `Pauses: web, api`.

`r` na aba **All** usa o mesmo modal (título "Restart all").

## Estado da interface lembrado

`.logs/.atlas-ui.json` guarda, por projeto: a aba ativa (pelo **nome**, então sobrevive a uma reordenação dos serviços), o zoom, as alternâncias de quebra de linha / timestamps / JSON, o filtro de nível e os comandos recentes da paleta. Ele é restaurado na inicialização. O intervalo de tempo (`V`) de propósito não é salvo. O arquivo é de melhor esforço: se estiver ausente ou ilegível, o Atlas inicia com os padrões, e uma gravação que falhar é ignorada.

## Sair

Anexado a um runner: veja o [modo anexado](#modo-anexado); sair é desanexar, a menos que você escolha **Stop everything**.

`q` abre "Quit? All services will be stopped." com **Cancel** (padrão) e **Quit**. `y` sai, `n`/`Esc` cancela, `Tab`/`←`/`→` alternam a seleção, `Enter` a confirma. `Ctrl+C`, SIGTERM e SIGHUP (fechar a janela do terminal) saem direto. Em todos os casos o Atlas para todos os serviços e sessões abertas antes de sair. Veja [Runtime, parada](runtime.md#encerramento).

## Modo anexado

`atlas dev --detach` inicia um **runner em segundo plano** (veja [Runtime, runner e attach](runtime.md#runner-e-attach)); rodar `atlas dev` sem a flag enquanto ele está ativo anexa a ele em vez de iniciar qualquer coisa. O painel é o mesmo (mesmos widgets, mesmas teclas, mesmo visualizador de logs): a linha da marca na barra lateral mostra `⛓ attached`, e o rodapé e o `?` listam `D Detach`. Estado, logs e eventos chegam por streaming do runner; ao anexar, você recebe o final recente do log dele (até 1000 linhas por serviço) e os últimos eventos, depois o fluxo ao vivo.

| Tecla | No modo anexado |
|---|---|
| `r` / `s` / `S` / `m`, paleta, `open_key`, ações do projeto | igual ao normal; o pedido é enviado ao runner (uma ação com `confirm:` ainda pergunta antes, no seu terminal) |
| `D` | **Detach**: a TUI sai, e o runner e todos os serviços continuam. Imprime `detached — services keep running · atlas dev to reattach · atlas down to stop` |
| `q` | Diálogo **Detach** (padrão) / **Stop everything**: `d` desanexa, `s` para o runner (como `atlas down`), `Tab`/`←`/`→` alternam, `Enter` confirma, `Esc`/`n` cancela |
| `Ctrl+C`, SIGTERM, SIGHUP | Desanexa. Fechar o terminal nunca para os serviços do runner |
| `t` | Explica que o runner é dono do túnel (ele é fechado quando o runner para) |
| `i`, ações de sessão, seletor de dispositivo mobile | **Só o dono**: um toast avisa. Sessões interativas precisam do terminal que as iniciou; para elas, use `atlas down` e um `atlas dev` simples |

Vários clientes podem ficar anexados ao mesmo tempo (`atlas dev --status` os conta) e veem os reinícios uns dos outros. Um cliente que se desconecta ou cai nunca afeta o runner. Se o runner morre ou é parado de outro lugar (`atlas down`), o cliente sai com `atlas runner stopped`. Se o `atlas.yaml` mudou desde que o runner iniciou e a lista de serviços é diferente, o attach é recusado com uma mensagem para rodar `atlas down` antes.

Uma TUI que é **dona** dos próprios serviços (`atlas dev` sem runner) não aceita attach: o diálogo de saída dela diz `keep them running: atlas dev --detach`. Passar os serviços de uma TUI ativa para um runner exigiria trocar o processo pai de processos em execução, então a forma de manter os serviços vivos é iniciar o runner primeiro.

## Sessões e anexar

Uma ação com `session: true` abre uma aba própria que exibe stdout/stderr ao vivo. Pressionar a tecla da ação de novo enquanto ela roda a encerra.

Em uma aba de sessão, o painel é somente leitura, exceto navegação, rolagem, `f`, `v`, `V`, `M`, `L`, `P`, `?`, `:` e:

| Tecla | Ação |
|---|---|
| `i` | **Attach**: todas as teclas são encaminhadas ao stdin da sessão (texto colado também) |
| `Ctrl+C` | Encerra a sessão |

Enquanto está anexado, o rodapé mostra uma barra `ATTACHED`. As teclas são enviadas cruas (`Enter` → nova linha, `Backspace`, `Esc`, `Tab`, setas como sequências de escape); os caracteres listados no `input_map` da ação são substituídos pela string mapeada. Duas teclas são reservadas:

| Tecla | Ação |
|---|---|
| `Ctrl+B` | Desanexa e volta ao painel (a sessão continua rodando) |
| `Ctrl+C` | Encerra a sessão e desanexa |

Quando o processo termina, a aba fecha e os `pause_services` são reiniciados.

## Sessões mobile

Uma ação com `platform: ios|android` abre primeiro um **seletor de dispositivo** (a menos que `device_id` fixe um): `↑`/`↓` ou `j`/`k` movem, `Enter` seleciona, `Esc`/`q` cancela. Enquanto os dispositivos são detectados, mostra um spinner; sem nenhum dispositivo, `Enter` inicia sem um. O iOS lista primeiro os simuladores iniciados, depois os disponíveis, mais os dispositivos do `xcrun devicectl`; o Android usa `adb devices -l`.

Depois de iniciada, o Atlas analisa a saída do build em fases (detecting → rustc → gradle/xcode → install → launch → running, ou failed), mostradas na seção **MOBILE** da barra lateral e no rodapé da aba da sessão.

A sessão recebe `TAURI_DEV_HOST` e `ATLAS_DEV_HOST` definidos a partir do [`host_mode`](configuration.md) da ação: loopback para simuladores/emuladores, o IPv4 de LAN da máquina para dispositivos físicos (`auto`) ou sempre (`lan`). Valores que você mesmo define são mantidos.
