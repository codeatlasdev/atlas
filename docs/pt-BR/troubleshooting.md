---
title: "Solução de problemas"
source: troubleshooting.md
source_sha: "40d224e50c65c81086092275d558e41e059d38c0"
translation: machine
---

# Solução de problemas

Comece com `atlas dev --status` (o que está rodando e em que estado) e `atlas version`. Os logs de cada serviço ficam em `<project>/.logs/`.

## Instalar e atualizar

| Sintoma | Causa e correção |
|---|---|
| `atlas: command not found` depois de instalar | Abra um novo shell: o `atlas install` edita o arquivo de inicialização do seu shell. Confira se `~/.atlas/bin` está no `PATH`. |
| O macOS mata o binário (`zsh: killed`) | O binário não está assinado. Os builds de versão são assinados com Developer ID (ad-hoc quando construídos sem os secrets de assinatura, como os forks); para um binário que você mesmo compilou, rode `codesign -f -s - target/release/atlas`. Um download pelo navegador também pode precisar de `xattr -d com.apple.quarantine ./atlas`. |
| O `atlas update` manda rodar `brew upgrade` | O binário está em um prefixo do Homebrew; atualize pelo Homebrew. |
| O `atlas update` se recusa a rodar | Você está usando um build de desenvolvimento vinculado (`atlas link`). Rode `atlas unlink` primeiro. |
| A pergunta de atualização atrapalha | `ATLAS_NO_UPDATE_PROMPT=1`, ou responda `n` (silencia essa versão por 24 horas). |

## `atlas dev` e a stack

