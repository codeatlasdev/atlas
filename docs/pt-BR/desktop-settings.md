---
title: "Configurações do desktop"
source: desktop-settings.md
source_sha: "e0b09d96793f0f4243fd87e6fe07693804fc7d34"
translation: machine
---

# Configurações do desktop

O Atlas Desktop guarda as preferências em `~/.atlas/desktop/settings.json`. Tudo aqui pode ser mudado em **Configurações** (⌘,), que também tem uma caixa de pesquisa que leva à configuração pelo que ela faz. Edite o arquivo à mão quando quiser automatizar ou compartilhar uma configuração; o app lê o arquivo de novo e **descarta valores do tipo errado ou fora do intervalo**, usando o padrão no lugar, então uma edição errada nunca quebra o app.

As preferências de editor, interface e terminal usam os [nomes de configuração do Zed](https://zed.dev/docs/configuring-zed).

## Onde as coisas ficam em Configurações

As Configurações (⌘,) têm treze seções em três grupos (doze fora do macOS, onde **Captura** fica oculta) e uma caixa de pesquisa que leva à configuração pelo que ela faz. Cada seção começa com uma linha dizendo para que serve.

| Grupo | Seção | O que tem nela |
|---|---|---|
| Atlas | **Geral** | Atualizações: o app, a CLI `atlas` e o terminal host, o canal de versões que eles seguem e o idioma de exibição |
| | **Aparência** | Tema (claro, escuro, terminal), importar e exportar, cor de destaque, zoom, fonte da interface, painel de arquivos, painéis divididos |
| | **Notificações e sons** | Notificações quando um agente termina ou precisa de você; os sons que o Atlas toca, um interruptor para cada |
| | **Captura** (só no macOS) | Capturas de tela, gravações de tela e GIFs por atalho: acesso, teclas, onde são salvos, qualidade, a sobreposição, capturas recentes |
| | **Atalhos de teclado** | Reatribua ou remova a tecla de qualquer comando |
| | **Ajuda e primeiros passos** | O tour guiado, a folha de atalhos, a lista de primeiros passos |
| Trabalho | **Workspaces** | Onde as worktrees são criadas, prefixo do nome da branch, limpeza automática, limpeza manual, higiene (processos órfãos e disco) |
| | **Editor** | Abas **Editor** (fonte do código, edição, exibição) e **Linguagens** (servidores de linguagem) |
| | **Terminal** | Renderizador, fonte, cursor, margem interna, scrollback, área de transferência |
| | **Git** | Qual agente escreve as mensagens de commit |
| | **Agentes** | Agente padrão, aprovação de ferramentas, argumentos extras, o dock de agentes (interruptor e atalhos), agentes personalizados, comandos rápidos |
| Conectar | **Integrações** | Abas **Shell e links**, **Hooks dos agentes**, **Hosts SSH** e **Mobile** |
| | **Atlas Crew** | Conectar um crew server, conectar o GitHub, respostas curtas, ver o onboarding de novo |

Onde ficava antes: *Shell dos seus projetos* e *Abrir links no* (Geral) agora estão em **Integrações › Shell e links**; os interruptores de hooks de agentes (Agentes) estão em **Integrações › Hooks dos agentes**; **Hosts SSH** e **Mobile** eram seções próprias e viraram abas de **Integrações**; notificações e sons (Geral) estão em **Notificações e sons**. Links antigos como `updates.show("ssh")` ou `appearance/terminal` ainda abrem o lugar certo.

### O registro

`apps/desktop/src/lib/settings/registry.ts` é a lista única do que as Configurações contêm: **categoria** (uma entrada da barra lateral) → **página** (uma aba, ou a página inteira) → **grupo** (um cartão) → **item** (uma linha). A barra lateral e os títulos dos grupos, o cabeçalho da página e a faixa de abas, o índice de pesquisa, o caminho de um resultado de pesquisa e todo link direto `updates.show("<id>")` saem dele, então não têm como divergir.

Para adicionar uma configuração:

1. Coloque um `item(id, label, description, keywords?, ...aliases)` no grupo certo do `registry.ts`. O id nunca muda depois.
2. Renderize a linha no componente da página indicado em `files` dessa página, com `<Row id="the-id" …>`.

Só isso: a pesquisa encontra a configuração pelo rótulo, pela descrição e pelas palavras-chave, `updates.show("the-id")` abre a página dela e destaca a linha, e os testes falham se faltar qualquer uma das duas partes (`registry.test.ts`: todo item alcançável por pesquisa e por link direto, nenhum alias esconde um id; `placement.test.ts`: os ids que um componente renderiza são exatamente os que o registro coloca sob ele).

Para adicionar uma seção ou uma aba, adicione a categoria (ou a página) ao registro, o componente dela em `components/settings/pageComponents.ts` e o ícone em `SettingsNav.svelte`; os tipos não compilam até os três existirem. Quando algo muda de lugar, mantenha o id e adicione o id antigo do link direto a `aliases` (no item, na página ou na categoria).

| Link direto | Abre |
|---|---|
| `terminal`, `integrations` | uma seção (a primeira aba, quando tem abas) |
| `editor/languages` | uma seção e uma aba |
| `shell`, `ws-dir`, `notify` … | a página dessa linha, rolada até ela e com destaque |
| `appearance/terminal`, `ssh`, `languages` … | ids de versões anteriores, mantidos como aliases |
| qualquer coisa desconhecida | a primeira seção |

## Editor e interface

| Chave | Padrão | Notas |
|---|---|---|
| `buffer_font_family` | `ui-monospace, "SF Mono", SFMono-Regular, Menlo, "Cascadia Mono", Consolas, monospace` | fonte do editor de código |
| `buffer_font_size` | `12.5` | 6 a 48 |
| `buffer_line_height` | `1.55` | 1 a 3 |
| `ui_font_family` | `-apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", system-ui, sans-serif` | fonte da interface |
| `ui_font_size` | `13` | 9 a 24 |
| `project_panel.dock` | `"left"` | `left` ou `right`: de que lado fica o painel de arquivos |
| `project_panel.file_icons` | `true` | ícones de arquivo do Material Icon Theme |
| `project_panel.folder_icons` | `true` | |
| `project_panel.indent_size` | `12` | 4 a 40 px |
| `active_pane_modifiers.border_size` | `0` | borda de 0 a 8 px em volta do painel ativo |
| `active_pane_modifiers.inactive_opacity` | `1` | 0,2 a 1; menos escurece os painéis inativos |
| `enable_language_servers` | `true` | servidores de linguagem (completar código, diagnósticos, hover, ir para a definição) |
| `format_on_save` | `false` | formata com o servidor de linguagem ao salvar |
| `inlay_hints.enabled` | `true` | tipos inferidos e nomes de parâmetros exibidos na linha |

## Editor de código

Configurações → Editor.

| Chave | Padrão | Notas |
|---|---|---|
| `editor.tab_size` | `4` | 1 a 16 colunas |
| `editor.insert_spaces` | `true` | Tab insere espaços; `false` insere caracteres de tabulação |
| `editor.word_wrap` | `false` | quebra linhas longas na borda do editor |
| `editor.render_whitespace` | `"none"` | `none`, `selection` ou `all` |
| `editor.line_numbers` | `true` | |
| `editor.minimap` | `false` | |
| `editor.sticky_scroll` | `true` | mantém a função ou classe que contém o trecho fixada no topo enquanto você rola o corpo dela |
| `editor.bracket_pair_colorization` | `true` | |
| `editor.indent_guides` | `true` | |
| `editor.rulers` | `[]` | colunas para linhas verticais, números inteiros de 1 a 500, por exemplo `[80, 100]`; qualquer outro valor na lista é descartado |
| `editor.highlight_active_line` | `true` | |
| `editor.trim_trailing_whitespace_on_save` | `true` | |
| `editor.insert_final_newline_on_save` | `true` | |
| `editor.large_file_threshold_bytes` | `2000000` | 100000 a 100000000; arquivos maiores abrem em um modo leve, sem recursos de linguagem |
| `editor.scroll_past_end` | `false` | |
| `editor.smooth_scrolling` | `true` | |
| `editor.auto_reload` | `true` | recarrega um arquivo alterado em disco quando ele não tem edições sem salvar |

## Servidores de linguagem

Configurações → Editor → Linguagens lista todo servidor de linguagem que o Atlas conhece, mostra se ele está no seu `PATH`, é gerenciado pelo Atlas ou está ausente, e deixa você instalar, atualizar, desinstalar, reiniciar e abrir o log dele. O Atlas roda o servidor do seu `PATH` quando encontra um e, senão, baixa um na primeira vez que você abre um arquivo daquela linguagem.

`lsp` substitui o servidor de uma linguagem ou muda como ele inicia. As chaves são ids de linguagem do LSP (`typescript`, `rust`, `go`, `python`…); o mesmo formato funciona no `.atlas/settings.json` de um projeto.

```json
{
  "lsp": {
    "typescript": {
      "command": "/opt/homebrew/bin/typescript-language-server",
      "args": ["--stdio"],
      "initialization_options": { "preferences": { "importModuleSpecifier": "relative" } },
      "settings": { "typescript": { "inlayHints": { "parameterNames": { "enabled": "all" } } } }
    },
    "python": { "disabled": true }
  }
}
```

| Chave | Notas |
|---|---|
| `lsp.<language>.command` | roda este programa em vez do que o Atlas encontra ou instala: um nome no `PATH` ou um caminho absoluto. Se ele não puder ser iniciado, a barra de status avisa |
| `lsp.<language>.args` | argumentos para o `command` |
| `lsp.<language>.initialization_options` | objeto JSON enviado ao servidor uma vez, quando ele inicia |
| `lsp.<language>.settings` | objeto JSON enviado como a configuração de workspace do servidor |
| `lsp.<language>.disabled` | `true` desliga os recursos de linguagem para essa linguagem |

Entradas do tipo errado são descartadas uma a uma (um `command` que não é string, `args` que não é uma lista de strings, uma opção que não é objeto), e uma entrada que fica vazia é removida, então uma edição errada nunca impede as outras de funcionar. As mudanças valem para servidores iniciados depois; **Reiniciar**, na aba Linguagens, aplica a um servidor em execução.

### Substituições por projeto

Um projeto pode ter as próprias substituições em `.atlas/settings.json`, na raiz da worktree, com o mesmo formato de `lsp`:

```json
{
  "lsp": {
    "go": { "command": "/usr/local/bin/gopls", "settings": { "gopls": { "staticcheck": true } } }
  }
}
```

O arquivo do projeto vence o `~/.atlas/desktop/settings.json` campo a campo para a linguagem que ele nomeia (`command`, `args` e `disabled` são substituídos; `initialization_options` e `settings` são mesclados em profundidade, com as chaves do projeto vencendo); linguagens e campos que ele não menciona mantêm as suas configurações. Configurações → Editor → Linguagens leva ao arquivo (`.atlas/settings.json` no rodapé) e o cria quando não existe. Um arquivo ilegível é ignorado e relatado no status do servidor de linguagem do editor.

## Terminal

| Chave | Padrão | Notas |
|---|---|---|
| `terminal.font_family` | `SF Mono, Menlo, Cascadia Mono, Consolas, ui-monospace, monospace` | lista de fontes CSS para os dois terminais. O [terminal GPU](terminal.md) usa a primeira família instalada (fontes variáveis funcionam) e, senão, o JetBrains Mono embutido; vale na hora |
| `terminal.font_thicken` | `false` | desenha o texto um pouco mais pesado (`font-thicken` do Ghostty; o xterm.js usa os pesos 500/700); vale na hora |
| `terminal.font_size` | `12.5` | 6 a 48; vale na hora |
| `terminal.cursor_shape` | `"block"` | `block`, `bar`, `underline` ou `hollow` |
| `terminal.blinking` | `false` | |
| `terminal.max_scroll_history_lines` | `10000` | 0 a 100000 |
| `terminal.scrollback_bytes` | `67108864` | 1 MiB a 1 GiB; o máximo de memória que um [terminal GPU](terminal.md#scrollback) gasta com o scrollback (bytes de células). As Configurações mostram em MB. Vale o menor entre este limite e o limite de linhas; vale na hora |
| `terminal.osc52` | `"ask"` | o que acontece quando um programa escreve na sua área de transferência: `allow`, `ask` ou `deny`. Ler a área de transferência é sempre negado |
| `terminal.ligatures` | `true` | terminal GPU |
| `terminal.minimum_contrast` | `4.5` | 1 a 21; razão WCAG até a qual o texto de uma célula é clareado na hora de desenhar (a tabela ANSI do tema não muda), `1` = desligado. Terminal GPU |
| `terminal.minimum_contrast_chosen` | `false` | Gravada como `true` quando você define o contraste mínimo por conta própria. Um `minimum_contrast` gravado como `1` sem ela é o padrão anterior ao 4.5 (a aba Terminal costumava gravá-lo de volta) e é lido como `4.5`; defina `1` na aba Terminal para mantê-lo desligado |
| `terminal.copy_on_select` | `true` | terminal GPU |
| `terminal.option_as_alt` | `"both"` | `both`, `left`, `right` ou `none`. Terminal GPU |
| `terminal.click_to_move` | `true` | um clique no prompt move o cursor (exige integração com o shell). Terminal GPU |
| `terminal.padding_x` | `10` | 0 a 64 pt; margem interna à esquerda e à direita do terminal. A margem usa o fundo do tema do terminal |
| `terminal.padding_y` | `6` | 0 a 64 pt; margem interna acima e abaixo |

O próprio terminal GPU é ligado em **Configurações → Terminal → Terminal rápido (GPU)**.

## Tema e atalhos

| Chave | Notas |
|---|---|
| `theme` | `{ "v": 2, "mode": "system", "light": "Atlas Light", "dark": "Atlas Dark", "terminal": "app" }`: `mode` é `system`, `light` ou `dark`; `light` e `dark` nomeiam o tema usado em cada aparência; `terminal` é `app` (igual ao app) ou o nome de qualquer tema. Os padrões são Atlas Dark e Atlas Light (um par baseado no Tokyo Night, veja [Atlas Dark e Light](#atlas-dark-e-light)); Dracula e Alucard, de Zeno Rocha e Luxonauta (MIT), continuam embutidos, e quem os escolheu os mantém. Os seus próprios temas ficam em `~/.atlas/themes/` como arquivos de tema do Zed (formato v0.2.0) e são recarregados quando você salva; o antigo `~/.atlas/desktop/themes/` ainda é lido. Um nome de tema que não existe mais volta em silêncio para o Atlas Dark (escuro) ou o Atlas Light (claro), e o terminal para o tema do app. Configurações → Aparência importa configurações do Ghostty, `.itermcolors` do iTerm, `.json` e `.vsix` de tema do VS Code, `.toml`/`.yml` do Alacritty e YAML do Base16, ou um `.zip` / pasta com vários temas (descompactados em uma pasta temporária neste Mac, nada é enviado; traga a sua cópia licenciada, o Atlas não inclui temas pagos), e exporta um tema como JSON ou como configuração do Ghostty. Embutidos: Atlas Dark/Light, Dracula, Alucard, One Dark/Light |
| `keymap` | reatribui ou desvincula atalhos, sintaxe do Zed, `null` desvincula. Veja abaixo |

```json
{
  "keymap": {
    "cmd-j": null,
    "cmd-shift-j": "workspace::ToggleBottomDock"
  }
}
```

Todo atalho é um comando com nome (`workspace::ToggleLeftDock`, `pane::SplitRight`, `atlas::NewWorktree`…). ⌘⇧P lista todos os comandos com a tecla atual, e Configurações → Atalhos de teclado os reatribui com um gravador. Os modificadores são `ctrl`, `alt`, `shift` e `cmd`, unidos por `-`: `cmd-shift-p`, `ctrl-tab`, `alt-cmd-left`. ⌘Z, ⇧⌘Z e ⌘A são fixos no menu do app e não podem ser reatribuídos. Os padrões estão listados em [App desktop › Teclado](desktop.md#teclado).

## Workspaces, agentes e notificações

| Chave | Notas |
|---|---|
| `workspaceDir` | pasta das novas worktrees (padrão `~/.atlas/worktrees`) |
| `branchPrefix` | `"git-username"` (padrão: o seu `git config user.name`, transformado em slug), `"none"` ou `"custom"`; o prefixo das branches que o Atlas cria |
| `branchPrefixCustom` | o prefixo usado quando `branchPrefix` é `"custom"` |
| `nestWorkspaces` | `"false"` deixa as pastas de worktree planas, em vez de `<repo>/<name>` |
| `defaultAgent` | agente iniciado por padrão em uma nova worktree: `"omp"` (padrão), `"claude"` ou `"codex"` |
| `agentPermissions` | `"manual"` (padrão, o agente pergunta antes de rodar comandos ou editar arquivos) ou `"yolo"` (nunca pergunta; use só em worktrees descartáveis) |
| `agentArgs` | argumentos extras de linha de comando por agente |
| `notifications` | `{ "enabled": true, "sound": true, "suppressWhenFocused": true }`: notificações nativas para agentes que terminaram ou precisam de você |
| `quickCommands` | comandos que você envia a um agente com um clique |
| `openServicesIn` | `system` abre as URLs dos serviços no seu navegador, em vez do navegador do Atlas |

## Shell, atualizações e primeiros passos

O shell fica em Configurações → Integrações → Shell e links; as atualizações ficam em Configurações → Geral.

| Chave | Notas |
|---|---|
| `shell` | o shell cujo ambiente é usado nos seus projetos, por exemplo `"/opt/homebrew/bin/fish"`. Sem valor = o shell de login da sua conta. Veja [App desktop › Ambiente do shell](desktop.md#ambiente-do-shell) |
| `updates` | interruptor de verificação automática, versão ignorada e o que você já viu. Gerenciada pelo app |
| _canal de atualização_ | não é uma chave: o canal é a palavra única em `~/.atlas/update-channel`, compartilhada com a CLI `atlas` (`ATLAS_UPDATE_CHANNEL` o substitui por processo). Escolha em Configurações → Geral → Atualizações → _Canal de atualização_. Veja [App desktop › Atualizações](desktop.md#atualizações) |
| `onboarding` | progresso do tour da primeira execução e da lista de primeiros passos. Gerenciada pelo app |

## Arquivos relacionados em `~/.atlas/desktop/`

| Arquivo | Conteúdo |
|---|---|
| `projects.json` | os projetos que você adicionou |
| `workspace.json` | abas, painéis e docks por worktree. Uma cópia que não pode ser lida (não é JSON, está vazia, não é um workspace) nunca impede o app de abrir: ela é movida para `workspace.json.corrupt-<ms unix>`, o Atlas começa com um workspace vazio e um aviso diz onde o arquivo foi salvo |
| `crew-servers.json` | os servidores de crew que você registrou. Uma cópia ilegível é mantida como `crew-servers.json.corrupt-<ms unix>`, do mesmo jeito |
| `themes/` | local antigo dos seus arquivos de tema do Zed (ainda é lido; os novos vão para `~/.atlas/themes/`) |
| `hook-endpoint.env` | endpoint de loopback e token dos hooks de agentes (token novo a cada execução do hostd) |

## Atlas Dark e Light

O par de temas padrão. O Atlas Dark nasce da paleta personalizada "Tokyo Night" do Ghostty do mantenedor (cores de [enkia](https://github.com/enkia/tokyo-night-vscode-theme), MIT): as 16 cores ANSI, o primeiro plano, o cursor e os matizes de sintaxe são usados como estão, e o fundo da interface é refeito sobre uma única rampa de azul. Tudo mora em `apps/desktop/src/lib/themes/atlas.json`; o terminal, os logs dos serviços, o editor e o chrome leem esse arquivo, e a tabela de reserva do terminal GPU (`atlas-term` `colors.rs`) e os `--ansi-*` de reserva do `.dark` em `app.css` são testes fixados nele.

Por que ele tem essa aparência (o que Dracula, Tokyo Night, Catppuccin e One têm em comum, adaptado a painéis reais):

| Decisão | Valor | Motivo |
|---|---|---|
| Superfícies em um matiz só | matiz OKLCH ~262, croma de 0,029 a 0,036; `#0b1320` rebaixado, `#131c2b` editor e terminal, `#1d2637` elevado, `#273143` sobreposição | Um único matiz de azul se lê como um material só. O croma fica baixo nas áreas grandes (calmo, sem blocos saturados), mas acima de 0,025 para que um painel IPS com brilho de backlight não vire cinza. |
| Degraus iguais | cerca de 4% de luminosidade OKLCH entre níveis (0,186, 0,226, 0,268, 0,312) | Visíveis em IPS e em painéis de baixo contraste, então painéis, popovers e a barra lateral se separam sem bordas pesadas. |
| Sem preto puro | a superfície mais escura tem L 0,186 | O preto do OLED borra e esmaga o detalhe das sombras ao rolar; um azul quase preto mantém os pixels ligados e os gradientes suaves. |
| Primeiro plano suave | texto `#c0caf5` (L 0,85), nunca branco puro | Texto claro sobre fundo escuro irradia (halação, tremulação do PWM no OLED) e afina as bordas dos glifos; 12:1 no editor é suficiente. |
| Acentos com a mesma luminosidade | vermelho, laranja, azul e magenta perto de L 0,75; amarelo, verde e ciano perto de L 0,80, croma de no máximo 0,15 | A regra do Dracula: papéis fixos, acentos de alta saturação sobre um fundo dessaturado. A mesma luminosidade percebida impede que um acento grite mais que outro, e todos têm 5,5:1 ou mais sobre a superfície de sobreposição. |
| Croma seguro para sRGB | nenhum acento passa do que o sRGB comporta | Uma tela P3 mostra a mesma cor; nada é cortado no sRGB nem muda em gamut amplo. |
| Legível em 1x | cores de sintaxe ajustadas para pelo menos 4,7:1 em toda superfície de código (linha, seleção, linha ativa); comentários `#acbae2` em itálico | O anti-aliasing em um painel de baixo DPI afina texto claro sobre escuro, então uma margem acima do AA 4,5 vale mais do que ficar na linha. |
| Papéis de sintaxe | palavra-chave roxo, função verde, string âmbar, tipo e namespace ciano, constante laranja, propriedade azul | Um matiz por papel em todo lugar (a especificação do Dracula), com os matizes do Tokyo. |

O Atlas Light é o par do Tokyo Night Day: fundo cinza-azulado claro (`#e1e2e7`), texto azul-marinho, o mesmo mapa de papel para matiz com valores mais profundos para que toda cor mantenha o AA sobre o fundo claro. O `bun run test` (`theme/atlasDefault.test.ts`) fixa a paleta, as regras da escala OKLCH acima e a seleção padrão; o `bun scripts/contrast-audit.ts` confere as telas renderizadas nos dois temas.
