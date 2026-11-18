# Como escrever em português do Brasil no Atlas

Leia antes o [`README.md`](README.md): as regras de voz, pontuação e texto lido por agentes valem aqui também. Este arquivo cobre o que é específico do português do Brasil.

O objetivo: o texto deve parecer escrito em português por quem usa a ferramenta, não traduzido do inglês ([Microsoft, guia pt-BR](https://download.microsoft.com/download/8/e/3/8e349e32-9eb9-4b63-9909-7586b94a24dd/por-bra-StyleGuide.pdf): o produto deve ter o "look and feel" de um produto escrito originalmente em português).

## Tratamento e verbos

- **Use "você"**, nunca "o usuário" falando com a própria pessoa e nunca voz passiva para fugir do pronome. Omita o "você" quando o sentido já está claro: `Você pode inserir as informações e depois criar o banco` vira `Insira as informações e crie o banco` (Microsoft).
- **Botões, itens de menu e comandos: infinitivo.** `Salvar`, `Criar worktree`, `Instalar crew server`, `Copiar logs para IA`. É a convenção do [Design System do gov.br](https://www.gov.br/ds/padroes/writing/microcopy?tab=visao-geral) ("Inicie o rótulo com um verbo no infinitivo") e do Windows e do VS Code em pt-BR.
- **Instruções dentro de frases: imperativo.** `Tente novamente.`, `Insira um e-mail no formato nome@provedor.com.`, `` Rode `atlas crew status box`. `` O botão diz a ação, a frase diz o que fazer.
- **Tempos simples.** Presente e pretérito perfeito. Evite o futuro quando o presente serve: `O ícone aparece na barra`, não `O ícone será exibido na barra` (Microsoft, seção 4.1.15).
- **Voz ativa e verbos no lugar de substantivos.** `O Atlas instala o hostd`, não `A instalação do hostd é realizada pelo Atlas`. A [Lei 15.263/2025](https://www2.camara.leg.br/legin/fed/lei/2025/lei-15263-14-novembro-2025-798293-publicacaooriginal-177011-pl.html) (Linguagem Simples, para o setor público) lista as mesmas técnicas: ordem direta, frases curtas, voz ativa, "evitar o uso de substantivos no lugar de verbos".
- **Sem gerundismo.** `Vamos estar enviando` é erro; escreva `Enviamos` ou, melhor, diga quem envia: `O Atlas envia` (Microsoft, "Avoid gerundism").
- **Sem subjuntivo duro.** `Certifique-se de que o hostd esteja rodando` vira `Verifique se o hostd está rodando` (Microsoft, seção 4.1.14).

## Mecânica

- **Caixa de frase em tudo**, inclusive botões e títulos: `Criar worktree`, não `Criar Worktree`. Nomes de meses e dias da semana em minúscula: `16 de novembro de 2026`, `segunda-feira`.
- **Números, datas e moeda pelo formatador da plataforma** (`Intl`, ICU), nunca montados à mão. Resultado esperado em pt-BR:

| Tipo | Formato |
|---|---|
| Número | `1.234.567,89` |
| Unidade | `10 MB` (espaço), `42%` (sem espaço) |
| Moeda | `R$ 1.234,50` (espaço inseparável) |
| Data curta | `16/11/2026` |
| Data longa | `16 de novembro de 2026 às 14:05` |
| Hora | `14:05` (24 h, sem AM/PM) |
| Relativo | `há 5 minutos`, `ontem` |
| Lista | `claude, codex e omp` (sem vírgula antes do "e") |

- **Plural com zero.** No CLDR, o pt-BR trata 0 como singular (`one`), então `{count} arquivo` vira "0 arquivo". Escreva sempre o caso zero por extenso na mensagem: `Nenhum arquivo`.
- **Aspas**: duplas retas `"assim"`, simples dentro de duplas. Para nomes de comandos e arquivos, formatação de código, não aspas.
- **Sem espaço antes de `:`, `?`, `!`.** Sem dois espaços depois do ponto.
- **Abreviações**: `ex.:` em vez de "e.g."; prefira escrever `por exemplo`.
- **Mensagens de erro com frases fixas**, como recomenda a Microsoft: `Não foi possível + infinitivo` para algo que falhou agora, `Não é possível + infinitivo` para algo impossível em geral, `Falha ao + infinitivo` só em rótulos curtos de status.

## Gênero

- Use estratégias que a gramática já tem (Microsoft, seção 3.1): falar com "você", plural (`desenvolvedores`), coletivo (`a equipe`, `a coordenação`), `pessoa`, artigo no lugar do possessivo (`o documento`, não `o documento dele`), reescrita da frase.
- Não use `ele(a)`, `x`, `@` nem pronomes novos: leitores de tela leem mal, e a Lei 15.263/2025 os proíbe na comunicação pública (art. 5º, XI). Reescrever quase sempre resolve: `Bem-vindo(a)!` vira `Boas-vindas` ou sai da tela.
- Crewmates e agentes não são pessoas: o gênero segue a palavra (`o crewmate`, `o agente`).

## Termos: o que fica em inglês

Fica em inglês o que desenvolvedores brasileiros falam em inglês no dia a dia. A prova está em como a comunidade escreve: fóruns da Alura usam "Fiz uma branch e agora não consigo dar merge no main" e "Testar código do pull request antes de fazer o merge" ([1](https://cursos.alura.com.br/forum/topico-fiz-uma-branch-e-agora-nao-consigo-dar-merge-no-main-346144), [2](https://cursos.alura.com.br/forum/topico-testar-codigo-do-pull-request-antes-de-fazer-o-merge-no-github-182789)). A tradução automática de grandes produtos mostra o custo de não decidir: o VS Code em pt-BR usa "branch" e "ramificação", "Commit" e "Confirmar", "mesclagem" e "Pull" nos mesmos menus ([catálogo git do VS Code pt-BR](https://raw.githubusercontent.com/microsoft/vscode-loc/main/i18n/vscode-language-pack-pt-BR/translations/extensions/vscode.git.i18n.json)), e o GitHub Docs mistura "solicitação de pull", "PR", "bifurcação", "ramificação" e "ramo" ([docs.github.com/pt](https://docs.github.com/pt/pull-requests)). O Atlas escolhe uma forma por termo e o `i18n/glossary.yaml` registra.

A fonte dos termos é o `i18n/glossary.yaml`; a tabela explica as decisões:

| Inglês | pt-BR | Gênero e verbo |
|---|---|---|
| branch | branch | a branch; criar uma branch |
| commit | commit | o commit; fazer commit |
| merge | merge | o merge; fazer merge |
| rebase | rebase | o rebase; fazer rebase |
| pull request (PR) | pull request (PR) | o pull request, o PR; abrir um PR |
| push / pull | push / pull | fazer push, fazer pull |
| worktree | worktree | a worktree |
| diff | diff | o diff |
| deploy | deploy | o deploy; fazer deploy |
| build | build | o build |
| log | log | o log, os logs |
| prompt | prompt | o prompt |
| token | token | o token |
| hook | hook | o hook |
| issue | issue | a issue |
| stash, tag, checkout, fork | iguais | o stash, a tag, o checkout, o fork |
| crewmate | crewmate | o crewmate (nome do produto) |
| crew server | crew server | o crew server (nome do produto) |
| terminal | terminal | o terminal |
| shell | shell | o shell |
| repository | repositório | o repositório |
| settings | configurações | as configurações |
| file / folder | arquivo / pasta | |
| task | tarefa | |
| automation | automação | |
| service | serviço | |
| port | porta | |
| pane / tab / window | painel / aba / janela | |
| run (button) | executar | rodar só em texto corrido de docs |
| `delete` / `remove` | excluir / remover | excluir apaga; remover tira da lista |
| search | pesquisar | |
| download / upload | baixar / enviar | |
| sign in / sign out | entrar / sair | |
| retry | tentar novamente | |
| approve / approval mode | aprovar / modo de aprovação | |

Nas telas, use `fazer commit`, `fazer merge`, `fazer push`, `fazer deploy`. Formas como "commitar", "mergear", "buildar" e "deployar" são comuns na fala, mas soam informais demais na interface.

Nomes de produto nunca mudam: Atlas, Atlas Crew, Atlas Desktop, Atlas Mobile, hostd, Claude Code, Codex, omp.

## Português do Brasil, não de Portugal

Modelos treinados com texto de Portugal deixam escapar formas de lá. O lint rejeita as mais comuns:

| pt-PT | pt-BR |
|---|---|
| `ecrã` | tela |
| `ficheiro` | arquivo |
| `utilizador` | usuário |
| `palavra-passe` | senha |
| `definições` | configurações |
| `partilhar` | compartilhar |
| `aceder` | acessar |
| `equipa` | equipe |
| `registo` | registro |
| `contacto`, `facto`, `projecto`, `secção` | contato, fato, projeto, seção |
| `carregue no botão` | clique no botão |
| `está a executar` | está executando |

## Decalques do inglês

| Evite | Use | Motivo |
|---|---|---|
| `endereçar o problema` | resolver, tratar | "address" não é "endereçar" |
| `suporta Linux` | funciona no Linux, é compatível com Linux | sinônimos da própria Microsoft |
| `eventualmente` (= "eventually") | depois, por fim | em português significa "de vez em quando" |
| `assumir que` | supor que | |
| `a nível de` | no nível de, ou corte | |
| `ao invés de` (= "instead of") | em vez de | `ao invés` é "ao contrário" |
| `deletar`, `customizar`, `randômico` | excluir, personalizar, aleatório | |
| `realizar a instalação` | instalar | verbo, não substantivo |
| `no final do dia` | no fim, afinal | decalque de "at the end of the day" |

## Marcas de texto de máquina em português

Os modelos têm vícios próprios em português, e o leitor brasileiro os reconhece. A versão da Folha do estudo da Economist ([2026](https://www1.folha.uol.com.br/mercado/2026/08/como-identificar-textos-gerados-por-ia.shtml)) cita `mergulho profundo`, `aprofundar-se`, `rica tapeçaria`, palavras longas como "significativo" e "consequências", nominalizações e as construções `não X, mas Y`, `não apenas ... mas também` e a regra de três.

| Não escreva | Escreva |
|---|---|
| `Vale ressaltar que o hostd precisa de sudo.` | `O hostd precisa de sudo.` |
| `No cenário atual de desenvolvimento com IA, o Atlas Crew surge como...` | `O Atlas Crew roda agentes no seu servidor Linux, sem o seu laptop.` |
| `Desbloqueie todo o potencial dos seus agentes de forma fluida` | `Rode agentes 24 h por dia no seu servidor.` |
| `Não é apenas um terminal, é uma central completa.` | `Terminal, editor e agentes na mesma janela.` |
| `Em suma, o Atlas potencializa sua jornada de desenvolvimento.` | (apague a frase) |
| `Ops! Algo deu errado 😕` | `Não foi possível iniciar o api: a porta 3000 está em uso (pid 4182).` |
| `Estaremos enviando uma notificação quando terminar.` | `Você recebe uma notificação quando terminar.` |
| `Por favor, insira um e-mail válido.` | `Insira um e-mail no formato nome@provedor.com.` |
| `Você digitou a senha incorretamente.` | `Senha do sudo incorreta. Tente novamente.` |

## Exemplos por superfície

| Superfície | Inglês | pt-BR |
|---|---|---|
| Botão | `Create worktree` | `Criar worktree` |
| Botão destrutivo | `Delete 3 worktrees` | `Excluir 3 worktrees` |
| Toast | `Worktree created` | `Worktree criada` |
| Toast | `Copied 120 lines` | `120 linhas copiadas` |
| Estado vazio | `No tasks yet. Send one below.` | `Nenhuma tarefa ainda. Envie uma abaixo.` |
| Estado vazio | `No projects yet. Add any project folder: Atlas detects how to run it.` | `Nenhum projeto ainda. Adicione qualquer pasta de projeto: o Atlas descobre como rodá-lo.` |
| Erro | `Couldn't reach crew server box: SSH timed out after 10 s.` | `Não foi possível conectar ao crew server box: o SSH não respondeu em 10 s.` |
| Erro de CLI | `error: cannot reach crew server box` / `` help: run `atlas crew status box` `` | `erro: não foi possível conectar ao crew server box` / `` ajuda: rode `atlas crew status box` `` |
| Confirmação | `Uninstall Atlas Crew from box?` | `Desinstalar o Atlas Crew de box?` |
| Notificação | `Waiting for your approval` | `Aguardando sua aprovação` |
| Onboarding | `Step 1 of 3: Connect to your server` | `Etapa 1 de 3: conectar ao seu servidor` |
| Docs (título) | `Install a crew server` | `Instalar um crew server` |

Prefixos `error:`, `warning:`, `help:` e `tip:` (a dica do clap) traduzidos na saída para pessoas (`erro:`, `aviso:`, `ajuda:`, `dica:`) e `caused by:` como `causado por:`; em modo `--json` e nos códigos, nada muda.

## Revisão humana específica do pt-BR

Além da lista do `README.md`:

1. Lê como português escrito por brasileiro? Leia em voz alta.
2. Contrações com "de" e "em" corretas (Microsoft, seção 4.1.10: `Referência do Modelo de Objetos`, não `do Modelo do Objeto`).
3. Concordância com termos em inglês: `a worktree criada`, `o commit feito`.
4. Possessivos demais: `Abra o seu arquivo nas suas configurações` vira `Abra o arquivo nas configurações`.
5. Coube no layout? Texto em português costuma ser mais longo que o inglês.

## Palavras da plataforma

O Atlas roda em macOS, Windows e Linux. Nenhum texto fixa um sistema.

- **Atalhos nunca ficam dentro do texto.** A mensagem leva um marcador (`{key}`) e o código passa o atalho do sistema (`⇧⌘P` no macOS, `Ctrl+Shift+P` no resto). Nos nomes de tecla, `Ctrl`, `Shift` e `Alt` ficam como estão impressos nos teclados; `Space` vira `Espaço`; `Esc`, `Enter`, `Tab`, `Home`, `End`, `PgUp` e `PgDn` ficam iguais.
- **Nomes do sistema vêm dos termos compartilhados** (`os` no glossário), nunca digitados: `seu Mac` / `este PC` / `este computador`; `Finder` / `Explorador de Arquivos` / `gerenciador de arquivos`; `Lixeira`; `Ajustes do Sistema` / `Configurações` / `configurações do sistema`; `Option` / `Alt`; `Return` / `Enter`; `barra de menus` / `bandeja do sistema`.
- **Preposição depende do substantivo.** `no Finder`, `no Explorador de Arquivos`, `na bandeja do sistema`: quando a frase precisa de "em" ou "para" antes do termo, escreva a frase inteira com seleção por `os`, em vez de montar `em` + termo.
- Uma tela que só existe em um sistema (permissão de captura do macOS) pode citar o nome do sistema; marque a chave como `platform: mac`.
- **Menus nativos: o nome do sistema vence a tradução.** A barra de menus do macOS, a bandeja e os diálogos usam a palavra que o sistema usa em pt-BR: `Ajustes…` (macOS), `Configurações…` (Windows), `Preferências…` (GNOME e KDE); `Sair do Atlas` no macOS e `Sair` no resto. Itens que o sistema já tem (`Desfazer`, `Refazer`, `Recortar`, `Copiar`, `Colar`, `Minimizar`, `Zoom`, `Ocultar`, `Serviços`) levam o nome exato do sistema.
- **Caixa de frase também nos menus nativos.** O macOS em pt-BR usa Title Case (`Selecionar Tudo`, `Ocultar Outros`), mas o Windows e o GNOME usam caixa de frase e uma mensagem serve aos três sistemas. A regra do guia vale: `Selecionar tudo`, `Ocultar outros`. O inglês dos mesmos menus está em Title Case (herdado do macOS); isso é um desvio do `en.md`, não uma exceção para o pt-BR.