| Sintoma | Causa e correção |
|---|---|
| `atlas dev needs an interactive terminal` (saída 2) | O stdin ou o stdout não é um TTY. Em scripts e agentes, use `atlas dev --status`, `--ci`, `--headless` ou `--detach`. |
| ``O `atlas dev` já está rodando (pid N)`` | Uma instância por diretório de projeto. Conecte-se com `atlas dev`, confira com `atlas dev --status` e pare com `atlas down`. |
| `atlas.yaml: services.web.helth: unknown field` | Um erro de digitação. Chaves desconhecidas são erros em qualquer profundidade; a mensagem sugere o nome certo. Rode `atlas config check`. |
| Um serviço fica em `starting` para sempre | Ele espera pela `port`, pela URL de `health` ou pelo `ready_log`. A nota na barra lateral diz qual (`waiting for :3001 (12s)`). Depois de `start_timeout` (padrão 60 s), um aviso é registrado; o serviço continua tentando. Corrija a verificação ou a porta. |
| Um serviço está `failed` com `dependency X failed` | Uma dependência falhou, saiu com código diferente de zero ou desistiu de reiniciar. Corrija o `X` primeiro; os serviços que dependem dele iniciam sozinhos quando as condições valem, ou reinicie-os. |
| `gave up after N restarts` | O limite de reinícios (`max_restarts`, padrão 5) acabou. Leia o log do serviço, corrija a causa e reinicie pelo menu (`m`) ou com `atlas_restart_service`. |
| Um serviço está verde mas não responde | Um serviço sem `port`, `health` ou `ready_log` conta como em execução depois de 2 s vivo. Adicione uma verificação. |
| `EADDRINUSE` | Outro processo segura a porta. O Atlas consulta isso antes de cada início, então o serviço mostra `port N held by PID P (cmd, cwd …): stop it yourself (kill P) or change the service port` (barra lateral, log do serviço, `atlas dev --status`, `port_conflict` do MCP): leia isso em vez do log, depois pare o processo você mesmo (`kill P`) ou mude a `port` do serviço. O Atlas limpa só o que ele mesmo iniciou (a partir de `.logs/.pids`) e nunca mata por porta, nem quando pedem. |
| Um serviço está `unhealthy` | O processo está vivo, mas errou `failure_threshold` (padrão 3) verificações de vida seguidas, cada uma com `health_timeout` (padrão 2 s) de duração: ele travou ou está lento demais. `atlas dev --status`, `atlas_status` e a barra lateral mostram desde quando e a última saída dele (`unhealthy_lines`); leia o log e reinicie. Se a verificação está apertada demais para um servidor de desenvolvimento lento, aumente `health_timeout` ou `failure_threshold`. Defina `on_unhealthy: restart` para o Atlas reiniciar sozinho um serviço travado (com o mesmo limite de `max_restarts`). O estado some sozinho quando o serviço volta a responder. |
| Um serviço está `degraded` | Ele responde à verificação de `health`/`port`, mas a [verificação `ready:`](configuration.md#saúde-e-prontidão) falha, ou a mesma linha de erro se repetiu 3 vezes em 60 s. O motivo está em `probe_error` / na nota abaixo da linha em `atlas dev --status` (`GET /ready → 503`, `connection refused`, `` `cmd` exited 1: … ``, `repeating error: …`). Corrija a dependência que ele aponta; ele volta a `running` quando a verificação passa ou o erro para. Um dependente `healthy` ainda espera por `running`. |
| `atlas.yaml: services.api.ready: set exactly one of http (path or URL) or command` | Um bloco `ready:` precisa de `http` **ou** `command`, não os dois e não nenhum. |
| `ready http '/ready' needs the service port` | Um `ready.http` com só o caminho é resolvido contra a `port` do serviço; adicione `port` ou use uma URL `http://` completa. |
| `EADDRINUSE` | Outro processo segura a porta, em geral um órfão de uma execução anterior. O Atlas limpa só o que ele mesmo iniciou (a partir de `.logs/.pids`) e nunca mata por porta. Encontre o processo com `lsof -iTCP:<port> -sTCP:LISTEN` e pare você mesmo. |
| Navegadores, servidores de desenvolvimento ou caches ficaram rodando ou em disco depois de uma execução de agente, ou o Mac fica lento indexando worktrees | `atlas doctor --hygiene` lista os processos órfãos que o Atlas iniciou, o disco por projeto e as pastas pesadas que o Spotlight indexa; `--fix` encerra os órfãos e libera o que o Atlas possui (nunca `target/`, `node_modules` ou worktrees). O mesmo está em Configurações > Workspaces > Higiene. Veja [sessões e higiene](runtime.md#sessões-e-higiene). |
| O `docker compose` falhou e aparece um alerta de `docker` | O daemon não respondeu em 15 s. Inicie o Docker ou o OrbStack; os serviços do app iniciam mesmo assim. |
| Sair deixa contêineres rodando | É de propósito: o Atlas nunca roda `docker compose down`. Pare-os com `docker compose down`. |
| Toast ``O `pre_command` falhou`` | A instalação das dependências falhou; os serviços iniciam mesmo assim. Leia `.logs/setup.log`. |
| `.logs/atlas.sock` velho depois de uma falha | Inofensivo; a próxima execução o substitui. |
| As cores estão erradas ou o texto não pode ser selecionado | `M` solta o mouse para o seu terminal poder selecionar; senão, segure ⌥ ao arrastar. |

## MCP e agentes

| Sintoma | Causa e correção |
|---|---|
| As ferramentas do atlas não aparecem no agente | O `atlas` não está no `PATH` do agente (apps gráficos muitas vezes não têm o `PATH` do seu shell). Rode `atlas setup <agent> --apply` de novo em um shell onde `which atlas` funcione, para o caminho absoluto ser gravado. No omp: `/mcp test atlas`. |
| `atlas dev is not running; start it in a terminal` | As ferramentas de controle precisam de um executor: `atlas dev`, `atlas dev --detach` ou `--headless`. O `--ci` não oferece controle. |
| `atlas_status` diz `running: false` mas a TUI está aberta | O agente está olhando outro diretório de projeto. Compare `--dir` ou `cwd`. |
| `atlas_wait_ready` é cortado depois de 30 s | O timeout do cliente é menor que `timeout_s`. Mantenha `timeout_s` ≤ 25 ou aumente o timeout do cliente. |
| O agente ainda roda `atlas dev` | O hook do omp carrega no início da sessão: reinicie o omp e confira `omp plugin list`. O `/reload-plugins` atualiza só skills e MCP. |
| O Codex ignora os hooks | Confie neles uma vez com `/hooks` no Codex. |
| ``CONFLITO … existe uma entrada `atlas` que não foi criada pelo atlas`` | Você tem uma entrada de servidor `atlas` própria. Renomeie, ou rode de novo com `--force`. |

A tabela completa está em [Configuração dos agentes](agents.md#solução-de-problemas) e em [MCP](mcp.md#solução-de-problemas).

## Atlas Desktop

| Sintoma | Causa e correção |
|---|---|
| O `atlas desktop` não consegue baixar o app | Confira a conexão e se a versão para a sua versão da CLI existe. `atlas desktop --status` mostra o que está instalado; `atlas desktop --install --latest` reinstala. |
| `bun` ou `node` não encontrado dentro do app (`exit 127: sh: 1: bun: not found` na configuração da stack) | Um app aberto pelo Finder, pelo Dock ou por uma entrada de menu do Linux recebe um ambiente vazio. O Atlas lê o ambiente do seu shell de login na inicialização (como shell de login interativo, então `~/.bashrc`, `~/.zshrc` e `~/.config/fish/config.fish` valem); o erro de configuração mostra o `PATH` que o Atlas usou; Configurações → Integrações → Shell e links → *Shell dos seus projetos* mostra o que ele encontrou e tem **Recarregar agora**. Reinicie uma stack em execução depois de mudar. |
| Os terminais usam o renderizador clássico com a GPU ligada, e as Configurações dizem que o host de terminais em execução é de uma versão mais antiga do Atlas | O daemon é anterior ao app e não pôde ser substituído ao vivo. Clique em **Reiniciar o host de terminais** em Configurações → Terminal quando nenhum terminal estiver ocupado. |
| Linux: sem sons, e o log do app (ou `stderr.log`) tem `GStreamer element autoaudiosink not found` | Os plugins good do GStreamer não estão instalados (o `.deb`, o `.rpm`, o pacote do Arch e o AppImage os trazem; uma instalação feita à mão ou com `--nodeps` não). O Atlas então fica em silêncio em vez de iniciar um dispositivo de áudio que o WebKit não consegue abrir. Instale `gst-plugins-good` (`gstreamer1.0-plugins-good`, `gstreamer1-plugins-good`) e reinicie o Atlas. |
| Linux: a página congelou no primeiro som, `atlas desktop debug eval` nunca responde, e o PipeWire roda sem gerenciador de sessão | Nada roteia os fluxos de áudio (sem WirePlumber nem `pipewire-media-session`), então o sink de áudio do WebKit espera para sempre. O Atlas detecta isso ao iniciar e fica em silêncio; inicie o gerenciador de sessão (`systemctl --user enable --now wireplumber`) e reinicie o Atlas para ter som. |
| Um terminal fica em branco logo depois de abrir | Desligue *Terminal rápido (GPU)* em Configurações → Terminal para comparar com o renderizador xterm.js e abra uma issue com a saída de `atlas desktop debug logs`. |
| Uma worktree não foi apagada junto com a branch | A branch não passou por merge, então o Atlas a mantém (um toast avisa). Apague com `git branch -D` quando tiver certeza. |
| `atlas wt` ou `atlas term` diz que o app não está rodando | Eles usam o `~/.atlas/desktop.sock`; abra o Atlas Desktop primeiro. O `atlas term` recorre ao hostd local quando o app está fechado. |
| As versões do app e da CLI são diferentes | `atlas update` leva as duas para a mais recente; **Atualizar CLI e app**, no app, faz o mesmo. As stacks em execução mantêm a versão delas até `atlas down && atlas dev --detach`. |

### Linux: lentidão, travamentos ou quadros velhos

O Atlas Desktop no Linux pinta pelo WebKitGTK e desenha os terminais com xterm.js. Em algumas configurações, mais comumente com NVIDIA, o WebKitGTK e o driver gráfico não se entendem. O Atlas não define nenhuma variável do WebKit por conta própria (forçá-las ajuda uma máquina e atrasa outra, veja [a página de gráficos no Linux do Tauri](https://tauri.app/develop/debug/linux-graphics)), então teste você mesmo, nesta ordem, uma de cada vez, em um terminal:

```bash
# 0. O que o Atlas viu: sessão, servidores de display, driver NVIDIA e toda variável WEBKIT_/GDK_/GSK_ definida.
grep "linux graphics" ~/.atlas/desktop/app.log | tail -1
# O que o GTK, o WebKit e os processos web dele imprimiram (janela em branco, EGL, GStreamer), também em "Copiar detalhes".
# Fica guardado quando o Atlas inicia pelo lançador ou por `atlas desktop` (2 MiB, depois o anterior vira stderr.log.1);
# em um terminal, é impresso lá.
tail -50 ~/.atlas/desktop/stderr.log

# 1. Wayland + NVIDIA ("Error 71 (Protocol error)", janela em branco): não custa velocidade.
__NV_DISABLE_EXPLICIT_SYNC=1 atlas desktop

# 2. Uma janela piscando ou em branco, "AcceleratedSurfaceDMABuf was unable to construct a complete framebuffer":
WEBKIT_DISABLE_DMABUF_RENDERER=1 atlas desktop

# 3. Último recurso (a janela morre ao redimensionar): desliga a composição acelerada. Lento: toda mudança repinta a janela.
WEBKIT_DISABLE_COMPOSITING_MODE=1 atlas desktop
```

Variáveis que você definiu ficam como estão. Ao reportar, inclua a linha `linux graphics` e qual das três ajudou.

### Depurar um build de debug

```bash
atlas desktop debug info
atlas desktop debug logs --level csp   # violações de CSP
atlas desktop debug shot /tmp/x.png
```

## Reportar um bug

Se a CLI entra em pânico, ela imprime um caminho como `crash-<unix_ms>-<pid>.toml` (os 20 mais novos são mantidos) em `~/Library/Application Support/atlas/crashes/` (`$XDG_DATA_HOME/atlas/crashes` no Linux). Uma falha do próprio processo (uma falha de segmentação dentro do WebKit, de um driver gráfico ou do Atlas) grava `crash-native-<unix_ms>-<pid>.toml` no mesmo diretório, então não depende mais de o `coredumpctl` estar presente. Os relatórios de falha ficam na sua máquina e nunca são enviados. Tokens são ocultados e o seu diretório home aparece como `~`, mas revise um antes de anexá-lo.

O Atlas Desktop avisa na próxima inicialização: se um relatório de falha (ou o host de terminais morrendo sozinho) é mais novo que o último que você dispensou, um cartão no canto inferior direito diz **O Atlas fechou inesperadamente na última vez**, com **Reportar** (a tela de relatório de sempre, com dados ocultados, e nada é enviado até você apertar *Reportar issue*) e **Dispensar**. Dispensar guarda o horário em `~/.atlas/desktop/crash-ack`, então o cartão não volta para essa falha; a primeira execução só olha os últimos 7 dias.

Abra uma issue em [github.com/codeatlasdev/atlas/issues](https://github.com/codeatlasdev/atlas/issues) com a saída completa de `atlas version` (ela nomeia o build exato), o seu SO, o que você rodou e as linhas relevantes de `.logs/<service>.log` (oculte os secrets). Problemas de segurança vão pelo [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md), não por issues públicas.
