---
title: "Editor de código"
source: editor.md
source_sha: "6d35e816834e1f2f739c0fcac8201ad7c73da685"
translation: machine
---

# Editor de código

O Atlas Desktop abre arquivos de texto em um editor CodeMirror 6 com language servers integrados. Esta página explica o que ele faz, como configurá-lo e como o desempenho dele se mantém previsível. As chaves estão listadas em [Configurações do desktop](desktop-settings.md); os atalhos podem ser reatribuídos em Configurações → Atalhos de teclado.

## A barra de status

Abaixo de todo editor de texto:

| Item | Clique para |
|---|---|
| `Ln 12, Col 5 (3 selections) (24 selected)` | ir para uma linha (⌃G) |
| `3 errors, 1 warning` | focar o editor no primeiro problema |
| `Spaces: 2` / `Tab Size: 4` | mudar a indentação, ou converter a indentação do arquivo |
| `UTF-8` | (somente leitura: o Atlas edita texto UTF-8; os outros arquivos abrem como binário) |
| `LF` / `CRLF` / `Mixed` | escolher o fim de linha gravado ao salvar |
| nome da linguagem | (o modo de sintaxe segue o nome do arquivo) |
| chip do language server | veja abaixo |

Acima do editor, as **breadcrumbs** mostram o caminho e os símbolos em volta do cursor (a partir dos símbolos do documento no language server, ou da árvore de sintaxe quando não há servidor); clique em um símbolo para ir até ele.

## Language servers

O Atlas roda os language servers dentro do `atlas hostd` (um processo por linguagem e projeto, compartilhado por todos os arquivos abertos dela). Linguagens com suporte: TypeScript, JavaScript, Rust, Go, Python, PHP, HTML, CSS (SCSS, Less), JSON, YAML e Shell. Um servidor que esteja no seu `PATH` é usado primeiro; senão, o Atlas baixa um servidor fixado e com checksum verificado para `~/.atlas/lsp` na primeira vez que você abre um arquivo daquela linguagem.

O chip da barra de status sempre aparece em um arquivo de texto e sempre diz o porquê. O rótulo dele é o nome e a versão do servidor enquanto ele roda, ou um dos rótulos abaixo, ao lado de um ponto (verde: em execução; âmbar: ocupado ou precisando de atenção; vermelho: falhou; cinza: desligado). Passe o mouse para ler o motivo em uma frase; clique para ver detalhes e ações:

| Chip | O que significa | Ações |
|---|---|---|
| o nome e a versão do servidor (`rust-analyzer 1.2.3`) | o servidor está em execução e autocompletar, diagnósticos, hover e ir para a definição estão ativos; o popover diz se ele vem do seu `PATH`, foi instalado pelo Atlas ou é o seu comando personalizado. Não existe um rótulo "Pronto" à parte | **Reiniciar servidor**, **Abrir log**, **Configurações de linguagens** |
| _Iniciando {servidor}…_ / _Instalando {servidor}…_ | o que está acontecendo, com o progresso quando o servidor informa | **Cancelar instalação** (instalando), **Abrir log** |
| _Sem servidor_ | o Atlas não tem um para esta linguagem; o popover lista os compatíveis | **Configurações de linguagens**, **Como funcionam os servidores de linguagem** |
| _Host antigo_ / _Atualizando host…_ | o `atlas hostd` em execução é anterior aos language servers (veja abaixo); enquanto ele se atualiza ao vivo, não há ações | **Reiniciar o host de terminais**, **Reiniciar depois** |
| _Servidores desativados_ / _Servidor desativado_ | `enable_language_servers` está desligado, ou a linguagem está em `disabled` | **Ativar**, **Configurações de linguagens** |
| _Precisa de Go_ (gopls) / _Precisa de curl_ | falta a toolchain para instalar o servidor | **Copiar comando**, **Verificar novamente**, **Configurações de linguagens** |
| _Sem conexão_ | o download falhou por falta de conexão | **Tentar instalar de novo**, **Configurações de linguagens** |
| _Falha na instalação · {servidor}_ | a instalação falhou; a mensagem diz o motivo | **Tentar instalar de novo**, **Abrir log**, **Configurações de linguagens** |
| _Não instalado · {servidor}_ | o servidor ainda não está instalado | **Instalar**, **Configurações de linguagens** |
| _Parou · {servidor}_ | o servidor parou; as últimas linhas do log dele aparecem | **Reiniciar servidor**, **Abrir log**, **Configurações de linguagens** |
| _Servidor não encontrado_ | `lsp.<language>.command` não está no `PATH` | **Configurações de linguagens**, **Verificar novamente** |
| _Fora de um projeto_ | o arquivo está fora de todas as worktrees abertas, então nenhum servidor o atende | **Como funcionam os servidores de linguagem** |

