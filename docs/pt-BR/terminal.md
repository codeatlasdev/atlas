---
title: "Terminal GPU"
source: terminal.md
source_sha: "5ca8b905ec32d1793fde21b91501a9c5b4da7ec3"
translation: machine
---

# Terminal GPU

O Atlas Desktop desenha os terminais locais de forma nativa, em vez de usar o xterm.js (ativado por padrão; Configurações → Terminal → **Terminal rápido (GPU)**, salvo em `atlas.gpuTerminal`, `"0"` desativa; vale para os terminais abertos depois). Cobre os terminais das worktrees locais do hostd (macOS); os terminais das worktrees de hosts SSH continuam no xterm.js.

## Arquitetura

```
program ─pty─▶ atlas hostd ──pty_attach_raw (raw bytes)──▶ reader task ─▶ Vt (alacritty_terminal + atlas scanner)
                                                                                  │ snapshot (Frame)
                                                    native view over the pane ◀── render thread (wgpu / Metal)
```

- **`crates/atlas-term`**: sem Tauri. `vt.rs` envolve o `alacritty_terminal` (o parser e a grade que o Alacritty e o Zed também usam) atrás de um único lock; `render.rs` desenha um snapshot `Frame` com pipelines instanciados (fundos das células, imagens abaixo do texto, glifos de um atlas swash, imagens acima do texto, decorações, cursor, barra de rolagem); `graphics.rs` decodifica imagens inline; `input.rs` codifica as teclas.
- **`crates/atlas-desktop/src/term.rs`**: uma `NSView` por aba sobre o retângulo do painel, uma task de leitura por terminal e uma thread de renderização que dorme até chegarem novos bytes, um redimensionamento, o piscar do cursor ou o flash do bell.
- **Recuperação** (`term/watchdog.rs`, `render_loop.rs`, `system.rs`): um frame devido a uma aba (bytes, entrada, redimensionamento, retomada) e que o `render` não apresentou (sem textura após a suspensão ou uma troca de monitor) é desenhado de novo a cada 250 ms, sem esperar novos bytes; após 2 s a view da aba e o `Renderer` são substituídos (a tela e o scrollback ficam no `Vt` e sobrevivem), no máximo uma vez a cada 2 s. A mesma reconstrução roda para todos os terminais em `NSWorkspaceDidWakeNotification` e `NSApplicationDidChangeScreenParametersNotification` (tampa, suspensão, conexão de um monitor), e manualmente por **Recarregar terminal** (⌘⇧P, ou o menu `+` de uma aba de terminal). Uma aba cuja thread de renderização terminou responde `term_bounds` com `terminal view dropped` e a página a abre de novo. Cada reconstrução é um incidente (`terminal-rebuild`, `terminal-recovered`, `terminal-system-event`, `terminal-dropped`) no relatório de diagnóstico. Uma view estacionada (aba em segundo plano, diálogo aberto) não é desenhada a cada rajada de bytes: ela desenha uma vez quando é exibida e nunca conta como travada.
- **Locks**: o registro `TERMS` nunca é tomado em uma thread de renderização. Comandos, o leitor e os observadores do sistema o seguram só o tempo de clonar o `Arc<Shared>` de um terminal; uma thread de renderização toca apenas o próprio `Shared` e os hooks que recebeu. Um renderer que bloqueia (espera de vsync, uma layer travada) trava, portanto, só a própria aba.
- **Caminho raw do hostd**: um socket por terminal que leva os bytes do pty sem JSON nem base64. É limitado: quando o terminal fica para trás, o programa bloqueia no próprio pty, como em um terminal nativo, e a memória do hostd fica estável. Um leitor que não aceita nada por 2 s é descartado, para que um cliente travado não congele o programa.
- **Versão do host**: o stream raw exige a capability `pty_attach_raw` (`info.caps`). O app atualiza um daemon mais antigo ao vivo na conexão (veja [Desktop](desktop.md): os terminais continuam rodando); um daemon de protocolo 1 com terminais rodando não pode ser substituído, então esses terminais usam o xterm.js até ele reiniciar, e as Configurações mostram um aviso.

## Recursos disponíveis

| Área | Detalhes |
|---|---|
| Texto | `terminal.font_family` (fontes de sistema instaladas, vale a primeira família instalada da lista, fontes variáveis incluídas; nenhuma instalada = JetBrains Mono embutida) + fallback de símbolos Nerd Font, `terminal.font_thicken`, negrito e itálico sintéticos, box drawing, contraste mínimo, paleta ANSI do tema |
| Decorações | sublinhado, duplo, ondulado, pontilhado, tracejado, colorido (SGR 58), tachado |
| Interação | seleção (caractere, palavra e URL/caminho inteiro com duplo clique, linha com triplo clique, ⇧-clique estende, bloco com ⌥-arrastar, rolagem automática nas bordas, ⌘A seleciona tudo; veja [Seleção e links](#seleção-e-links)), scrollback (10.000 linhas, no máximo `terminal.scrollback_bytes`) com barra de rolagem, copiar ao selecionar (`terminal.copy_on_select`, ativado) e ⌘C, colar (bracketed), soltar um arquivo cola o caminho entre aspas, cópia por OSC 52 (programa → área de transferência: perguntar por padrão, `terminal.osc52` allow/ask/deny; ler a área de transferência é negado), clique na linha do prompt move o cursor (`terminal.click_to_move`), a roda do mouse na tela alternativa rola o programa com as setas, terminais encerrados não aceitam entrada |
| Pesquisa | ⌘F abre uma barra de pesquisa acima do terminal: literal ou regex (alternável), diferenciar maiúsculas de minúsculas (alternável), Enter / ⇧Enter próxima / anterior com retorno ao início, Esc fecha. Toda ocorrência no scrollback e na tela é destacada (âmbar) e a atual fica em destaque forte (laranja); a viewport rola até ela e a barra mostra `n/total`. As ocorrências são recalculadas conforme a saída chega |
| Links | Hiperlinks OSC 8 e URLs `http(s)://` / `file://` simples (também entre linhas com soft wrap; a pontuação final é cortada). Segure ⌘ sobre uma URL, um link OSC 8 ou um caminho de arquivo existente para sublinhá-lo e ver a mão de ponteiro; ⌘-clique abre um link onde **Abrir links no** indicar (Configurações → Integrações → Shell e links → Links: aba do _Navegador do Atlas_ ou _Navegador do sistema_; `openPreview`) e um caminho no editor, na linha dele |
| Configurações ao vivo | Mudanças de tema recolorem os terminais abertos (`term_recolor`: fg, bg, cursor, texto do cursor, bg/fg da seleção, ANSI 16, contraste mínimo). `terminal.font_size`, `terminal.font_family` e `terminal.font_thicken` (o renderer reconstrói a fonte e os atlas, depois a grade e o pty são reajustados), `terminal.max_scroll_history_lines`, forma e piscar do cursor, `terminal.osc52`, `terminal.ligatures` e `terminal.minimum_contrast` (razão WCAG, 1 = desligado) valem para os terminais abertos (`term_set_prefs`). `terminal.padding_x` / `terminal.padding_y` movem o retângulo da view |
| Clusters de grafemas | Os caracteres de largura zero de uma célula (marcas combinantes, VS16, ZWJ, modificadores de tom de pele) viajam com o caractere base no snapshot (`GlyphCell::extra`), com `wide` para células de largura dupla, de modo que o renderer desenha o cluster inteiro. O DECRQM `?2027` informa "always set" (sempre ativo) |
| Emoji colorido | Atlas RGBA alimentado pelo Apple Color Emoji; células largas ajustadas a 2 células; clusters ZWJ, de bandeira, de tom de pele, keycap e VS16 montados em um único glifo (swash) |
| Ligaduras e sprites | Ligaduras `calt`/`liga` para o JetBrains Mono, ativadas por padrão (`terminal.ligatures`; as sequências são células adjacentes de mesmo estilo, interrompidas por lacunas, sprites, símbolos e emoji; `Renderer::set_ligatures`). Sprites procedurais para as células se encaixarem: box drawing U+2500 a U+257F, block elements U+2580 a U+259F, braille U+2800 a U+28FF, sextantes U+1FB00 a U+1FB3B, powerline E0B0 a E0B7. Um cache de layout por linha (hash do conteúdo) faz um frame em cache custar ~90 µs; um reshape a frio de 144×44 leva ~0,25 ms em texto simples / ~0,4 ms com muito código (release; `cargo test -p atlas-term --release -- --ignored --nocapture screen_bench`) |
| Integração com o shell | Marcas de prompt OSC 133 (⌘↑ / ⌘↓ pulam entre prompts, código de saída, limites da saída do comando), cwd por OSC 7 (Reiniciar reabre ali), título OSC 0/2, notificações OSC 9 / 777, bell (flash + atenção no Dock quando sem foco) |
| Cursor | bloco, barra, sublinhado (DECSCUSR), piscando, vazado quando sem foco |
| Cores | Primeiro plano, fundo, cursor, texto do cursor, seleção e os ANSI 16 vêm do tema do terminal (o tema do app, ou o escolhido em Configurações → Aparência → **Tema do terminal**: `terminal.cursor`, `terminal.cursor_text`, `terminal.selection.*`, `terminal.ansi.*` do arquivo de tema). Um tema que omite uma cor recebe uma derivada da paleta |
| Fonte e padding | `terminal.font_family` é uma lista CSS (`"Fira Code", Menlo, monospace`): aspas e palavras-chave genéricas (`monospace`, `ui-monospace`, `system-ui`, `sans-serif`, `serif`) são descartadas e a primeira família instalada é usada. Quando a primeira não existe, o Atlas mostra um toast uma vez (`A fonte Fira Code não está instalada, então o terminal GPU usa JetBrains Mono.`, evento `term://font`). `terminal.padding_x` / `terminal.padding_y` (pt, 0 a 64, padrão 10 e 6) afastam o terminal das bordas do painel; a margem é pintada com o fundo do terminal (Ghostty `window-padding-color = extend`) |
| Teclado | Codificação xterm; flags 1 (disambiguate), 8 (all keys as escapes) e 16 (associated text) do protocolo Kitty keyboard. Só eventos de pressionar. Edição de linha do macOS e atalhos de terminal do Ghostty: veja [Teclado](#teclado) |
| Modos | saída sincronizada (2026), bracketed paste, application cursor, tela alternativa |
| Imagens | Kitty graphics: transmit/display/put/`delete`, RGB/RGBA/PNG, zlib, em blocos (`m=1`), mídia direta e de arquivo/arquivo temporário, números `I=`, `q=` quiet, `C=1`, `z` acima/abaixo do texto. Sixel com paletas HLS/RGB. `CSI 14 t` informa a área de texto em pixels. As imagens rolam com o texto; imagens da tela alternativa somem com ela |

## Teclado

Modelado nos padrões do Ghostty no macOS (`ghostty +list-keybinds --default`) e nas teclas de edição de linha do Terminal.app e do iTerm2. Funcionam com um terminal GPU em foco; os terminais SSH (xterm.js) mantêm o conjunto clássico.

**Edição de linha** (enviada ao programa):

| Teclas | Legado | Com protocolo Kitty | Faz |
|---|---|---|---|
| ⌘⌫ | `^U` | `^U` | apaga até o início da linha |
| ⌘← / ⌘→ | `^A` / `^E` | `^A` / `^E` | início / fim da linha |
| ⌥⌫ | `ESC DEL` | `CSI 127;3u` | apaga a palavra anterior |
| ⌥← / ⌥→ | `ESC b` / `ESC f` | `ESC b` / `ESC f` | palavra anterior / seguinte |
| `⌥Delete` | `ESC d` | `CSI 3;3~` | apaga a palavra seguinte |

⌘⌫, ⌘← ⌘→ e ⌥← ⌥→ são bindings, como no Ghostty (`super+backspace=text:\x15`, `alt+left=esc:b`, …): os mesmos bytes, qualquer que seja o protocolo de teclado que o programa ativou. Enviá-los como teclas Kitty (`CSI 127;9u`) chegava sem binding no fish 4, então ⌘⌫ não fazia nada. ⌥⌫ e `⌥Delete` são teclas: com o protocolo Kitty ativado, o programa recebe a tecla e os modificadores reais e faz o binding por conta própria. ⇧ ou ⌃ com essas teclas mantêm os parâmetros de modificador xterm simples.

**Ações do terminal** (tratadas pelo Atlas):

| Teclas | Faz | Ghostty |
|---|---|---|
| ⌘K | limpa a tela e o scrollback, depois envia `^L` para o shell redesenhar (não na tela alternativa) | `clear_screen` |
| ⌘A | seleciona tudo (scrollback e tela) | `select_all` |
| ⌘C / ⌘V | copia a seleção / cola (bracketed) | `copy_to_clipboard` / `paste_from_clipboard` |
| ⌘F | pesquisa no scrollback | `start_search` |
| ⌘↑ / ⌘↓, ⌘⇧↑ / ⌘⇧↓ | prompt anterior / seguinte (OSC 133) | `jump_to_prompt` |
| ⌘Home / ⌘End | rola até o topo / o fim | `scroll_to_top` / `scroll_to_bottom` |
| ⌘PgUp / ⌘PgDn, ⇧PgUp / ⇧PgDn | rola uma página (⇧: só na tela principal; na tela alternativa o programa recebe `CSI 5;2~`) | `scroll_page_up` / `scroll_page_down` |
| ⌘+ / ⌘= / ⌘− / ⌘0 | aumenta / diminui / restaura o tamanho da fonte, só neste terminal, ao vivo | `increase_font_size` / `decrease_font_size` / `reset_font_size` |
| ⌘D / ⌘⇧D | divide à direita / abaixo | `new_split` |
| ⌘G / ⌘⇧G | próxima / anterior ocorrência da barra de pesquisa aberta (abre a barra quando está fechada) | `navigate_search` |
| ⌘⇧Y | seleciona (e copia) a saída do último comando, de novo = a do anterior | nenhuma (Atlas) |

⌘K e ⌘⇧D seguem o Ghostty só enquanto um terminal tem o foco do teclado: em qualquer outro lugar ⌘K é a paleta Ir para worktree ou terminal (também acessível por ⌘⇧P e pelo menu Ir) e ⌘⇧D é Alterações. O menu nativo e o handler de teclas da janela (`terminalKeeps`) passam a tecla ao terminal enquanto um está em foco; antes da 1.5.0 o handler de teclas ainda enviava ⌘K à paleta Ir para worktree ou terminal.

**Configurações** (`settings.json`, Configurações → Terminal): `terminal.copy_on_select` (padrão true, Ghostty `copy-on-select`), `terminal.click_to_move` (true, `cursor-click-to-move`), `terminal.option_as_alt` (`both` | `left` | `right` | `none`, padrão `both`, `macos-option-as-alt`). Com ⌥ como Alt, ⌥B envia `ESC b`; com `none` digita o caractere do macOS (∫). ⌥ com setas, ⌫ e `Delete` é sempre Alt: essas teclas não têm caractere no macOS.

**Clicar para mover.** Um clique simples (sem arrastar, sem modificador) no prompt em edição envia setas que movem o cursor do shell até ali, como o `cursor-click-to-move` do Ghostty. Exige OSC 133 (o shell marca o início do prompt `A` e o início do comando `C`/fim `D`), então um comando em execução, a tela alternativa e uma viewport rolada para cima nunca recebem setas. Só contam as linhas do próprio prompt (da primeira linha até a linha do cursor).

**Roda do mouse e trackpad** (`crates/atlas-term/src/wheel.rs`, uma chamada `term_wheel` por evento da página; a página envia apenas linhas na altura da célula, o tipo de dispositivo, a célula sob o ponteiro e os modificadores). Algoritmo do Ghostty: um trackpad informa pixels, uma roda com cliques informa linhas que valem 3 linhas de pixels (`DISCRETE_ROWS`), um acumulador transforma os dois em linhas inteiras e guarda o resto abaixo de uma linha (uma mudança de direção o descarta). Para onde as linhas vão: na tela alternativa com DECSET 1007 e sem modo de mouse, setas Cima/Baixo (modo de cursor de aplicação respeitado, no máximo 50 por evento; `less`, `vim` e `htop` não têm scrollback); um programa que pediu relatórios de mouse recebe uma única escrita com todos os relatórios de roda do evento (⇧ devolve a roda ao Atlas; um relatório que a codificação não consegue representar é descartado, não vira rolagem); todo o resto rola o scrollback do próprio terminal. Um terminal somente leitura nunca escreve no programa. A **rolagem suave** vale só nesse último caso, estritamente dentro do scrollback: o resto é desenhado como um deslocamento de pixels abaixo de uma linha (`Frame.over` = uma linha de overscan acima e abaixo da viewport, `Frame.scroll_frac`, o uniform de origem da grade em `term.wgsl`), e `SETTLE` (110 ms) após o último evento o deslocamento se ajusta à linha mais próxima. No fim ao vivo e no topo não há deslocamento, e uma tecla ou uma saída nova no fim o descarta. `term_stats[tab].vt` (`display_offset`, `max_offset`, `mouse`, `alt_screen`, `alt_scroll`) mostra por que a roda fez o que fez.

**Cadência de frames e saída sincronizada.** Um terminal desenha no máximo uma vez por atualização do monitor (`render_loop::FRAME_INTERVAL`, piso de 60 Hz) e sempre o estado mais recente: a saída só marca a tela como suja; o primeiro desenho após um período ocioso é imediato. Uma atualização sincronizada (DECSET 2026) fica retida por 1 s, como o `sync_reset_ms` do Ghostty (`vt/sync.rs`; o do próprio vte é de 150 ms, menor que um frame de agente lento), e um redimensionamento a encerra.

**Relatórios de mouse.** Um programa que ativa DECSET 1000 / 1002 / 1003 (vim, tmux, htop, TUIs de agentes) é dono do mouse: pressionar, soltar, arrastar (1002/1003) e cliques da roda chegam a ele como relatórios X10, UTF-8 (1005) ou SGR (1006), com ⇧ / ⌥ / ⌃ nos bits do botão. ⇧ (Ghostty `mouse-shift-capture = false`) e ⌘ deixam o clique para o Atlas, então ⇧-arrastar ainda seleciona e ⌘-clique ainda abre links. Relatórios além do que a codificação legada consegue representar (coluna ou linha acima de 222) são descartados em vez de corrompidos. **SGR-Pixels (DECSET 1016)**, que o Claude Code sonda, é o formato SGR com o ponteiro em pixels do dispositivo a partir do canto superior esquerdo da grade (base 0, arredondado, sem limitar: um arrasto que saiu do terminal informa uma posição negativa), e tem precedência sobre 1006 e 1005, como no xterm e no Ghostty. Precisa de um modo de mouse como toda codificação, não tem limite de alcance, e um chamador que conhece só a célula recebe a célula. O alacritty ignora 1016, então `vt/pixel.rs` acompanha `?1016h/l` (também dentro de listas como `?1000;1016;1006h`, divididas entre leituras), responde DECRQM (`?1016$p` → `;1` ativo, `;2` desativado) e o descarta em DECSTR/RIS/`Vt::reset`. `term_mouse` aceita `px`/`py` opcionais (px do dispositivo) que a página envia. `crates/atlas-term/src/mouse.rs` (bytes), `crates/atlas-desktop/src/term/mouse.rs` (comandos), `terminal/pointer.ts` + `mouseRoute.ts` (roteamento).

**Notificações.** OSC 9 (`ESC ] 9 ; text`) e OSC 777 (`ESC ] 777 ; notify ; title ; body`) disparam uma notificação do desktop, como o Ghostty faz; use-os em um script ou hook de agente (`printf '\e]9;done\a'`) para ser avisado quando terminar. O aviso cai na caixa de notificações e no ponto de não lido da worktree, como um agente que termina, e mostra um banner do sistema quando o Atlas está em segundo plano ou outra aba está sendo exibida (Configurações → Notificações e sons → Notificações: **Mostrar notificações**, **Tocar um som**, **Ficar em silêncio enquanto estou olhando**). O progresso do ConEmu (`OSC 9;4;…`) é ignorado; os caracteres de controle são removidos e o corpo é limitado a 120 caracteres.

**⌘-clique em arquivos.** ⌘-clique em `path`, `path:line` ou `path:line:col` (também `path(line,col)`) na saída abre o arquivo no editor naquela posição quando ele está dentro da worktree. Um caminho relativo é resolvido a partir do diretório atual do terminal (OSC 7), `..` não pode sair da worktree, e todo o resto (URLs vão para o navegador, `~/…`, outros diretórios, `12:34:56`) não faz nada. O ⌘-hover sublinha o caminho só quando o arquivo existe (`termPathExists.ts`: um `fs_stat`, lembrado por 5 s, uma falha por 2 s), então `example.com` não acende; o clique só precisa do formato.

### Seleção e links

Como o ponteiro se comporta sobre um terminal GPU (`terminal/pointer.ts` decide; `atlas-term/src/vt/pick.rs` e `crates/atlas-desktop/src/term/select.rs` executam; o slot recebe eventos de mouse do DOM porque a view nativa deixa o clique atravessar).

| Gesto | Faz |
|---|---|
| arrastar | seleciona por células; um arrasto que sai da grade rola a viewport (1 linha por tick na borda, +1 por célula de distância, no máximo 12, a cada 32 ms, também enquanto o ponteiro fica parado fora) e continua selecionando; a soltura é detectada também fora do terminal |
| duplo clique | a palavra pelos separadores do alacritty, exceto uma URL ou um `path:line:col` (um `/` ou um `:line` no fim), que é tomado inteiro; um arrasto a partir dele cresce por células a partir da palavra |
| triplo clique | a linha, copiada sem a quebra de linha final (⌘A também) |
| ⇧-clique | move o fim da seleção, mantendo a âncora; sem seleção, inicia uma no clique; um arrasto depois dele continua estendendo |
| ⌥-arrastar | seleção retangular (em bloco) |
| ⌘-hover | sublinha o link sob o ponteiro (OSC 8, URL `http(s)://`/`file://` simples ou caminho de arquivo existente) e mostra a mão; acompanha o mouse, e também quando ⌘ é pressionado ou solto com o mouse parado |
| ⌘-clique | abre o link conforme **Abrir links no** (Configurações → Integrações → Shell e links), ou o arquivo na linha dele; fora disso é um clique simples |
| programa dono do mouse | pressionar, arrastar, soltar e a roda simples vão para o programa; ⇧ devolve a seleção ao Atlas (⌘ mantém os links) |

A seleção fica ancorada ao texto, não à tela: a saída em fluxo a move para o scrollback junto com o texto que ela cobre, e copiar ao selecionar (`terminal.copy_on_select`) copia quando o botão sobe. `term_pointer_state` (ponte de debug) retorna o tipo e as células da seleção, o link sublinhado e o deslocamento de rolagem: `atlas desktop debug eval 'return await window.__TAURI_INTERNALS__.invoke("term_pointer_state",{tab:"<tab id>"})'`. Não feito: um arrasto após um duplo clique em uma URL cresce por células, não por palavras; não há verificação de existência do arquivo para o ⌘-clique (só para o sublinhado).

**Selecionar a saída de um comando.** ⇧⌘Y seleciona a saída do último comando terminado (entre OSC 133 `C` e `D`, nunca o prompt nem a linha de comando) e a copia; pressionar de novo volta ao comando anterior. Um comando que não imprimiu nada é pulado, assim como um que ainda está rodando. Não funciona na tela alternativa.

**Menu Editar.** ⌘C e ⌘V vêm do menu Editar nativo: o AppKit envia `copy:` / `paste:`, e a web view dispara um evento `copy` / `paste` no campo de entrada do terminal, então cada um roda uma vez. ⌘Z e ⇧⌘Z não fazem nada em um terminal e nunca chegam ao shell.

### Colar

O evento `paste` da web view só expõe texto, então ⌘V é um gatilho e o host lê a área de transferência do macOS por conta própria (`term_paste_clipboard`, decidido por `atlas_term::input::paste_clipboard`). O que ela contém decide os bytes, com a precedência que o Ghostty usa (o Finder também coloca o nome de um arquivo e um bitmap de ícone gerado ao lado da URL dele, então arquivos vencem imagem e texto):

| Área de transferência | Enviado |
| --- | --- |
| Arquivos (⌘C no Finder) | os caminhos, com aspas de shell (`shlex.quote`), separados por espaço |
| Texto | o texto (bracketed quando o programa pediu; um `ESC[201~` colado é descartado) |
| Imagem, sem texto (ferramentas de captura, "Copy Image") | bracketed paste ativado: um bracketed paste vazio (`ESC[200~ ESC[201~`, o que o iTerm2 e o Terminal.app enviam; o omp lê a imagem por conta própria nesse caso). Desativado: Ctrl+V (`0x16`, ou `CSI 118;5u` sob o protocolo Kitty keyboard), a tecla que omp, Claude Code e Codex associam a "colar imagem" |
| Vazia | nada |

Um terminal não carrega um bitmap, então, para uma imagem, o programa lê a área de transferência por conta própria; o Atlas só manda que ele leia. Soltar um arquivo no terminal continua colando o caminho entre aspas. Terminais que usam xterm.js (hosts SSH, ou com Terminal rápido (GPU) desativado) colam só texto.

**Teclas da área de transferência em terminais xterm.js (Linux, hosts SSH).** Ctrl+Shift+C copia a seleção e Ctrl+Shift+V a cola (bracketed); Ctrl+C só copia enquanto há texto selecionado e, fora disso, continua sendo SIGINT. Com `terminal.copy_on_select` (ativado), selecionar também copia. A seleção PRIMARY (colar com o botão do meio) não é preenchida. O macOS mantém ⌘C/⌘V pelo menu Editar. Ctrl+Shift+V lê a área de transferência pela webview, então no Linux a janela do próprio app tem acesso a ela (as abas do navegador embutido nunca têm); se a leitura for negada mesmo assim, um aviso diz isso e Ctrl+V simples continua colando.

## Scrollback

Limitado em linhas e em bytes, por terminal. `terminal.max_scroll_history_lines` (padrão 10 000, 0 a 100.000) é o limite de linhas; `terminal.scrollback_bytes` (padrão 64 MiB, restrito ao intervalo de 1 MiB a 1 GiB, as Configurações mostram MB; o `scrollback-limit` do Ghostty tem padrão de 50 MB) é o limite de memória, contado em bytes de células (`Vt::history_bytes`). Uma linha custa `columns` células, então vale o mais restrito dos dois e um terminal muito largo mantém menos linhas que a configuração de linhas (`vt/history.rs::history_lines`). Os dois valem ao vivo: baixar um libera as linhas excedentes de imediato, e um redimensionamento recalcula o limite para a nova largura. `0` bytes nunca desativa o histórico (é restrito a 1 MiB); para isso, defina o limite de linhas como 0. ED3, RIS e um limite menor devolvem as linhas liberadas (teste `scrollback_memory`, um alocador contador).

## Identidade do terminal

Todo terminal que o hostd inicia tem `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=Atlas` e `TERM_PROGRAM_VERSION=<Atlas version>` (`crates/atlas-host/src/pty/spawn.rs`), do mesmo modo que o Ghostty define `TERM_PROGRAM=ghostty`. Os agentes (o Claude Code tem 35 referências a `TERM_PROGRAM`) escolhem por ele a amplificação da roda, os cliques em links e os gráficos. Quem as define é o hostd, não a herança: a sondagem do ambiente de login descarta `TERM_PROGRAM`, `TERM_PROGRAM_VERSION` e `TERM_SESSION_ID`, então o terminal que iniciou o Atlas (`bun run desktop:dev` a partir do Ghostty) nunca vaza a própria identidade, e um `env` explícito no `pty_create` ainda prevalece. Sessões por pipe (`omp --mode rpc-ui`, um protocolo JSONL, não um terminal) não recebem identidade. As abas xterm.js (hosts SSH, Linux) rodam o mesmo hostd, então recebem as mesmas variáveis.

## Comparação com o Ghostty

Auditoria contra os recursos documentados do Ghostty (ghostty.org/docs/config/reference e as ações de keybind). **presente** = se comporta como o Ghostty, **parcial** = parte do comportamento, **ausente** = não existe. A evidência é o código que faz isso; a prova de cada item é um teste no mesmo crate.

| Recurso do Ghostty | Situação | Evidência / o que falta |
|---|---|---|
| Ligaduras (`font-feature`) | presente | `font/layout.rs`, `terminal.ligatures`; só `calt`/`liga`, sem lista `font-feature` livre |
| `font-thicken` | presente | `terminal.font_thicken`, `render/mod.rs` |
| Fontes variáveis (`font-variation`) | parcial | `font/system.rs` controla `wght` / `ital` / `slnt` a partir de negrito / itálico; sem configuração por eixo |
| `font-family`, fallback | parcial | `font/system.rs`; símbolos Nerd e emoji têm fallback, sem fallback de sistema para CJK ou outros sistemas de escrita |
| Estilos de cursor, piscar, `cursor-style-blink` | presente | `vt/mod.rs::set_options`, DECSCUSR, vazado quando sem foco |
| `macos-option-as-alt` | presente | `termKeys.ts::optionActsAsAlt`, `terminal.option_as_alt` |
| Teclas do macOS ⌘K, ⌘←/→, ⌥←/→, ⌘⌫, ⌥⌫ | presente | `input.rs` (testes `macos_line_editing_map_*`); o ⌘K exigiu corrigir o handler de teclas da janela (`terminalKeeps`) |
| ⌘A / ⌘C / ⌘V, tamanho da fonte, ⌘Home/End, ⌘PgUp/PgDn | presente | `termKeys.ts`, `term/keys.rs` |
| `copy-on-select` | presente | `terminal.copy_on_select`, `term/select.rs` |
| Pesquisa no scrollback ⌘F, `navigate_search` ⌘G / ⇧⌘G | presente | `search.rs`, `TermSearchBar.svelte`; o ⌘G é novo |
| Seleção em `start_search`, `search_selection` | ausente | ⌘E é Config no Atlas |
| Integração com o shell: marcas de prompt OSC 133, `jump_to_prompt` | presente | `vt/shell.rs` |
| Selecionar a saída de um comando | presente (Atlas) | `vt/shell.rs::select_last_output`, ⇧⌘Y; o Ghostty não tem tecla padrão |
| cwd por OSC 7 no Restart | presente | `term://cwd`, `GpuTerminal.svelte` |
| cwd por OSC 7 em novas abas e divisões | parcial | `workspace.svelte.ts::newTerminal` sempre começa na raiz da worktree; uma divisão move uma aba existente, então nenhuma das duas herda o diretório do terminal em foco |
| Hiperlinks OSC 8, URLs simples, sublinhado e mão no ⌘-hover, ⌘-clique | presente | `vt/pick.rs::link_at`, `search.rs::osc8_at`, `pointer.ts` |
| ⌘-clique em caminhos de arquivo (`path:line:col`) | presente (Atlas) | `search.rs::token_hit`, `termPath.ts`, `termPathExists.ts` (sublinha só arquivos que existem) |
| Escrita por OSC 52 | presente | `terminal.osc52` allow / ask / deny; a leitura é negada |
| Protocolo Kitty keyboard | parcial | `input.rs`: flags 1, 8, 16, só eventos de pressionar; sem eventos de soltar (flag 2), sem teclas alternativas (4) |
| Kitty graphics, Sixel | parcial | `graphics/`; sem memória compartilhada, animação nem placeholders Unicode |
| Quick terminal | ausente | exige um atalho global e uma janela suspensa (`quick-terminal-*`) |
| Divisões, redimensionamento | parcial | painéis do Atlas (`pane::SplitRight/Down`, ⌘D a partir do terminal); sem as ações de navegação por divisão e de equalização do Ghostty |
| Relatórios de mouse (1000/1002/1003, 1005/1006/1016), `mouse-shift-capture` | parcial | `mouse.rs`, `vt/pixel.rs` (SGR-Pixels); faltam o movimento puro sem botão e os eventos de foco |
| Colar com o botão do meio, menu de contexto | ausente | `pointer.ts` trata o botão esquerdo para a seleção |
| Bell | presente | `VtEvent::Bell`: flash mais atenção no Dock |
| Notificações do desktop OSC 9 / OSC 777 | presente | `vt/shell.rs::notify`, `termNotice.ts`, `agents.svelte.ts` (novo); sem as configurações `desktop-notifications` / `bell-features`, valem as configurações de notificação do Atlas |
| Título (OSC 0/2) | presente | `pty_title` do hostd, rótulo de aba `Terminal` |
| Largura de Unicode e emoji, clusters de grafemas (modo 2027) | parcial | `vt/frame.rs`; a largura é por code point no `alacritty_terminal`, uma sequência ZWJ ocupa várias células |
| Desempenho com saída grande | presente | veja [Desempenho](#desempenho): `cat` 1 GiB 8,0 s contra 12,3 s |
| `minimum-contrast` | presente | `terminal.minimum_contrast`, `colors.rs` |
| `window-padding-x/y`, `window-padding-color = extend` | presente | `terminal.padding_x/y`; sem `window-padding-balance` |
| `scroll-to-bottom = keystroke` | presente | a saída deixa no lugar uma viewport rolada para cima (#97); uma tecla ou um colar volta ao fim (`term/shared.rs` `Shared::input`) |
| `mouse-hide-while-typing` | ausente | |
| Consultas de cor OSC 10/11/4 | presente | respondidas a partir da paleta do tema (`Vt::set_palette`); uma cor que o próprio programa definiu com OSC 4/10/11 não aparece na resposta |

## O que ainda falta

- Fonte de emoji só no macOS (Apple Color Emoji, carregada sob demanda de `/System/Library/Fonts`): sem fallback embutido nem de outros sistemas, então em outros sistemas os emoji caem na Nerd Font monocromática ou em nada. Sem fallback de sistema para CJK ou outros sistemas de escrita. Só o primeiro glifo de um cluster que a fonte de emoji não consegue juntar é desenhado.
- O `alacritty_terminal` mede a largura por code point, então uma sequência ZWJ que ocupa várias células largas na grade não tem a largura de uma célula, mesmo desenhada como um único glifo.
- Blocos de sombra U+2591 a U+2593 são alfa chapado (sem dither).
- A pesquisa se limita ao que o motor DFA do alacritty aceita: sem look-around, back-references nem `\b` Unicode (use `(?-u:\b)`); no máximo 20.000 ocorrências são mantidas.
- Meio de memória compartilhada do Kitty graphics (`t=s`), animação, placeholders Unicode, posicionamentos relativos; eventos de soltar tecla.
- Marcas de prompt e imagens são ancoradas a linhas do scrollback; quando o histórico de 10.000 linhas enche, elas se deslocam a cada linha descartada.
- Lacunas nos relatórios de mouse: o movimento só é informado com um botão pressionado, então um programa DECSET 1003 (efeitos de hover) recebe arrastos, mas não movimento puro; sem eventos de foco (DECSET 1004); colar com o botão do meio não está implementado.
- Bindings do Ghostty não trazidos: navegação de abas/janelas/divisões além de ⌘D (o Atlas tem teclas de painel próprias), `write_screen_file`, `scroll_to_selection` (⌘J é o dock inferior), `adjust_selection` (⇧Home/End/setas para estender uma seleção), `search_selection` (⌘E é Config), `toggle_fullscreen`, `inspector`, quick terminal (exige um atalho global e uma janela acessória), `mouse-hide-while-typing`.

## Roadmap: Windows e Linux

Hoje o terminal nativo roda só no macOS; Windows e Linux usam xterm.js (`term_stub.rs`). A meta é o mesmo terminal, no mesmo nível, no Windows 10 1809+ / Windows 11 e no Linux: GPU completa, GPU integrada primeiro, CPU e memória quase nulas com muitos agentes transmitindo texto. Projeto, pesquisa e medições: [discussão #237](https://github.com/codeatlasdev/atlas/discussions/237). Issue de acompanhamento: [#249](https://github.com/codeatlasdev/atlas/issues/249); toda issue é `area: terminal` no milestone **Roadmap: native GPU terminal (Windows & Linux)**.

| Camada | Decisão | Por quê |
|---|---|---|
| Renderer no Windows | D3D11 nativo atrás de um trait de backend de GPU; VT, fontes, atlas e montagem de instâncias compartilhados | medido em um Win11 com Iris Xe: D3D11 32 MB / ~40 ms para criar um dispositivo, wgpu 30 DX12 97 MB / ~115 ms; o D3D11 também roda em VMs e no WARP |
| Shaders | uma fonte, `term.wgsl`; naga → HLSL → FXC SM 5.0 em tempo de build | spike: os 10 entry points compilam depois de trocar as linhas de sampler-heap do DX12 do naga |
| Renderer no macOS / Linux | wgpu (Metal / Vulkan) | sem mudança |
| Sem GPU, VM, RDP | D3D11 WARP, depois xterm.js | |
| PTY no Windows | camada ConPTY própria no `atlas-host`, `conpty.dll` + `OpenConsole.exe` embutidos e fixados em versão, autoteste no primeiro uso, fallback para o inbox | o Windows 10 não recebe as correções do ConPTY inbox; o `portable-pty` carrega a DLL por nome simples e não consegue pedir larguras de grafemas |
| Texto no Windows | shaping, fallback e rasterização do DirectWrite no atlas | mesma receita do Windows Terminal e do Zed |
| Composição no Windows | HWND filho primeiro, depois DirectComposition + hospedagem visual do WebView2 | só o DirectComposition deixa os overlays HTML cobrirem o terminal |
| Linux | janela filha X11; o Wayland fica no xterm.js até o wry migrar para o GTK4 | |

| Fase | Issue | Entrega |
|---|---|---|
| 0 | [#239](https://github.com/codeatlasdev/atlas/issues/239) | harness de benchmark, referência no macOS e baselines no Windows, matriz de máquinas |
| 1 | [#240](https://github.com/codeatlasdev/atlas/issues/240) | camada ConPTY própria |
| spike | [#241](https://github.com/codeatlasdev/atlas/issues/241) | HWND filho ao lado do WebView2: entrada, foco, IME, z-order |
| 2 | [#242](https://github.com/codeatlasdev/atlas/issues/242) | backend D3D11, `term/win.rs` |
| 3 | [#243](https://github.com/codeatlasdev/atlas/issues/243) | bytes do PTY direto para o terminal nativo, renderização sob demanda |
| 4 | [#244](https://github.com/codeatlasdev/atlas/issues/244) | DirectWrite, IME (TSF), UI Automation, DPI por monitor |
| 5 | [#245](https://github.com/codeatlasdev/atlas/issues/245) | DirectComposition + hospedagem visual do WebView2 |
| 6 | [#246](https://github.com/codeatlasdev/atlas/issues/246) | ativado por padrão no Windows |
| 7 | [#247](https://github.com/codeatlasdev/atlas/issues/247) | Linux X11 |
| 8 | [#248](https://github.com/codeatlasdev/atlas/issues/248) | opcional: WSL sem ConPTY |

Trabalho de engine compartilhado (todas as plataformas): [#252](https://github.com/codeatlasdev/atlas/issues/252) passagem única sobre os bytes do PTY antes do parser, [#253](https://github.com/codeatlasdev/atlas/issues/253) leitura e parse em threads separadas, [#254](https://github.com/codeatlasdev/atlas/issues/254) liberar os recursos de GPU de terminais ocultos, [#255](https://github.com/codeatlasdev/atlas/issues/255) spike do libghostty-vt, [#256](https://github.com/codeatlasdev/atlas/issues/256) compressão em repouso do scrollback frio. No Linux, o Wayland espera o wry no GTK4 e depois segue o modelo do Ghostty (frames exportados como DMA-BUF para um widget GTK), veja #247.

Metas propostas, **ainda não medidas** no Windows nem no Linux, todas em GPU integrada: parse ≥ 500 MB/s sem bloquear a UI; < 3% de CPU com 4 TUIs de agentes a 30 redesenhos/s; tecla até pixel p95 ≤ um frame + 2 ms; 0 despertares/s em repouso; abas em segundo plano fazem parse mas não desenham; ≤ 8 MB por terminal com 10.000 linhas; inicialização do shell ≤ 150 ms acima do processo puro. No Windows, o ConPTY limita a vazão. O ponto de partida no macOS é [Desempenho](#desempenho) abaixo (M4 Pro, execuções únicas preliminares, compiladas com debug assertions ligadas, então não comparáveis diretamente com os builds distribuídos); a Fase 0 acrescenta o que falta: um Mac com GPU integrada, despertares em repouso, CPU e RAM com N agentes, latência de tecla até pixel, inicialização e todos os números do Windows.

## Desempenho

Preliminar, execuções únicas. M4 Pro, grade 144×44, 120 Hz. Atlas: perfil `release` compilado com `CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true` (necessário para a ponte de debug; verificações de overflow ligadas), então os builds distribuídos devem ser pelo menos tão rápidos. Ghostty 1.x como referência. Os dois terminais rodaram um após o outro nos mesmos arquivos e no mesmo tamanho de janela.

| Carga | Atlas | Ghostty |
|---|---|---|
| `seq 1 10000000` | 8,20 s | 8,48 s |
| `cat` 1 GiB (texto com SGR) | 8,02 s | 12,34 s |
| trabalho de renderização por frame, p50 / p95 / máx | 0,22 / 0,29 / 1,6 ms | n/d |
| intervalo entre frames p50 / p95 | 8,33 / 8,45 ms | n/d |

vtebench, `--max-secs 3` (mediana de ms por amostra de 1 MiB, 97 a 429 amostras por célula; menor é melhor):

| Benchmark | Atlas | Ghostty |
|---|---|---|
| dense_cells | 9 | 8 |
| medium_cells | 8 | 11 |
| sync_medium_cells | 8 | 13 |
| unicode | 7 | 6 |
| scrolling | 18 | 16 |
| scrolling_fullscreen | 25 | 21 |
| scrolling_top_region | 30 | 19 |
| scrolling_top_small_region | 14 | 18 |
| scrolling_bottom_region | 14 | 18 |
| scrolling_bottom_small_region | 14 | 18 |

A vazão do parser (`Vt::feed`, blocos de 64 KiB) é de cerca de 170 MB/s otimizado e 14 MB/s em um build de debug, então meça com um build otimizado.

### Muitos agentes ocupados (#110)

Um terminal a 8 ms por frame não diz nada sobre sete deles desenhando uma TUI a 30 Hz enquanto os hooks, os títulos e a saída das ferramentas caem todos em uma única thread de UI, que é como as pessoas realmente usam o Atlas. `scripts/agents-load-bench.ts` roda esse cenário em uma instância de agente (nunca o seu app; `bun run desktop:agent` compila antes o app de debug):

```bash
scripts/heavy.sh bun scripts/agents-load-bench.ts [--agents 7] [--seconds 30] [--warmup 5] [--save docs/perf/agents-7.json] [--keep]
bun scripts/agents-load-bench.ts --judge docs/perf/agents-7.json      # re-judge a saved report, no app
```

Ele abre N abas de terminal em uma worktree, cada uma rodando `scripts/agents-load-gen.ts`, um omp sintético: um redesenho de tela cheia na tela alternativa (`--hz`, padrão 30), linhas em fluxo (`--lines`, 40/s), um spinner no título OSC 0 (`--title-hz`, 10/s, quase sempre com o texto que já tinha) e um post de hook ao servidor de hooks real (`--hook-every`, 150 ms). As abas são abas de agente, então os eventos de status, a barra lateral, a bandeja e o feed reagem como com agentes reais. Depois do warmup ele mede, uma vez por segundo durante `--seconds`:

| Medida | Como | Limite (`scripts/agents-load-judge.ts`) |
|---|---|---|
| travamento da thread principal | maior intervalo entre dois turnos de `MessageChannel` a cada segundo (o WKWebView não tem `longtask`) | p95 ≤ 16 ms, máx ≤ 100 ms |
| intervalo entre frames da página | intervalos de `requestAnimationFrame` | p95 ≤ 34 ms |
| frames do terminal | `frames` do `term_stats` na janela, para todo terminal cujo programa imprimiu | ≥ 20 fps cada |
| memória | RSS do app, do hostd e do WebContent da instância, último menos primeiro | ≤ 400 MB de crescimento cada |
| tempestade de eventos | `agent://status` + `host://event` que a página recebeu, por agente por segundo | ≤ 8 |

Ele sai com 1 acima de qualquer limite, e também quando nada foi medido (nenhum terminal aberto, nenhuma amostra, ou um relatório que não é um relatório: um app que travou não é uma aprovação). A instância é parada explicitamente no fim, também em caso de falha. `atlas desktop debug perf --seconds 3` é a mesma medição de travamento e de intervalo entre frames do que está na tela, para uma olhada manual.

O que o bench existe para pegar, e o que impede cada caso de voltar:

- **Lock do registro mantido durante um lock lento.** O `term_stats` travava o VT de cada terminal (`FairMutex`, na fila atrás do leitor que o alimenta) enquanto segurava o registro que todo comando `term_*` toma primeiro, então um terminal ocupado travava todos, inclusive o tratamento do ponteiro. Agora ele lê os handles primeiro (`open_terms`).
- **Pushes de título idênticos.** O omp redesenha o título do spinner a cada frame, quase sempre com o mesmo texto; cada OSC virava um push `pty_title`, um evento do Tauri e quatro listeners da webview. O hostd só envia um título quando ele muda.
- **Pushes de status idênticos.** O omp posta cada evento de ferramenta, e cada um virava um push `agent_status`. O hostd envia quando algo que um cliente mostra mudou e, fora isso, uma vez por segundo por painel (`HEARTBEAT_MS`; o desktop confia nos hooks mais que no título enquanto `updated_at` tem menos de 5 s).
- **`ps` por terminal em `pty_list`.** A lista pedia o programa em primeiro plano de cada sessão com um `ps` bloqueante, a cada conexão, operação de aba e atualização da barra lateral. Agora só o `pty_info` o tem.

### Benchmark de TUIs de agentes

`scripts/term-agent-bench.ts` é o gate de flicker e de rolagem para o que os agentes imprimem. Um terminal rápido no `cat` ainda pode provocar tearing no Claude Code, porque a falha está em como um *redesenho* encontra o renderer: o Ink apaga e reescreve as últimas N linhas, o ratatui escreve diffs a cada 8 ms, o omp reescreve caixas com CUP + EL, e qualquer um deles pode pausar no meio de um frame. O bench roda em uma instância de agente (nunca o seu app) por meio de `scripts/heavy.sh`:

```bash
scripts/heavy.sh bun scripts/term-agent-bench.ts [--seconds 8] [--only ink-2026,slow-2026] [--hz 60] [--save docs/perf/term-agent.json] [--keep]
bun scripts/term-agent-bench.ts --judge docs/perf/term-agent.json      # re-judge a saved report, no app
```

(ou a ação **Terminal agent bench**, tecla `K`, no `atlas.yaml` deste repositório). Exige a permissão de gravação de tela, como o flicker sweep. O gerador é `scripts/term-agent-gen.ts`:

| Padrão | Imita | O que pega |
|---|---|---|
| `ink-2026` | Ink (Claude Code clássico): reescrita com `CUU` + `EL` da região dinâmica dentro de 2026 | frames que precisam chegar inteiros à tela |
| `ink-no-2026` | o mesmo sem 2026 | um renderer que desenha entre leituras mostra meio frame; a cadência precisa escondê-lo |
| `ratatui-diff` | codex: um frame 2026 de células alteradas a cada 8,3 ms | um render por leitura (400 fps) em vez de um por atualização do monitor |
| `omp-boxes` | omp: caixas com borda reescritas com `CUP` + `EL`, cursor oculto, autowrap desligado | box drawing sob reescrita |
| `slow-2026` | um frame 2026 dividido por uma pausa de 300 ms (hook, GC) | o timeout de segurança do 2026: os 150 ms do vte desenham o meio frame, o 1 s do Ghostty não |
| `scroll-stream` | saída em fluxo durante a rolagem (#97) e um programa de mouse contando relatórios de roda | a viewport puxada para o fim, relatórios perdidos ou fora de ordem |

Cada frame começa toda linha com uma célula colorida pelo id do frame (seis cores), então uma captura cujas linhas mostram duas cores misturou dois frames (rasgada): o oráculo está nos pixels, não em um contador. `crates/atlas-term/tests/agent_frames.rs` prova isso no `Vt` real com os bytes do próprio gerador (um frame inteiro é uma cor, metade do seguinte por cima são duas, e um frame 2026 alimentado pela metade nunca chega à grade), então o oráculo do bench não pode se afastar do produtor.

Para cada padrão, ele filma o terminal durante `--seconds`, recorta no retângulo dele, julga com `apps/desktop/src/lib/debug/flickerSweep.ts` e lê `term_stats`:

| Medida | Limite (`scripts/term-agent-judge.ts`) |
|---|---|
| capturas rasgadas | 0 (todo padrão com o oráculo) |
| blips, frames em branco, flashes BT.1702 | 0 |
| frames desenhados por segundo | ≤ 1,05 × a taxa de atualização (coalescência), ≥ 0,5 × a taxa de atualização enquanto o produtor imprime |
| intervalo entre frames p95 | ≤ 1,1 × o intervalo de atualização |
| rolagem: linhas movidas vs pedidas | dentro de 1 linha (um fluxo de trackpad sintetizado com inércia, alimentado como chamadas `term_wheel` no ritmo dele: o que o handler de roda da página invoca) |
| relatórios de roda que um programa de mouse contou | = cliques enviados, em ordem |
| viewport enquanto a saída flui | fica onde a pessoa a deixou |
| intervalo entre frames com inércia p95 | ≤ 1,5 × o intervalo de atualização |

Ele sai com 1 em qualquer linha, e também em uma etapa que não pôde rodar, um produtor que não imprimiu nada, uma gravação com menos de 3 frames ou um relatório que não é um relatório (`--judge` valida todos os campos: um arquivo truncado não pode passar). O juiz é puro e coberto por `bun test scripts` (`term-agent-bench.test.ts`: cada modo de falha aciona a própria linha). A instância é parada explicitamente no fim, também em caso de falha.

## Depuração

Com um build de debug rodando (`atlas link --desktop`), a ponte de debug o conduz sem precisar de entrada manual:

```bash
atlas desktop debug eval 'return await window.__TAURI_INTERNALS__.invoke("term_stats")'   # frames, timings, cell size
atlas desktop debug eval 'return await window.__TAURI_INTERNALS__.invoke("term_text",{tab:"<tab id>"})'
atlas desktop debug shot /tmp/term.png
```