Apertar F12, F2, ⇧F12 ou ⌘. em um arquivo sem servidor explica o motivo uma vez, com as mesmas ações, em vez de não fazer nada.

### Um host de terminais antigo

O `atlas hostd` sobrevive às atualizações do app, então depois de uma atualização ele pode ser mais velho que o app. Um host anterior aos language servers não consegue atendê-los. O Atlas compara os dois a cada conexão:

- **Um host que aceita upgrade ao vivo** é trocado automaticamente. Seus terminais continuam rodando e os arquivos se reconectam sozinhos.
- **Um host mais antigo com terminais rodando** não consegue transferi-los. O Atlas nunca encerra um terminal sem perguntar: o chip diz _Host antigo_ (o popover: _Os recursos de linguagem precisam de um host de terminais mais novo_) e oferece **Reiniciar o host de terminais**, que pergunta _Reiniciar o host de terminais?_, lista os terminais em execução (`title · folder`) e avisa que eles vão terminar (**Encerrar os terminais e reiniciar**), ou **Reiniciar depois** (a pergunta volta na próxima abertura do app).
- **Nenhum terminal rodando**: o host é reiniciado em silêncio.

O ciclo de vida do próprio host de terminais está documentado em [App desktop](desktop.md#manter-o-host-de-terminais-atualizado).

### Configurar um servidor

Configurações → Editor → **Linguagens** lista todos os servidores com o estado, a versão e se estão no seu `PATH` ou são gerenciados pelo Atlas, e permite instalar, atualizar, desinstalar e reiniciar cada um e abrir o log dele. Para cada linguagem você pode definir um `command`, `args`, `initialization_options` e `settings` personalizados, ou desligá-la.

As mesmas configurações podem ficar no projeto, em `.atlas/settings.json` na raiz da worktree:

```json
{
  "lsp": {
    "typescript": {
      "command": "/opt/tsserver/bin/typescript-language-server",
      "args": ["--stdio"]
    },
    "rust": { "settings": { "rust-analyzer": { "check": { "command": "check" } } } }
  }
}
```

O arquivo do projeto vale mais que o `~/.atlas/desktop/settings.json` para a linguagem que ele cita; veja as regras exatas em [Configurações do desktop](desktop-settings.md#servidores-de-linguagem). Um arquivo inválido é ignorado e informado no chip de status.

Os logs dos servidores ficam em `~/.atlas/lsp/logs/<server>.log`; o **Abrir log** do chip os mostra.

## Edição

Atalhos do macOS (todos reatribuíveis onde o shell é dono da tecla):

| | |
|---|---|
| Vários cursores | ⌥-clique, ⌘D próxima ocorrência, ⌘⇧L todas as ocorrências, ⌥⌘↑ / ⌥⌘↓ cursor acima / abaixo |
| Seleção em coluna | ⌥⇧-arrastar |
| Mover / copiar linha | ⌥↑ ⌥↓ / ⌥⇧↑ ⌥⇧↓ |
| Excluir linha · selecionar linha | ⌘⇧K · ⌘L |
| Comentar | ⌘/ linha, ⌥⇧A bloco |
| Indentar / desfazer indentação | Tab, ⇧Tab, ⌘] ⌘[ |
| Recolher | clique na margem, ⌥⌘[ ⌥⌘], recolher tudo e expandir tudo pela paleta de comandos |
| Ir para a linha | ⌃G |
| Expandir / reduzir a seleção | ⌥⇧→ / ⌥⇧← |
| Quebra de linha | ⌥Z |
| Formatar o documento | ⇧⌥F |
| Salvar · salvar tudo · reverter | ⌘S · ⌥⌘S · Reverter arquivo na barra de ferramentas e na paleta de comandos |

Quando o cursor está dentro de um editor, ⌘/ ⌘[ ⌘] ⌘L e ⌘⇧\ editam o texto. Os comandos do shell que eles normalmente executam (Ajuda, Voltar, Avançar, Logs, Dividir para baixo) continuam na paleta de comandos e nos menus.

Também vêm de fábrica: indentação inteligente, fechamento automático de colchetes, aspas e tags, cores nos pares de colchetes, guias de indentação, réguas (`editor.rulers`), exibição de espaços em branco (`editor.render_whitespace`), destaque do colchete correspondente e da seleção, sticky scroll da função ou classe que contém o cursor e um minimapa opcional (`editor.minimap`, desligado por padrão).

### Buscar e substituir

⌘F abre a busca, ⌥⌘F ou ⌘H acrescenta a substituição. Há alternadores para maiúsculas (Aa), palavra inteira e regex, **na seleção** (⌥⌘L), contador `n of m` (limitado a 10.000), anterior / próximo, selecionar todas as ocorrências (⌥Enter), substituir e substituir tudo, e uma prévia do que a ocorrência atual vira (grupos de regex como `$1` já resolvidos).

### Salvar

O autosave roda 0,5 s depois da última edição (⌘S salva na hora). Ao salvar, o Atlas remove os espaços no fim das linhas e garante uma quebra de linha final (`editor.trim_trailing_whitespace_on_save`, `editor.insert_final_newline_on_save`) e grava o arquivo com o fim de linha dele. O [`.editorconfig`](https://editorconfig.org) é respeitado (`indent_style`, `indent_size`, `tab_width`, `end_of_line`, `trim_trailing_whitespace`, `insert_final_newline`) e vale mais que as preferências. A indentação de um arquivo é detectada pelo conteúdo quando não há `.editorconfig`. O histórico de desfazer de um arquivo dura enquanto a aba dele estiver aberta.

### Mudanças feitas fora do editor

O Atlas observa o arquivo aberto. Se ele muda no disco e você não tem edições não salvas, o editor recarrega (`editor.auto_reload`), mantendo o cursor. Com edições não salvas, ele pergunta: **Carregar do disco** ou **Manter minha versão** (que grava a sua por cima); o autosave pausa nesse meio-tempo para nunca sobrescrever a outra mudança em silêncio. Um arquivo excluído é informado, e salvar o grava de novo. Todo salvamento também confere o arquivo com o que o editor leu ou gravou por último: se alguém o mudou antes, nada é gravado (fechar a aba então mostra o toast "Não salvo: `<file>` mudou no disco"), enquanto o mesmo texto com um horário de modificação mais novo (`touch`, git, um formatador) não conta como mudança.

## Arquivos grandes

Arquivos de texto de até 32 MB abrem. A partir de `editor.large_file_threshold_bytes` (2 MB por padrão), ou quando uma linha tem mais de 10.000 caracteres (bundles minificados), o editor passa para um modo mais leve e avisa em uma notificação e na barra de status: quebra de linha, cores de colchetes, guias de indentação, sticky scroll, minimapa, correspondências da seleção, inlay hints e recursos de linguagem ficam desligados (a árvore de sintaxe é montada sob demanda em volta do que você vê, como sempre). A partir de cinco vezes o limite, ou de uma linha com mais de 200.000 caracteres, o destaque de sintaxe e o recolhimento de código também ficam desligados.

## Layout

O editor sempre preenche o painel, seja qual for o conteúdo do arquivo: a área de rolagem é o painel inteiro, a rolagem horizontal só aparece quando uma linha é maior que o painel, os números de linha ficam parados na rolagem lateral, e a rolagem nunca move a janela que está atrás.
