---
title: "Atlas Crew"
source: crew.md
source_sha: "ec0b3ad11e88e1fac4d5383ac548d90d47020f5a"
translation: machine
---

# Atlas Crew

O Atlas Crew roda agentes de IA 24 horas por dia em um servidor Linux seu, sem depender do seu laptop. Você instala o `atlas hostd` como um serviço systemd no servidor (um **crew server**); nele você recruta **crewmates**: agentes com nome, função, aparência, fila de tarefas, memória e automações. Os agentes (omp, Claude Code, Codex, OpenCode) rodam na sua própria assinatura ou chave de API; o Atlas Crew nunca chama uma API de modelo.

| Termo | Significado |
|---|---|
| **crew** | tudo o que você roda com o Atlas Crew; as configurações gerais do crew |
| **crew server** | uma máquina Linux que roda crewmates (`atlas crew install`) |
| **crewmate** | um agente com nome, função, engine do agente, modo de aprovação, aparência, memória e tarefas |
| **tarefa** | um prompt em que um crewmate trabalha, uma de cada vez por crewmate |
| **automação** | algo que inicia uma tarefa sozinho: um agendamento, um intervalo, um webhook, um evento do GitHub, uma mudança em arquivo, outro crewmate ou um serviço (`triggers` no protocolo) |

Seções: [Instalar um crew server](#instalar-um-crew-server) · [Fazer backup e mover crewmates](#fazer-backup-e-mover-crewmates) · [Autenticar os agentes](#autenticar-os-agentes) · [Crewmates](#crewmates) · [Tarefas e aprovações](#tarefas-e-aprovações) · [Automações](#automações) · [GitHub](#github) · [Telegram](#telegram) · [Ditado](#ditado) · [Respostas faladas e respostas por voz](#respostas-faladas-e-respostas-por-voz) · [Notificações push](#notificações-push) · [Desktop](#atlas-desktop) · [Mobile](#atlas-mobile) · [Ferramentas MCP](#ferramentas-mcp-para-o-agente) · [Modelo de segurança](#modelo-de-segurança) · [Solução de problemas](#solução-de-problemas) · [Aparência do crewmate](#aparência-do-crewmate) · [Teste end-to-end](#teste-end-to-end-em-systemd-real) · [Mapa do código](#mapa-do-código) · [Referência](#referência)

## Instalar um crew server

```sh
# 1. Uma vez, precisa de sudo no servidor
atlas crew install ubuntu@myserver.example.com

# 2. Autentique cada agente como o usuário crew (veja abaixo)
atlas crew login ubuntu@myserver.example.com claude

# 3. Adicione o servidor no Atlas Desktop (Atlas Crew → Configurações) ou acesse-o pelo Atlas Mobile, por um computador que você pareou
```

Requisitos:

- Um servidor Linux, x86_64 ou aarch64 (arm64: Graviton, Oracle A1, Hetzner CAX), com `systemd` e `sudo` (sudo sem senha quando você roda o comando de um terminal). Qualquer outra arquitetura é recusada, com uma mensagem que diz isso.
- Uma chave pública SSH na sua máquina: a que o próprio `ssh` oferece para aquele host (`ssh -G` lista os `IdentityFile`s do `~/.ssh/config` e os nomes padrão; uma chave com qualquer nome funciona). Ela é procurada antes de qualquer mudança no servidor.
- Acesso à internet no servidor **ou** na sua máquina (a versão é então baixada aqui e enviada por ssh), ou um `atlas` Linux da arquitetura do servidor (veja [Instalação offline](#instalação-offline)).

`atlas crew install <ssh-target>` faz, nesta ordem: uma verificação prévia que não muda nada e aponta **todos** os itens que faltam de uma vez (systemd rodando como PID 1, sudo, curl, tar, git, sha256sum, awk, useradd, ... cada um com o pacote que o contém); coloca o `atlas` em `/usr/local/lib/atlas-crew/atlas.new`; cria o usuário de sistema `atlas-crew` (diretório home `/var/lib/atlas-crew`); liga o binário em `<home>/.atlas/bin/atlas`; escreve e habilita `/etc/systemd/system/atlas-crew.service` (cada unit que a instalação controla, essa e as units auxiliares, é primeiro **desmascarada** se estava mascarada, com uma linha de etapa avisando, e escrita em um arquivo temporário que é movido sobre o caminho, então um symlink no caminho, inclusive uma máscara, é substituído e nunca escrito através dele; se o unmask for recusado, a instalação para e imprime o `sudo systemctl unmask <unit>` para rodar no servidor); troca o binário (um binário que já existia fica como `atlas.prev`), inicia o serviço e **espera até 60 s o hostd responder**, voltando para o `atlas.prev` se ele não responder (um binário que não sobe faz a instalação falhar e aponta para `atlas crew logs`; nunca é reportado como pronto); acrescenta a sua chave pública SSH ao `authorized_keys` do usuário crew. O serviço inicia no boot e reinicia em caso de falha.

### Adicionar um crew server no Atlas Desktop

O Atlas Desktop faz o mesmo no macOS, Linux e Windows, a partir de um formulário (endereço e porta, ou um servidor escolhido do `~/.ssh/config`) com o método de login definido logo de início:

- **Chave SSH** (o padrão quando este computador tem uma). O formulário mostra a chave que o `ssh` vai usar para esse endereço (`ssh -G`: um alias do `~/.ssh/config` tem a sua própria) e **Escolher outra…** abre um seletor de arquivo para qualquer chave privada; escolher `id_work.pub` significa `id_work`. A chave é passada como `-i … -o IdentitiesOnly=yes` a cada chamada ssh da instalação (`atlas crew install --identity <key>`, também em `atlas crew browsers install|status`), e fica salva em `crew-servers.json` (`identity`, um caminho, nunca a chave) para que o túnel, o relay do mobile e as Configurações usem a mesma depois de um reinício. Uma chave sem o `.pub` funciona: a chave pública é derivada dela (`ssh-keygen -y`).
- **Senha.** Você entra uma vez com a senha da conta, o Atlas adiciona a chave pública deste computador ao `authorized_keys` do servidor (uma chave é criada antes quando não há nenhuma; adicionar duas vezes não empilha cópias) e depois instala com a chave, então as conexões seguintes não pedem senha. A senha nunca aparece em linha de comando, no ambiente nem no disco: o `ssh` roda o próprio executável do Atlas Desktop como seu programa `SSH_ASKPASS` (`SSH_ASKPASS_REQUIRE=force`), e esse processo busca a senha em um endpoint local de uso único (um socket Unix em um diretório privado, um named pipe no Windows) que existe só durante aquela chamada ssh e atende no máximo algumas conexões. Ele responde apenas a um prompt de senha, nunca a uma pergunta de passphrase ou de host key. Um helper `.sh` ou `.cmd` foi rejeitado: ele guardaria a senha em um arquivo e não poderia ser um único programa em todos os sistemas operacionais.

Antes de instalar qualquer coisa, o formulário faz uma conexão (`first contact`) e confia em uma host key que este computador nunca viu, como `StrictHostKeyChecking=accept-new`; uma chave que **mudou** é recusada, com o comando que esquece a antiga. Toda falha diz o que é e o que fazer: uma chave recusada nomeia a chave e oferece a senha ou outra chave; um servidor fora do ar, mudo ou com o nome errado diz isso; um servidor que aceita só chaves leva você de volta à chave; um sudo que pede senha a pede; uma unit mascarada mostra a linha `hint:` que o CLI imprimiu (`atlas crew install` escreve `hint: …` no stderr quando conhece a correção). A última linha da própria ferramenta fica em **Detalhes**.

No Windows, `ssh` e `ssh-keygen` vêm do recurso opcional **Cliente OpenSSH** (Configurações → Sistema → Recursos opcionais → Adicionar um recurso, ou `Add-WindowsCapability -Online -Name OpenSSH.Client~~~~0.0.1.0` em um PowerShell de administrador). Sem ele, o formulário avisa, com o comando, em vez de falhar mais tarde.

### Em um servidor que já roda crewmates

Servidores instalados antes da renomeação usavam o nome `atlas-bot`. Nada quebra e nada se perde:

- `atlas bot …` continua funcionando como alias de `atlas crew …` (oculto no `--help`), então os scripts existentes seguem rodando.
- Rode `atlas crew install <ssh-target>` de novo nesse servidor. Ele para, desabilita e remove o antigo `atlas-bot.service`, remove `/usr/local/lib/atlas-bot/`, instala o `atlas-crew.service` e **mantém os seus dados**.
- Se o usuário de sistema `atlas-bot` existir, ele é **reaproveitado**, nunca renomeado nem excluído: o diretório home dele (`/var/lib/atlas-bot`) guarda os seus crewmates, a memória e os segredos. O instalador avisa e imprime uma linha `user: <name>`. Servidores novos recebem `atlas-crew`.
- Na primeira inicialização, o hostd renomeia o armazenamento `<hostd dir>/bots/` para `<hostd dir>/crew/` (e o Atlas Desktop renomeia `~/.atlas/desktop/bot-hosts.json` para `crew-servers.json`). Se o antigo e o novo existirem, o novo vence e o antigo fica intocado, com uma linha de log.

Rode `atlas crew status <ssh-target>` depois para conferir.

### Comandos

| Comando | O que faz |
|---|---|
| `atlas crew install <ssh-target> [--yes] [--port N] [--identity KEY] [--binary PATH] [--browsers] [--voice] [--voice-model small\|base]` | instala ou migra um crew server (acima), x86_64 ou aarch64. `--port` para uma porta SSH fora do padrão; `--identity` entra com essa chave privada (`ssh -i`) e instala a chave pública dela para o usuário crew; `--binary` envia esse `atlas` Linux (da arquitetura do servidor) em vez de baixar a versão publicada correspondente a esta (também lido de `ATLAS_CREW_BINARY`); `--browsers` instala também os navegadores de captura ([Capturas](#capturas)) |
| `atlas crew update <ssh-target> [--version X] [--rollback] [--port N] [--binary PATH]` | coloca uma versão (padrão: a deste CLI) em um crew server, guarda o binário antigo como `atlas.prev`, reinicia, espera o hostd e volta atrás sozinho quando ele não responde; `--rollback` restaura o `atlas.prev` ([Atualizar um crew server](#atualizar-um-crew-server)) |
| `atlas crew uninstall <ssh-target> [--yes] [--port N]` | lista o que vai excluir (a home do crew, o número de crewmates e o tamanho), para e remove o serviço, o usuário crew (e o diretório home dele, ou seja, os seus crewmates) e `/usr/local/lib/atlas-crew`. Em um terminal, oferece uma exportação antes e pede para você digitar o nome do servidor; `--yes` pula as perguntas, mas ainda imprime o que foi excluído e confirma que sumiu; sem terminal e sem `--yes`, recusa. Recusa enquanto o usuário ainda tiver uma sessão de login; nunca encerra processos |
| `atlas crew export <ssh-target> [-o FILE] [--crewmate ID]… [--with-secrets] [--port N]` | grava um arquivo compactado com os crewmates ([Fazer backup e mover crewmates](#fazer-backup-e-mover-crewmates)) |
| `atlas crew import <ssh-target> <FILE> [--rename] [--port N]` | restaura um arquivo compactado em um servidor |
| `atlas crew status <ssh-target> [--port N]` | `systemctl status` do serviço mais `atlas hostd --status` como usuário crew, depois uma linha com a versão do atlas do servidor: atualizada, **atrás** deste CLI (rode `atlas crew update`), à frente ou desconhecida |
| `atlas crew pause <ssh-target> [--port N]` | o kill switch: cancela toda tarefa em execução, segura as filas, ignora as automações. Roda `atlas hostd --crew-pause` no servidor como usuário crew ([Pausar e retomar](#pausar-e-retomar)) |
| `atlas crew resume <ssh-target> [--port N]` | inicia as filas de novo (`atlas hostd --crew-resume`) |
| `atlas crew logs <ssh-target> [-f] [--port N]` | últimas 200 linhas de `journalctl -u atlas-crew`; `-f` acompanha |
| `atlas crew login <ssh-target> <agent> [--port N]` | abre `ssh -t <crew-user>@<host> <agent-binary>` (com o mesmo `PATH` que a verificação de engine do Desktop usa: `~/.local/bin`, `~/.bun/bin`, `~/.npm-global/bin`, então um agente instalado ali é encontrado) para você concluir o login do próprio agente. `<agent>` é `omp`, `claude`, `codex` ou `opencode`; antes, o comando imprime a linha de instalação desse agente |
| `atlas crew pair <ssh-target> [--port N]` | pareia um celular direto com este crew server, de qualquer máquina que acesse o servidor por SSH: imprime um QR code, um código de 6 dígitos e o link (`atlas hostd --mobile-pair` como usuário crew; um código, 120 s). O celular guarda o servidor na lista de computadores, então o crew funciona com o seu computador desligado ([Atlas Mobile](mobile.md#parear-um-crew-server-sem-nenhum-computador-no-meio)) |
| `atlas crew browsers install <ssh-target> [--port N] [--identity KEY]` | instala o chromium e o webkit do Playwright para a ferramenta `capture` ([Capturas](#capturas)); idempotente. Nada do npm roda como root: o sudo é usado só em um `apt-get install` que o atlas monta sozinho |
| `atlas crew browsers status <ssh-target> [--port N] [--identity KEY]` | uma linha por item que falta (node anterior à versão 20 ou ausente, npm, runner, pacote ou versão do Playwright, cada navegador, um navegador que não inicia por falta de bibliotecas); diz que está pronto e sai com 0 quando nada falta, sai com 1 caso contrário |
| `atlas crew voice install <ssh-target> [--model small\|base] [--port N] [--identity KEY]` | compila o whisper.cpp fixado e baixa o modelo dele para o [ditado](#ditado); idempotente. Cada download é conferido contra um sha256 fixado antes de ser usado; o sudo só é usado para um `apt-get install` de cmake, um compilador C++, make e ffmpeg, quando faltam. `atlas crew install --voice` faz o mesmo |
| `atlas crew voice status <ssh-target> [--port N] [--identity KEY]` | uma linha por item que falta (binário, a versão dele, nenhum modelo, ffmpeg, um binário que não inicia); diz que está pronto e sai com 0 quando nada falta |

### Um só caminho SSH

Toda chamada ssh a um crew server (todo comando `atlas crew`, a verificação de engine do Atlas Desktop, a instalação de agente e o terminal de login, o túnel do hostd e o relay que o Atlas Mobile usa) é montada por um único helper, `atlas_core::ssh::SshTarget`, a partir do endereço e da porta do servidor. Uma porta diferente de 22 é informada uma vez: `--port N` em qualquer comando `atlas crew`, ou na instalação no Atlas Desktop, que a guarda junto do servidor em `~/.atlas/desktop/crew-servers.json` (`"port": 2222`; sem `port` vale 22, e uma porta definida no `~/.ssh/config` ainda vale para um servidor adicionado antes de o campo existir). As pastas dos agentes (`~/.local/bin`, `~/.bun/bin`, `~/.opencode/bin`, `~/.npm-global/bin`) entram no `PATH` remoto na verificação, na instalação, no `atlas crew login` e no terminal de login, então um agente instalado pelo próprio instalador é encontrado em um comando ssh não interativo.

Clientes Windows: o OpenSSH para Windows não tem compartilhamento de conexão, então `ControlMaster`/`ControlPath`/`ControlPersist` só são passados no macOS e no Linux (lá cada chamada faz o próprio handshake); um script remoto é enviado sem aspas duplas na linha de comando, que a análise de argumentos do Windows estragaria. A chave SSH que o onboarding procura é a que o `ssh -G` lista para o host (os `IdentityFile`s do `~/.ssh/config` e os nomes padrão), não só `id_ed25519` e `id_rsa`.

### Instalação offline

Se o servidor não consegue baixar a versão (sem internet, ou atrás de um proxy que ele não tem configurado), `atlas crew install` e `atlas crew update` a baixam **na sua máquina** (com o seu `https_proxy` e as configurações do curl), conferem o `.sha256` e enviam o binário por ssh. Só se isso também falhar eles usam um `atlas` Linux que você colocou em `~/.atlas/remote/atlas-x86_64-linux` (x86_64) ou `~/.atlas/remote/atlas-aarch64-linux` (arm64); `--binary` sempre vence. A versão baixada é conferida com o `.sha256` antes de ser instalada.

## Atualizar um crew server

```sh
atlas crew update ubuntu@myserver.example.com                 # para a versão deste CLI
atlas crew update ubuntu@myserver.example.com --version 1.6.0 # para uma versão específica
atlas crew update ubuntu@myserver.example.com --rollback      # volta ao binário anterior à última atualização
```

`atlas crew status <ssh-target>` termina com a versão do atlas do servidor e avisa quando ela está **atrás** deste CLI (ou à frente, ou desconhecida porque o hostd não está rodando). Esse é o sinal para atualizar.

`atlas crew update` precisa do servidor já instalado (`atlas crew install`), de sudo sem senha e do mesmo host Linux x86_64 ou aarch64 da instalação. No servidor, nesta ordem:

1. Baixa o asset da versão para a arquitetura do servidor (`atlas-<version>-x86_64-unknown-linux-gnu.tar.gz` ou `…-aarch64-unknown-linux-gnu.tar.gz`), confere o `.sha256` e o coloca em `/usr/local/lib/atlas-crew/atlas.new`. Sem internet no servidor, envia o binário [offline](#instalação-offline); `--binary PATH` envia esse arquivo (sem verificação de versão nesse caso: qualquer hostd que responda é aceito).
2. Copia o `atlas` em execução para `atlas.prev`, move o `atlas.new` sobre o `atlas` e reinicia o `atlas-crew.service`.
3. Espera até 60 s o `atlas hostd --status` responder como usuário crew.
4. Se o hostd não responder, **restaura o `atlas.prev` e reinicia de novo sozinho**, depois termina com um erro dizendo que voltou atrás (o código de saída do comando é diferente de zero). Se nem o binário antigo subir, ele diz isso e aponta para `atlas crew logs`.

`--rollback` faz os passos 2 e 3 com o `atlas.prev` explicitamente; o binário que ele substitui vira o novo `atlas.prev`, então rodar duas vezes volta ao ponto de partida. Falha com uma mensagem clara quando não há `atlas.prev`.

5. **Atualiza o runner de captura**, depois que o novo hostd sobe, em um servidor que já tem os [navegadores de captura](#capturas) (`<hostd dir>/capture/runner.mjs` existe): `runner.mjs` e `detectors.mjs` são reescritos como usuário crew, do mesmo jeito que o `atlas crew browsers install` os escreve. Sem root, sem npm, sem download de navegador; o pacote e o lock dele não são tocados (eles andam com o `atlas crew browsers install`, que o `atlas crew browsers status` pede quando o Playwright fixado difere). Um servidor sem a pasta de captura fica como está, e o `--rollback` mantém o runner que já tem. Sem esse passo, um hostd novo pediria os [detectores de UI](#detectores-de-ui-temporário) a um runner antigo, não receberia achados e falharia todo cenário do UI gate; se isso acontecer, o erro diz `this server's capture runner predates the UI detectors` e indica a correção (`atlas crew update` ou `atlas crew browsers install`).

Só `/usr/local/lib/atlas-crew/` muda, mais os dois arquivos do runner de captura acima quando existem. A home do crew (crewmates, memória, segredos, `crew.json`) nunca é tocada, e a atualização nunca chama pause nem resume: a flag de pausa fica no `crew.json` e o hostd a lê de novo ao iniciar, então **um crew pausado continua pausado**. A reinicialização é uma reinicialização do daemon: uma tarefa que estava rodando volta ao início da fila e roda de novo, marcada como `resumed` (após 2 reinícios ela falha), e as tarefas na fila são retomadas quando o hostd volta ([O que sobrevive a um reinício](#o-que-sobrevive-a-um-reinício)). Pause o crew antes (`atlas crew pause`) se você não quer uma tarefa cortada no meio e rodada de novo.

### Servidores arm64

Servidores Graviton, Oracle Ampere A1 e Hetzner CAX funcionam do mesmo jeito: `atlas crew install` e `atlas crew update` leem `uname -m` no servidor (`x86_64` ou `aarch64`/`arm64`) e baixam o asset correspondente. Qualquer outra arquitetura é recusada antes de mudar qualquer coisa. O build aarch64 é só o CLI (é o que um crew server roda); ele é compilado no Ubuntu 22.04, então roda em glibc 2.35 ou mais nova (Ubuntu 22.04, Debian 12).

## Fazer backup e mover crewmates

```sh
atlas crew export ubuntu@myserver.example.com                      # atlas-crew-myserver.example.com-<date>.tar.gz aqui
atlas crew export ubuntu@myserver.example.com -o aria.tar.gz --crewmate <id> --with-secrets
atlas crew import ubuntu@newserver.example.com aria.tar.gz [--rename]
```

`export` roda `atlas hostd --crew-export` no servidor como usuário crew e devolve um único `.tar.gz` por SSH. Ele lê os arquivos diretamente, então funciona com o serviço parado, e nunca sobrescreve um arquivo local (`-o` indica outro nome). O comando lê o arquivo compactado de volta e lista os crewmates que há nele. Um servidor cujo `atlas` é anterior a este recurso avisa; rode `atlas crew install <ssh-target>` para atualizá-lo.

| No arquivo compactado | Observações |
|---|---|
| `manifest.json` (sempre o primeiro) | `format` (atualmente `1`), `atlas_version`, `created_at`, se tem segredos, e os ids e nomes dos crewmates |
| `crew.json` | as configurações gerais do crew; `paused` nunca é levado |
| `crew/<id>/crewmate.json` | definição e automações, escritas no formato atual (um servidor instalado como Atlas Bot exporta com os nomes novos) |
| `crew/<id>/memory.md`, `tasks.jsonl`, `usage.jsonl`, `usage-<date>.jsonl.gz` | memória, histórico de tarefas, uso (o arquivo ativo e o histórico rotacionado) |
| `crew/<id>/secrets.json` | **só com `--with-secrets`** |

- **Segredos são opcionais.** Sem `--with-secrets`, o arquivo compactado não tem `secrets.json`, e uma automação de webhook (cujo token é uma credencial) chega com o token apagado e **desabilitada**; crie um novo webhook depois da importação. Com `--with-secrets`, o arquivo guarda credenciais em texto claro: ele é gravado com permissão 0600 e o comando avisa. Proteja-o e exclua-o quando terminar.
- **Nunca exportados:** o checkout `work/`, a sessão salva do agente (`session.json`), os arquivos de "já visto" do GitHub e os arquivos de login dos agentes (ficam fora dos dados do crew). Depois de uma importação, rode `atlas crew login <ssh-target> <agent>` no novo servidor.
- **Entre servidores.** O arquivo compactado é um `tar.gz` simples de texto, então ele se move entre servidores x86_64 e arm64 e entre versões do Atlas que leem o mesmo `format`.

`import` confere o manifesto na sua máquina primeiro, depois envia o arquivo; o servidor não grava nada até todas as verificações passarem.

- **Formato desconhecido.** Um arquivo compactado com um `format` que esta versão não lê é recusado por inteiro ("update atlas").
- **Colisões de id.** Um id de crewmate que já existe no servidor é recusado, e nada é importado. `--rename` dá novos ids aos crewmates em colisão; uma automação que dispara depois de um crewmate e seguia um deles passa a seguir o novo id. As automações importadas começam do zero (`last_fired` apagado).
- **Configurações.** O `crew.json` só é restaurado em um servidor que não tem nenhum: um crew em funcionamento mantém as próprias configurações e o próprio estado de pausa.
- **Caminhos seguros.** Uma entrada que não é `manifest.json`, `crew.json` nem `crew/<id>/<one of the files above>` (um `..`, um caminho absoluto, aninhamento) faz o arquivo compactado inteiro ser recusado.

## Autenticar os agentes

Os agentes são instalados e autenticados como o usuário crew. `atlas crew login <ssh-target> <agent>` imprime a linha oficial de instalação e abre a sessão de login:

| Agente | Linha de instalação (rode como usuário crew) |
|---|---|
| omp | `curl -fsSL https://omp.sh/install \| sh` |
| Claude Code | `curl -fsSL https://claude.ai/install.sh \| bash` |
| Codex | `npm_config_prefix="$HOME/.npm-global" npm install -g @openai/codex` |
| OpenCode | `curl -fsSL https://opencode.ai/install \| bash` |

O `PATH` da unit já inclui `~/.atlas/bin`, `~/.bun/bin`, `~/.local/bin`, `~/.opencode/bin` e `~/.npm-global/bin`. Um crewmate também pode usar um provedor personalizado (o seu próprio gateway) definido nas configurações dele; as chaves vão para o `secrets.json` dele ([Modelo de segurança](#modelo-de-segurança)).

### Entrar pelo celular

Sem nenhum PC ligado, um celular pareado com o crew server autentica um engine por conta própria (Configurações do Crew → **Engine sign-in**, ou **Sign in here** em um card "Sign in to …"). `crew_engine_login {engine}` abre um terminal como usuário crew que roda só o comando de login desse engine, e `crew_engine_status {refresh?}` informa por engine `signed_in`, `needs_signin`, `not_installed` ou `unknown`:

| Engine | Terminal de login | Verificação de status (rode como usuário crew) | Um login expirado aparece como |
|---|---|---|---|
| Claude Code | `claude auth login` | `claude auth status` (código de saída 1 = sem login) | `Please run /login`, `Not logged in`, `Login expired`, `OAuth token has expired`, `Invalid API key` |
| Codex | `codex login --device-auth` (um código para digitar no celular: o servidor não tem navegador) | `codex login status` (código de saída 1, `Not logged in`) | `Please log out and sign in again` (refresh token expirado, reutilizado ou revogado) |
| omp | `omp login` (seletor de provedor) | `omp auth export` (linhas de credenciais) | `No API key for provider: …` |
| OpenCode | `opencode auth login` | `opencode auth list` | `ProviderAuthError`, `Unauthorized` |

Fontes: a documentação e o código dos próprios CLIs (`code.claude.com/docs/en/cli-reference` e `/errors`, `openai/codex` `codex-rs/cli` e `codex-rs/login`, `omp.sh/docs` e `oh-my-pi` `docs/providers.md`, `opencode.ai/docs/cli/providers`). A tabela é `crew/engine_login.rs` (`ENGINES`), a única lista do que um celular pode iniciar: qualquer outro nome é recusado (`unknown engine`), então o escopo que o celular tem (`act`) não vira um shell. Uma tarefa que falha com o texto do próprio engine passa o engine para `needs_signin` sem executar nada, e todo cliente recebe `{"ev":"crew_engine_auth","engine","state"}`. O terminal de login é auditado (`engine_login_started`, com quem pediu), nunca as teclas digitadas. Atrás do relay de um Mac, o status funciona e o login não (terminais nunca passam pelo relay): pareie o celular com o crew server.

## Crewmates

Você gerencia crewmates pelo Atlas Desktop, pelo Atlas Mobile, pelo Telegram ou pelo protocolo do hostd ([Referência](#referência)). Um crewmate tem:

- **name**: de 1 a 40 caracteres.
- **role**: até 2000 caracteres, adicionado antes de cada tarefa como contexto.
- **agent**: `omp`, `claude`, `codex` ou `opencode`, com modelo e provedor opcionais.
- **mode** (`approval` em versões antigas): `ask`, `edits` ou `bypass`; veja [Modos de permissão](#modos-de-permissão). Onde funciona depende de como o crewmate roda. Em um terminal (sem `engine` definido), o Claude Code e o Codex perguntam pelo hook deles, e o omp por uma extensão de aprovação que o Atlas grava na pasta do crewmate (nega por padrão: um erro ou timeout nega a chamada). Com um `engine` headless, só `claude` e `omp` conseguem perguntar; os engines `codex` e `opencode` rodam sem supervisão, então o servidor aceita só `bypass` para eles (`approval_supported: false` em `crewmate_list`).
- **project**: uma URL Git (clonada uma vez por servidor e baixada com checkout como uma worktree por crewmate) ou um caminho no servidor. Pasta de trabalho padrão: `<crewmate dir>/work`.
- **memory**: um arquivo markdown (até 64 KiB) que o agente lê e edita entre tarefas.
- **look**: veja [Aparência do crewmate](#aparência-do-crewmate).
- **automations**: veja [Automações](#automações). As rotinas agendadas antigas (cron de 5 campos) continuam rodando.

Configurações do crew (`crew_settings_get` / `crew_settings_set`): `short_replies` (ativado por padrão) resume respostas longas, opcionalmente com `short_replies_model`; `max_agents` (padrão 4, no mínimo 1) é quantos agentes o servidor roda ao mesmo tempo; `agent_memory_max` (em bytes, no mínimo 64 MiB, `0` volta ao padrão) é a memória que um agente pode usar ([abaixo](#um-usuário-linux-por-crewmate)); `default_approval` e `allow_bypass` ([Modos de permissão](#modos-de-permissão)). `crew_settings_get` também retorna `memory_cap`: `{available: true, bytes}` ou `{available: false, reason}`. Ficam salvas em `<hostd dir>/crew.json`.

Um novo crewmate começa com uma conversa curta de configuração (projeto, role, login do engine) no chat dele.

## Tarefas e aprovações

Uma tarefa é um prompt (e, opcionalmente, [imagens](#imagens-nas-tarefas)). Origens: `mobile`, `desktop`, `telegram`, `routine`, `trigger`. Um crewmate roda uma tarefa por vez (em um pty ou como turnos de engine headless); as demais ficam na fila. Uma tarefa espera na fila enquanto o crewmate dela roda outra e também enquanto o servidor já roda `max_agents` agentes (a tarefa mostra `waiting_for: "server busy"` e o chat também); uma tarefa que só espera por você (um login do GitHub, um card de ação) não ocupa vaga de agente. O checkout do projeto faz parte da tarefa: cada chamada `git` tem prazo de 120 s e é encerrada junto com os processos que iniciou, o lock de checkout é por repositório (dois repositórios fazem checkout ao mesmo tempo) e cancelar a tarefa durante o checkout a interrompe e nunca inicia o agente. Uma tarefa termina em `done`, `failed` ou `cancelled`, com limite rígido de 2 horas (o agente e os processos que ele iniciou são encerrados ao passar disso). Os prompts têm de 1 a 20000 caracteres (contados como caracteres, não bytes; os maiores são recusados, inclusive pelo Telegram), e nomes e roles têm o mesmo limite. Enquanto trabalha, os passos que o agente reporta aparecem ao vivo no chat, agrupados sob o título que ele mesmo definiu.

Uma tarefa também pode parar e esperar por **você** (`waiting_action`): conectar o GitHub, fornecer um segredo, responder a uma pergunta, escolher uma opção, fazer o login de um engine, decidir sobre um trabalho que as [proteções](#proteções-contra-o-próprio-agente) recusaram ou mostrar uma [tarefa de UI](#tarefas-de-ui-e-prova) que voltou sem prova. As respostas passam por `crewmate_action_answer`; um segredo é gravado no `secrets.json` do crewmate e nunca fica salvo na tarefa. Um agente propõe uma ação terminando o turno com uma linha `ATLAS_ACTION: {...}`.

### Proteções contra o próprio agente

Uma tarefa que termina bem é verificada antes de virar `done` (`runner/guard.rs`), porque "terminei" também é o que um agente diz quando fez as verificações passarem em vez de corrigir o código. As proteções leem a worktree do crewmate e as palavras do próprio agente:

| Proteção | Recusa quando |
|---|---|
| Testes enfraquecidos | O diff contra a base da tarefa (o upstream da branch, senão `origin/HEAD`; trabalho commitado e não commitado contam) remove asserções de um arquivo de teste (`assert!`, `assert_eq!`, `expect(`, `.should`…, contando as removidas contra as adicionadas), exclui um arquivo de teste ou uma função de teste, adiciona um skip (`#[ignore]`, `.skip`, `it.only`, `xit`, `skip:`, `@Skip`) ou afrouxa um limite em qualquer arquivo: `timeout`, `threshold`, `budget`, `max_bytes`, `max_warnings`, `waiver(s)`, `tolerance`, `retries`… aumentados, e `coverage_threshold` / `fail_under` reduzidos. Rust, TypeScript/JavaScript e Dart |
| Arquivos soltos | Um arquivo novo não rastreado tem nome de rascunho (`probe*`, `scratch*`, `tmp*`, `temp*`, `debug*`, `repro*`, `playground*`, `*.orig`, `*.rej`, `*.bak`, `*.log`) ou está em uma pasta `tmp`/`scratch`/`probes`; está na raiz do repositório e o repositório não rastreia nenhum arquivo dessa extensão; ou está em uma pasta nova cuja pasta pai o repositório também não rastreia (uma feature pode adicionar uma pasta ao lado das rastreadas, não uma árvore delas). Um arquivo de código novo ao lado dos irmãos é esperado, e qualquer coisa em `.atlas/` também (arquivos do próprio Atlas, como os anexos de tarefa). Um comando que redireciona ou usa `tee` para `/tmp` ou `/var/tmp` também é recusado: o git não enxerga esses arquivos, as chamadas de ferramenta enxergam |
| Referências inventadas | Um `#N` nos commits da tarefa, na resposta ou em um comando que publica texto (`gh pr`, `gh issue`, `gh api`, `git commit`) não existe no repositório do projeto no GitHub (verificado pela mesma API do GitHub que as automações do GitHub usam) ou não pôde ser verificado |
| Afirmações sem prova | Não é uma recusa: uma frase do resultado que diz que algo foi verificado, testado, passa ou compila é removida, e uma nota diz o que foi removido, a menos que um comando desta tarefa tenha rodado um teste ou uma verificação (`cargo test`, `bun test`, `bun run check`, `flutter test`, `pytest`, `go test`…) e impresso uma saída sem nenhuma falha |

O Atlas descobre o que um comando imprimiu pelos relatos do próprio agente. O `post_tool_use` do Claude Code traz isso (`tool_response`). O omp reporta quando uma chamada termina (`tool_execution_end`: o texto do resultado e se falhou): um crewmate omp em terminal envia pela extensão de status (`atlas setup omp --apply` a reescreve; antes disso o omp ainda reporta os comandos, mas não a saída deles, então suas afirmações contam como sem prova), e um crewmate omp com engine reporta no stream RPC, que o próprio hostd lê. Um resultado que a ferramenta marcou como erro nunca prova uma afirmação, seja qual for a saída. A saída é mantida até os últimos 16 KiB. Uma afirmação cujo comando não tem saída registrada conta como sem prova. Os comandos ficam na memória por tarefa e são descartados quando ela termina; depois de uma reinicialização do servidor a evidência some e a afirmação é julgada sem ela. Engines que não reportam nada por um hook ou por um stream que o hostd lê (um `codex` ou `opencode` headless, ou um `claude` cujos hooks o `atlas setup claude-code` não instalou) não deixam evidência, então as afirmações deles são removidas.

Uma recusa deixa a tarefa parada em `waiting_action` com uma ação `refused`, `{"kind": "refused", "reasons": ["…"]}`, exibida pelo card de ação no desktop e no celular e como uma mensagem do Telegram com os botões **Override** e **Retry with reasons**. Responda `crewmate_action_answer` com `"override"` (aceita o trabalho como está: a tarefa termina `done`) ou `"retry"` (a tarefa roda de novo desde o início com os motivos adicionados ao prompt; você pode anexar imagens a um retry, veja [Tarefas de UI e prova](#tarefas-de-ui-e-prova)). Qualquer outro valor é um erro e a tarefa continua esperando. Uma recusa sobrevive a uma reinicialização como qualquer outro card. Um agente não consegue levantar uma ação `refused` sozinho: uma linha `ATLAS_ACTION` desse tipo é ignorada. A auditoria registra `task_refused` (com os motivos) e `refusal_overridden`.

### Tarefas de UI e prova

Uma tarefa marcada como **`ui`** muda o que as pessoas veem, então só termina `done` com pelo menos **2 imagens que o próprio agente anexou** (a ferramenta `capture` dele, antes e depois, topo e fim da página). Uma imagem que *você* enviou com o prompt nunca conta: a prova é o que o agente olhou. Uma tarefa `ui` que termina com menos fica parada em `waiting_action` com uma ação `prove_ui`, `{"kind": "prove_ui", "reason": "Could not prove it: this is a UI task and the agent attached 1 of 2 captures. …"}`, exibida no mesmo card de uma recusa. O UI gate roda depois das [proteções](#proteções-contra-o-próprio-agente): um trabalho que as proteções recusaram e que você aceitou com `override` ainda precisa se mostrar.

Marque uma tarefa como `ui` com `crewmate_task {ui: true}`, com um `/ui ` no começo da mensagem no campo de mensagem do desktop ou no Telegram (`/ui /task Ana restyle the header`, ou `/ui restyle the header` para um único crewmate), ou deixe o agente fazer isso com a ferramenta MCP `set_ui` quando a tarefa acabar mudando a interface. A marca só liga a regra: um agente não consegue desligá-la e não consegue levantar um card `prove_ui` sozinho (uma linha `ATLAS_ACTION` desse tipo é ignorada). Um `/ui` sem mensagem depois dele não é uma tarefa: o Telegram responde `Usage: /ui <message>, or /ui /task <crewmate-name> <message>` e o campo de mensagem do desktop pede para você adicionar uma mensagem e mantém o que você digitou; nada entra na fila nos dois casos (uma foto com a legenda `/ui` ainda funciona, a imagem é o conteúdo).

O card mostra as capturas do agente (miniaturas que você toca para ampliar) e o motivo, no Atlas Desktop, no Atlas Mobile (o card da tarefa no chat e o Captain's call da aba Crew) e no Telegram (as imagens como um grupo de mídia de no máximo 10, as mais recentes, depois a mensagem com os botões). Responda `crewmate_action_answer` com:

- `"override"`: aceita o trabalho como está; a tarefa termina `done`. Aceitar o card `refused` das proteções não dispensa a prova: uma tarefa `ui` que ainda está sem prova recebe então o card `prove_ui`, e o override desse card é a palavra final. Vale do mesmo jeito para um card que estava esperando quando o hostd reiniciou: depois da reinicialização, `override` é a escolha explícita do dono do crew server (auditada como `refusal_overridden`) e `retry` roda a tarefa de novo e verifica a prova quando ela termina.
- `"retry"`: a tarefa roda de novo desde o início com o motivo no prompt. Você pode anexar imagens novas a um retry (`attachments`, enviadas como as de uma tarefa, para uma captura de tela com a parte quebrada circulada): elas ficam salvas na tarefa como suas, copiadas para a pasta de trabalho do agente e citadas no prompt dele. Imagens com um `override`, ou com a resposta a qualquer outro card, são recusadas e o card continua esperando.

As capturas do agente são os `attachments` da tarefa com `"source": "agent"` (os seus não têm `source`); são salvas pelo mesmo código das suas imagens, então valem as mesmas verificações (png, jpeg, webp ou gif pelos bytes). Um agente pode anexar até 32 por tarefa (uma chamada de `capture` gera até 16); o seu limite continua 4. A auditoria registra `task_unproven_ui` (`task_id`, `reason`, `captures`).

### Imagens nas tarefas

Uma tarefa pode levar até **4 imagens** de no máximo **5 MiB** cada (png, jpeg, webp ou gif), para os casos em que palavras não bastam: uma captura de tela com a parte quebrada circulada, uma foto de uma tela. Envie pelo Atlas Desktop (o **+** no campo de mensagem, ou cole uma captura de tela), pelo Atlas Mobile (o botão de imagem: câmera ou galeria de fotos) ou pelo Telegram (envie uma foto).

- **Os bytes decidem, não quem envia.** O tipo é lido dos primeiros bytes do arquivo: um script chamado `shot.png`, um SVG ou um PDF é recusado com o nome, seja qual for o mime que o cliente declarou. Um arquivo vazio, mais de 4 imagens ou uma acima de 5 MiB recusa a tarefa **inteira** (nada entra na fila, nada fica no disco). O nome do arquivo é reduzido a um único componente de caminho seguro (`../../x`, um caminho absoluto ou um caminho com barra invertida não saem da pasta da tarefa) e tornado único (`shot.png`, `shot-2.png`); a extensão segue os bytes.
- **Onde ficam.** No crew server, sob o crewmate: `attachments/<task id>/<name>` (veja o [layout](#referência)). A cada início da tarefa elas também são copiadas para a pasta de trabalho como `.atlas/attachments/<task id>/<name>`, e essa pasta entra no `info/exclude` local do checkout para que o `git add -A` de um agente nunca faça commit das suas capturas de tela (o `.gitignore` do próprio projeto não é tocado). O prompt informa ao agente os caminhos e manda olhar primeiro; essa linha é escrita pelo hostd e vem antes do texto da tarefa, então um corpo de tarefa hostil não consegue imitá-la.
- **No protocolo.** Uma linha do protocolo tem no máximo 1 MiB nos dois sentidos, então uma imagem de até 256 KiB vai inline em `crewmate_task` (`data_b64`); uma maior é enviada antes com `crewmate_attachment_put` (partes de no máximo 512 KiB, em ordem, o total limitado a 5 MiB) e nomeada pelo id `upload` em `crewmate_task`. Os apps leem uma imagem de volta com `crewmate_attachment`, em partes. Os dois passam pelos canais que já existem (o túnel SSH do desktop; o celular pelo `crew_call` do seu Mac). **O push relay nunca vê uma imagem**: um push leva só o tipo dele.
- **Envios incompletos têm limite.** Um app encerrado no meio do envio ou uma rede que caiu deixa um envio para trás, então o crew server nunca guarda um para sempre: um envio sem parte nova há **1 hora** é excluído (cada parte o renova) no próximo envio daquele crewmate e quando o crew server inicia, e um crewmate guarda no máximo **8 envios incompletos** (o equivalente a 2 tarefas): um envio *novo* além disso é recusado com `too many unfinished uploads (8); send the task or wait`, enquanto continuar um que já existe é sempre permitido. Com o limite de 5 MiB por envio, isso limita o que qualquer cliente pode deixar no disco a 40 MiB por crewmate.
- **Celulares.** Buscar uma imagem é `read`, enviar uma é `act` (o mesmo de enviar uma tarefa), então um celular que recebeu só `act` consegue enviar uma imagem, mas recebe `forbidden: read` quando a conversa pede a imagem de volta, e mostra essa imagem como "Tap to retry". As fotos são reduzidas a no máximo 2048 px e recodificadas com 85% de qualidade no celular antes do envio.

### Aprovações

Com `approval: ask`, uma chamada de ferramenta pausa a tarefa e espera até 30 minutos por `agent_approve {pane, req, allow}`; a chave do painel é `crew:<crewmate id>:<task id>`. A espera só começa quando alguém pode responder agora: um cliente desktop conectado, um celular conectado ou um chat do Telegram vinculado. Só um token de push registrado não conta (o celular pode estar offline). Caso contrário, o prompt é negado na hora, então configure um deles antes de contar com um crewmate `ask`. Claude Code e Codex: rode `atlas setup claude-code|codex --apply` de novo, uma vez, para o hook deles usar o timeout correspondente de 1805 s.

Só um **allow** explícito executa a ferramenta. Qualquer outro fim a bloqueia e diz qual foi: o dono negou, ninguém respondeu a tempo (auditado como `approval_decision` com `decision: deny` e `source: timeout`, e o dono é avisado) ou a própria requisição falhou. Um crewmate `claude` headless segue a mesma regra: um prompt que ninguém respondeu é uma negação ("nobody answered the approval request"), nunca um allow. As esperas formam uma cadeia, cada uma durando mais que a de dentro, para que a mais interna diga o que aconteceu: o hostd espera 30 minutos, a extensão de aprovação do omp 60 s a mais, e o omp 60 s a mais que isso. O omp impõe um limite próprio a todo handler `tool_call` de extensão (`extensionHandlers.toolCallTimeoutMs`, 30 s por padrão, e um `fetch` ao hostd não é um dos diálogos que ele desconta), então o overlay `--config` que o Atlas passa ao omp de cada crewmate o aumenta (`omp-crew.yml` no diretório do crewmate, reescrito no próximo turno); antes disso, as aprovações de um crewmate omp respondidas depois de 30 s falhavam com `omp-approval.ts timed out after 30000ms` mesmo quando o dono permitia. Um omp que não conhece a configuração (anterior à linha 18.4, onde ela existe) mantém os 30 s.

### Modos de permissão

O **modo** de um crewmate é o que o agente dele pode fazer sem perguntar a você, e você o troca enquanto ele trabalha (Atlas Desktop, Atlas Mobile, `/mode` no Telegram ou `crewmate_set_approval {id, mode}`):

| Modo | O que roda sem perguntar |
|---|---|
| `ask` | nada |
| `edits` (Aceitar edições) | edições de arquivo dentro da árvore de trabalho do crewmate |
| `bypass` | toda chamada de ferramenta, exceto as barreiras abaixo |

A troca vale a partir da **próxima chamada de ferramenta**, e uma chamada que já espera é decidida de novo na hora (a auditoria mostra `source: mode` e quem trocou). Só um cliente do dono troca: Desktop, Mobile (escopo `approve`) e o chat do Telegram vinculado do dono; o processo do próprio crewmate (um `atlas` rodando dentro do agente se identifica como `crewmate:<id>`) é recusado com `crew.approval.forbidden`, e as rotas de hook que um token de crewmate abre não têm esse comando. O `crew_settings` tem `default_approval` (o modo em que um novo crewmate começa, padrão `ask`) e `allow_bypass` (padrão `true`): desligado, o Bypass é recusado em todo lugar e todo crewmate nele volta para `ask`. O antigo `approval: ask|auto` ainda é lido (`auto` é `bypass`) e o `crewmate_list` ainda o envia (`auto` exatamente quando `mode` é `bypass`).

**O hostd decide cada chamada.** Um engine que consegue perguntar (`claude`, `omp`) sempre é iniciado em modo ask, de forma explícita, e nunca com flag de bypass: o claude com `--permission-mode default --permission-prompt-tool atlas:approve`, o omp com a extensão de aprovação e nunca `--auto-approve`, então nenhum dotfile o amplia e toda chamada que precisa de decisão chega ao hostd. Para cada chamada o hostd aplica, nesta ordem: as **barreiras**; o **modo atual** do crewmate; as **permissões da tarefa**; senão retém a chamada para uma pessoa, que pode **Permitir uma vez** (a resposta principal), **Permitir nesta tarefa** (`scope: task`: o mesmo tipo de chamada, só nessa tarefa, nunca na próxima; edições de arquivo na árvore contam como um tipo, um comando é o comando exato) ou **Negar**, com um `comment` opcional que é entregue ao agente. Não existe "sempre". Se ninguém responde em 30 minutos, a chamada é negada e o dono é avisado (um push `crewmate_approval_unanswered` e o Telegram); uma chamada sobre a qual ninguém pode ser consultado é negada na hora. Um agente de terminal sem `engine` também não é iniciado com flags de bypass; o hook dele é respondido pela mesma decisão.

**Barreiras que nenhum modo remove** (a chamada fica retida no `bypass` também, e o prompt diz qual barreira): caminhos protegidos (internos do `.git`, `.claude/`, `.omp/`, `.codex/`, `.mcp.json`, arquivos rc de shell, `.env*`, arquivos de chave como `*.pem`, `id_*`, `.ssh/`), verificados no caminho que a ferramenta nomeia e em cada palavra de um comando; e tarefas iniciadas pelo GitHub ou por um webhook (`no_auto`), a menos que a automação defina `allow_auto`. Um caminho está dentro da árvore depois de resolver `..` e links; uma URL ou um caminho `~` nunca está.

**O que o agente recebe.** Um fragmento curto (o modo, o que roda sem perguntar, as barreiras, "se negado, ache um caminho mais seguro ou pergunte ao usuário") vai antes do prompt de todo turno cujo agente não foi avisado do modo atual: o primeiro turno de uma tarefa e de novo quando o modo mudou. O omp também o recebe na hora, como mensagem de direcionamento que a extensão de aprovação injeta com a próxima decisão; o claude, cuja ferramenta de permissão não tem esse canal, recebe no turno seguinte, ou no motivo da próxima negação. Uma negação sempre leva o motivo e o comentário da pessoa (o `message` do claude, o motivo do bloqueio do omp). O comentário de um **allow** chega ao omp como mensagem de direcionamento e ao claude no turno seguinte. Engines que não conseguem perguntar (`codex`, `opencode`) rodam sem perguntar, como antes: `approval_supported` é `false`, só `bypass` é aceito, e as tarefas deles são recusadas quando o crew não permite Bypass.

**Agentes de terminal** (um crewmate sem `engine`) são iniciados do mesmo jeito: um agente que consegue perguntar nunca recebe as flags de bypass, e o modo ask dele é dito de forma explícita pelo `ask_args` em `crates/atlas-core/data/agents.json` (`claude --permission-mode default`), então um `defaultMode` nas configurações do próprio Claude do usuário não amplia o que ele pode fazer sem perguntar. O hook dele é respondido pela mesma decisão de um engine.

### Pausar e retomar

Um crew server tem um kill switch. **Pause** (`crew_pause`) cancela todas as tarefas em execução (estado `cancelled`, erro `paused`) e encerra o processo delas: em uma tarefa em terminal, o terminal é encerrado, e em uma tarefa com `engine` headless (`claude`, `codex`, `opencode`, `omp`) o turno é encerrado junto com tudo que o agente iniciou (o grupo de processos dele, então os processos de ferramenta das chamadas `bash` do omp ou de um build também somem). O mesmo vale ao cancelar uma tarefa e para o limite de 2 horas, que falha a tarefa e encerra o turno. O Pause mantém as tarefas da fila na fila, não inicia nada novo e pula toda automação que dispararia (uma automação pulada não vai para a fila para depois). Tarefas novas ainda entram na fila durante a pausa. **Resume** (`crew_resume`) reinicia as filas.

Acione pela CLI (`atlas crew pause|resume <ssh-target>`), pelo Atlas Desktop (menu do rodapé **Pausar tudo** / **Retomar tudo**, e Configurações → Atlas Crew por servidor) ou pelo Atlas Mobile (aba Crew, exige o escopo `act`). O estado é salvo em `<hostd dir>/crew.json` (`"paused": true`), então reiniciar o serviço mantém o crew pausado. Os clientes leem de `crewmate_list.paused` e do evento de push `crew_paused`; um servidor mais antigo que este recurso não envia nenhum dos dois e aparece como rodando.

### O que sobrevive a um reinício

Reiniciar o serviço (uma atualização, um reboot, uma queda) não perde nada que uma pessoa iniciou:

| A tarefa estava | Depois do reinício |
|---|---|
| `queued` | continua na fila, na mesma ordem |
| `running`, crewmate com engine | de volta ao início da fila, mesmo id, marcada `resumed`; o agente continua a sessão salva com um prompt dizendo que a execução anterior foi interrompida |
| `running`, agente em terminal | de volta ao início da fila, mesmo id, marcada `resumed`; o agente roda a tarefa de novo |
| `waiting_action` | continua esperando com o card; a resposta (`crewmate_action_answer`) ainda a continua |
| segurando uma pergunta `ask_user` | a pergunta é descartada (o agente dela se foi) e um passo avisa |

Uma tarefa retomada leva `resumed: true`, uma contagem `restarts` e um passo "The server restarted; running this task again". Depois de 2 reinícios a tarefa não é retomada de novo: o terceiro a falha com um erro que cita os reinícios, então uma tarefa que derruba o servidor não entra em loop. Um crew pausado não retoma nada até você clicar em **Retomar**. As aprovações de ferramenta pendentes não sobrevivem: o prompt pertencia ao processo do agente que morreu.

## Automações

As automações (`triggers` no protocolo) iniciam uma tarefa sozinhas. Crie uma nas configurações de um crewmate (Automações) ou peça no chat ("toda dia útil às 9, revise os PRs abertos") e o crewmate propõe como um card que você adiciona com um clique.

| Tipo | `type` no JSON | Campos |
|---|---|---|
| Agendamento | `schedule` | `cron`: 5 campos, avaliado no `tz` da automação (UTC quando não definido) |
| Intervalo | `interval` | `every_secs` ≥ 60 |
| Webhook | `webhook` | `token`, chamado como `POST /crew/<crewmate id>/hook/<token>` no hook server (webhooks configurados antes da renomeação continuam funcionando em `/bots/<crewmate id>/hook/<token>`) |
| GitHub | `github` | `repo` (`owner/repo`), `events`: `mention`, `issue_assigned`, `pr_review_comment`, `ci_failed`, `pr_opened`, `pr_review_requested`, `issue_opened`, `release`, `authors`: quem pode iniciar uma tarefa, `handle` (para `mention`) e `assignee` (para `issue_assigned`), veja [GitHub: entrada e saída](#github-entrada-e-saída). Consultado a cada 2 minutos com o `gh` |
| Arquivo | `file` | `path` (observado recursivamente), `glob` opcional (`*.md` corresponde a arquivos Markdown em qualquer profundidade, `docs/*.md` é relativo a `path`; um glob que não é interpretado não corresponde a nada). Uma mudança dentro da própria worktree do crewmate nunca a dispara |
| Após o crewmate | `crewmate` | `from_crewmate` (id), `on`: `done` ou `failed`. Uma tarefa iniciada assim lembra os crewmates que levaram a ela (`chain`); um crewmate que já está nessa cadeia não é iniciado de novo, então dois crewmates que se disparam mutuamente rodam uma rodada e param (auditoria `chain_refused`) |
| Serviço | `service` | `project_dir`, `service`, `on`: `failed` ou `recovered` |

Regras que o servidor aplica:

- Toda automação tem uma flag `enabled`, um `prompt` (pode conter `{{event}}`) e um `tz` IANA opcional (`"America/Sao_Paulo"`); uma zona desconhecida é rejeitada com `unknown timezone: <tz>`.
- `crewmate_triggers_set` substitui a lista inteira. Um `id` existente mantém o `last_fired`; um agendamento ou intervalo **novo** começa de agora, então dispara pela primeira vez no próximo horário real, não no próximo tick de 30 s.
- `next_fire` é calculado pelo servidor a cada `crewmate_list`; um valor enviado pelo cliente é ignorado. Um agendamento cujo cron o parser recusa (salvo sob um parser mais antigo) aparece com `invalid` (a mensagem do parser, também nas rotinas), nunca dispara e é mantido como foi salvo: editar as outras automações não mexe nele, e mudar o cron dele o valida como qualquer um novo.
- **Filtro de autores do GitHub.** Qualquer pessoa que pode abrir uma issue ou um PR em um repositório observado controla o texto do evento, então uma automação do GitHub só dispara para autores cujo `author_association` está em `authors`. O padrão é `["owner", "member", "collaborator"]`; adicione `"contributor"` ou outros só para repositórios em que você confia neles. Eventos de qualquer outra pessoa são ignorados.
- **Tarefas de fora pedem aprovação.** Uma tarefa iniciada por uma automação de **GitHub** ou **webhook** roda com `approval: ask` mesmo em um crewmate `auto`, porque o texto dela vem de fora: uma chamada de ferramenta espera a sua resposta. Defina `allow_auto: true` nessa automação (padrão `false`) para manter `auto`, só onde estranhos não conseguem escrever o evento. As automações de agendamento, intervalo, arquivo, após crewmate e serviço mantêm a aprovação do próprio crewmate, então as rotinas sem supervisão continuam rodando. Um crewmate que não consegue perguntar (um engine `codex` ou `opencode`) se recusa a rodar uma tarefa de GitHub ou webhook sem `allow_auto`.
- **Conteúdo não confiável.** `{{event}}` é texto de fora (um título do GitHub, um corpo de webhook). Antes de chegar ao agente, o hostd o envolve em um bloco de conteúdo não confiável e diz ao agente para não seguir instruções dentro dele. Isso reduz o risco, mas não o elimina, e é por isso que o filtro de autores e o `ask` existem.
- Um crew pausado ignora as automações ([Pausar e retomar](#pausar-e-retomar)).
- `crewmate_task` recebe o `tz` de quem chama; ele é salvo na tarefa e carimbado em qualquer automação proposta nela.
- Um agente propõe uma automação com `ATLAS_ACTION: {"kind":"add_trigger","trigger":{...},"prompt":"...","reason":"..."}`. Uma automação inválida encerra a tarefa com `The automation was invalid: <error>` para o agente tentar de novo. Caso contrário, um card mostra **Adicionar** / **Agora não** (no Atlas Mobile, **Add** / **Not now**), respondido com `crewmate_action_answer {id, task, value: "add"|"skip"}`. Os webhooks não são oferecidos aos agentes.

O Crew board (abaixo) mostra todas as tarefas na fila, em andamento, em espera e concluídas de todos os crewmates.

## GitHub

Crewmates que clonam repositórios privados ou abrem PRs precisam do `gh` com login feito no crew server. No Atlas Desktop, Configurações → Atlas Crew → GitHub inicia um login por código de dispositivo (`crew_github_login_start` e depois `crew_github_login_poll`); o hostd instala o `gh` na home do usuário do crew se ele não existir e configura o git para usá-lo. `crew_github_status` retorna `{installed, login}`, `crew_github_logout` o remove. Uma tarefa que precisa do GitHub espera em uma ação **Conectar o GitHub** e retoma quando o login termina (`crew_github_connect_resume`).

### GitHub App

Em vez do login do `gh`, um crew server pode ter o seu próprio **GitHub App**. Crewmates e o hostd passam então a falar com o GitHub como `<slug>[bot]`, com tokens de instalação de vida curta, limitados aos repositórios que você escolheu, e nenhum login pessoal é compartilhado. O `gh` continua sendo a opção simples: um repositório fora de todas as instalações ainda o usa.

**Criar** (Atlas Desktop, Configurações → Atlas Crew → GitHub → Criar GitHub App; o desktop usa os comandos abaixo). Você escolhe sua conta ou uma organização, o GitHub mostra o app já preenchido, você confirma e depois escolhe os repositórios na página de instalação (`https://github.com/apps/<slug>/installations/new`). O código que o GitHub devolve vale uma hora e funciona uma única vez.

| Comando (só o dono) | O que faz |
|---|---|
| `crew_github_app_manifest {owner_kind: "user" \| "org", org?, workflows?, server_name?}` | o manifesto a enviar ao GitHub e para onde: `{manifest, post_url, server_name}`. O desktop acrescenta `redirect_url` e `state`, porque o listener local dele é o dono desses dois |
| `crew_github_app_convert {code}` | `POST /app-manifests/{code}/conversions`; guarda a chave; responde com o status abaixo |
| `crew_github_app_status` | `{app: null}`, ou `{app: {id, slug, name, owner, permissions, created_at, identity: {name, email}, installations: [{id, account, repository_selection}], installations_error?, webhook: {url, synced, verified_installations, failing}}}`. Nunca traz a chave nem o segredo do webhook. As instalações são lidas do GitHub com o JWT do app, então uma consulta repetida mostra a instalação assim que ela acontece |
| `crew_github_app_remove` | apaga a chave guardada |

As falhas trazem um código: `crew.github_app.exists` (já existe um app; checado antes de gastar o código), `crew.github_app.code_expired` (usado, ou passou de uma hora), `crew.github_app.conversion_failed {status}`, `crew.github_app.none`, `crew.github_app.forbidden` (um celular ou qualquer não dono; o celular também é recusado pelo escopo). Um `org` ou `code` inválido é `hostd.invalid_argument` com `reason: "invalid"`. Fixtures: `apps/desktop/src/lib/__fixtures__/crew-github-app.json`, escrito por um hostd real (`UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::github_app`).

**O manifesto** pede o mínimo que o crew usa: Contents, Pull requests e Issues com escrita; Checks, Actions e Metadata só leitura. Eventos: comentários de issue, issues, pull requests, comentários de revisão, check runs, workflow runs e releases. `public: false` (o app pertence a este servidor). O webhook fica desligado (`active: false`, com a página do projeto como URL obrigatória), a menos que o crew tenha uma URL pública ([Webhooks](#webhooks)); aí ele fica ligado e envia para `<public_url>/github/webhook`. **Workflows é opcional** (`workflows: true` acrescenta `workflows: write`): editar `.github/workflows` exige essa permissão à parte.

**Onde a chave fica.** `<crew dir>/github-app.json`, só do dono (0600), gravado de forma atômica pelo hostd: id do app, client id, slug, chave privada e segredo do webhook. Nunca o `secrets.json` (que é do crewmate), nunca uma variável de ambiente, nunca uma resposta, nunca o log de auditoria. O desktop só carrega o código de uso único. O client secret do GitHub não é guardado.

**Tokens.** O hostd assina o JWT do app (RS256 com `ring`; `iss` é o **client id**, `iat` 60 s no passado, `exp` 9 minutos à frente) e acha a instalação de um repositório (`GET /repos/{owner}/{repo}/installation`; um acerto vale por 10 minutos, uma falha por um). Ele gera um token de instalação por repositório (`repositories: [repo]`, só as permissões mínimas que o app tem) e o mantém só na memória até **5 minutos antes de expirar**. Tokens nunca vão para o disco; a auditoria registra `github_token_minted {repo, expires_at}`, `github_app_created {slug, owner}`, `github_app_removed` e `github_app_fallback {repo, via: "gh"}` uma vez por repositório que nenhuma instalação cobre.

**Quem responde.** Os gatilhos do GitHub, o comentário de progresso, a busca do PR e as guardas passam todos por uma única `GithubApi` (`crew/github/app/api.rs`): um repositório que uma instalação cobre é respondido com o token dela (o login do próprio crew passa a ser `<slug>[bot]`, então os comentários dele nunca iniciam uma tarefa), qualquer outro cai para o `gh`, e um repositório fora de todas as instalações sem login do `gh` falha com a mensagem do próprio `gh`. Se não der para pedir um token ao GitHub, a chamada falha: a identidade nunca é trocada em silêncio para o login do `gh`.

**Git de um crewmate.** O ambiente do agente não leva token. Leva entradas `GIT_CONFIG_*` que fazem o git consultar um credential helper, `atlas crew git-credential`, e `GIT_AUTHOR_*` / `GIT_COMMITTER_*` com o bot (`<slug>[bot]`, `<id>+<slug>[bot]@users.noreply.github.com`). A cada operação que precisa de senha para `https://github.com` o helper pergunta ao hostd pelo endpoint de hook do próprio crewmate (`POST /crew/<id>/git_credential`, aberto só ao token daquele crewmate e só a partir de uma tarefa dele), e o hostd responde `username=x-access-token` com um token do repositório que o git citou. O helper não guarda nada, então um crewmate nunca segura um token além de uma operação, e um push uma hora depois do início da tarefa continua funcionando. Ele só responde `github.com` por https. Um crewmate com um segredo `GITHUB_TOKEN` próprio fica com ele. Um crewmate que é seu próprio usuário Linux nunca recebe o login do `gh` do usuário do crew como segundo helper.

**`gh` em um crewmate.** Um `gh` fica em primeiro no `PATH` do crewmate: um link para o binário `atlas` (`<crew dir>/shims/gh`, colocado no começo do `PATH` de um crewmate que não é o próprio usuário Linux; o `atlas crew install` cria o link no diretório de engines do spawn helper para os demais, então rode-o de novo em um servidor existente). Executado como `gh`, o `atlas` descobre o repositório que o comando usa (`-R`/`--repo`, `GH_REPO`, o `repos/<owner>/<repo>` de um caminho de `gh api`, ou o `origin` do diretório atual nos comandos que agem sobre um repositório), pede ao hostd um token para ele pela mesma rota de hook do helper do git, e roda o `gh` de verdade com `GH_TOKEN` definido só naquele processo filho. O token não fica no ambiente do crewmate nem em disco. Ele roda o `gh` de verdade sem mexer em nada quando não há app, quando nenhuma instalação cobre o repositório, para um comando que não nomeia repositório (`gh auth status`, `gh api user`, `gh api graphql`), para outro host (`GH_HOST`) e quando o crewmate já tem um `GH_TOKEN` ou `GITHUB_TOKEN` próprio.

**Autoria.** Uma tarefa iniciada por um evento do GitHub registra quem o provocou (`origin.requester`). O ambiente do agente passa então a levar `ATLAS_CO_AUTHOR` (`<login> <id>+<login>@users.noreply.github.com`, que não é segredo) e `core.hooksPath` apontando para um diretório que o hostd grava (`<crewmate dir>/githooks`, ou `crew-runtime/githooks` quando isolado; o agente não consegue alterá-lo). O hook `commit-msg` dele acrescenta `Co-authored-by: <requester>` a uma mensagem de commit que não o tenha e depois roda o hook do próprio repositório com esse nome; todos os outros hooks do lado do cliente são um despachante que roda os do repositório (em `.git/hooks`, ou no `core.hooksPath` que o repositório define, como o husky faz). `git commit --no-verify` o ignora, como ignora todo hook de commit. O prompt da tarefa só diz que a identidade já está definida e que um hook acrescenta o trailer.

**Limites.** O checkout e o fetch de um crewmate isolado usam um token de instalação só naquele processo do git (no ambiente limpo do spawn helper), e uma tarefa dessas não espera pelo login do `gh` quando o app cobre o projeto. Um crewmate que não é isolado tem o checkout feito pelo git do próprio hostd, que ainda usa o login do `gh` para clonar. O link do `gh` no diretório de engines é o que um crewmate isolado executa; um servidor instalado antes dele precisa de `atlas crew install` de novo.

### Webhooks

Por padrão, uma automação do GitHub consulta o repositório a cada 2 minutos. Quando o GitHub consegue alcançar o servidor do crew, o webhook do App entrega cada evento na hora. A consulta continua sendo o padrão: os webhooks só ligam quando você dá ao crew uma **URL pública**.

**A URL pública é sua, não do Atlas.** O servidor de hooks escuta só em loopback, e o Atlas nunca o expõe. Coloque a sua própria frente HTTPS na frente dele e encaminhe `POST <public_url>/github/webhook` para `http://127.0.0.1:47631/github/webhook` no servidor (a porta de hooks, veja o [Modelo de segurança](#modelo-de-segurança)): um proxy reverso que você já usa (Caddy, nginx), um Cloudflare Tunnel ou o Tailscale Funnel. Depois configure, como dono: `crew_settings_set {public_url: "https://crew.example.com"}` (vazio desliga os webhooks). Ela precisa ser `https://`, com host e sem credenciais, query ou fragmento, com até 200 caracteres: os payloads do GitHub atravessam a internet. Em seguida o hostd aponta o webhook do App para ela (`PATCH /app/hook/config`, com um segredo que ele cria quando o app não tem um) e `crew_github_app_status` mostra `webhook.synced`.

**Não use em produção:** smee.io (um terceiro vê cada payload) e `gh webhook forward` (uma ferramenta de desenvolvimento, presa ao seu login).

**O que uma entrega atravessa**, cada passo antes do próximo:
1. A assinatura: `X-Hub-Signature-256` precisa ser `sha256=` + o HMAC-SHA256 em hex do corpo **cru** com o segredo do webhook, comparado em tempo constante. Nada é lido, registrado ou consultado antes de ela passar. Uma entrega sem assinatura ou com assinatura errada recebe `401` e uma linha de auditoria `github_webhook_refused {reason, count}` (no máximo uma por motivo a cada minuto, com a contagem desde a anterior). Sem app ou sem segredo, a rota responde `404`.
2. O id da entrega (`X-GitHub-Delivery`) é registrado em `<crew dir>/github-deliveries.jsonl` (os últimos 1000, no disco antes de qualquer trabalho, para um restart não esquecê-los). Uma repetição recebe `200` e não faz nada (`github_webhook_duplicate`).
3. O hostd responde `202` na hora e faz o trabalho numa tarefa própria: o GitHub conta como falha uma resposta que demora mais de 10 s e não a reenvia. Com o crew pausado ele responde `503` e não registra nada, para o GitHub mostrar a falha.

**Mesmo evento, mesma tarefa.** Uma entrega passa por `decide`, a mesma função da consulta, então ela inicia exatamente a tarefa que a consulta iniciaria. O `author_association` do payload responde à checagem de autor (sem consulta REST); autores sem permissão de escrita são ignorados, a menos que a automação permita, como na consulta.

**A consulta passa o bastão por repositório.** Um repositório só deixa de ser consultado quando o webhook o cobre: uma instalação cobre o repositório e já entregou pelo menos um evento com assinatura correta. Os outros repositórios continuam sendo consultados. Se as entregas falharem por 30 minutos, o hook é marcado como falhando e todos os repositórios voltam a ser consultados até chegar uma entrega assinada; a consulta que assume pula o que o webhook já tratou. Estado: `<crew dir>/github-webhook.json` (sem nenhum segredo).

**Entregas perdidas** são reconciliadas a cada 5 minutos: o hostd lê `GET /app/hook/deliveries` e reenvia as que falharam nos últimos 3 dias (no máximo 20 por passada, 5 tentativas cada, com espera a partir de 5 minutos). Uma entrega que falhou num período que a consulta cobriu não é reenviada, então nada dispara duas vezes.

O segredo do webhook fica em `github-app.json`, junto da chave privada (0600), e em nenhum outro lugar: nunca numa resposta, numa linha de log ou numa entrada de auditoria.

### GitHub: entrada e saída

Um crewmate pode receber trabalho do GitHub e responder lá, como o app do Claude no GitHub faz, a partir do seu próprio servidor. Tudo passa pelo `gh` como o usuário do crew, com o login acima, ou pelo [GitHub App](#github-app) nos repositórios que ele cobre; nada é enviado à CodeAtlas.

**Como configurar.** Faça o login do `gh` (acima), depois adicione uma automação de GitHub ao crewmate (Configurações → Automações → GitHub, ou `crewmate_triggers_set`) com o repositório e os eventos que você quer:

```json
{"id": "gh", "enabled": true, "prompt": "Handle this: {{event}}",
 "kind": {"type": "github", "repo": "owner/repo",
          "events": ["mention", "issue_assigned", "pr_review_comment", "ci_failed"],
          "handle": "nami", "assignee": "nami-bot", "authors": ["owner", "member", "collaborator"]}}
```

O crew consulta os eventos do repositório a cada 2 minutos, então um comentário inicia uma tarefa em cerca de 2 minutos.

| Evento | Inicia uma tarefa quando | O prompt leva |
|---|---|---|
| `mention` | `@<handle>` aparece em um comentário de issue ou PR, ou no corpo de uma issue aberta ou editada. O `handle` padrão é o nome do crewmate como slug (`Code Reviewer` → `code-reviewer`); a correspondência é de palavra inteira e ignora maiúsculas (`@namibia` e `a@nami.com` não correspondem) | o número, o título e o corpo da issue ou do PR, e o comentário |
| `issue_assigned` | uma issue é atribuída a `assignee` (um login do GitHub, obrigatório para este evento) | o número, o título e o corpo da issue |
| `pr_review_comment` | alguém comenta no diff de um PR | o PR, o arquivo, a linha e o comentário; a tarefa trabalha na branch desse PR |
| `ci_failed` | uma check run falha | a check, a branch e o PR dela, e o fim do log do job que falhou (`gh run view --log-failed`, no máximo 200 linhas e 32 KiB; sem o log a falha ainda é reportada) |

`pr_opened`, `pr_review_requested`, `issue_opened` e `release` continuam funcionando como antes.

**O que ele publica.** Quando a tarefa inicia, o crewmate comenta "Working on it." na issue ou no PR onde foi chamado, depois **edita esse mesmo comentário**: uma checklist dos passos que o agente reporta (no máximo uma edição a cada 30 segundos, os últimos 15 passos, sem links, imagens nem menções com `@`) e, no fim, o resultado, o link do PR que ele abriu (encontrado com `gh pr list --head <branch>`, para a branch para a qual o agente foi ou a branch do próprio PR), ou o erro se a tarefa falhou ou foi cancelada. Uma tarefa cujo comentário não pode ser publicado (GitHub fora do ar, sem permissão) roda mesmo assim; o relato é feito na medida do possível. Todo comentário começa com um marcador oculto (`<!-- atlas-crew -->`) para nunca iniciar uma tarefa sozinho. Uma falha de CI em uma branch sem PR não tem onde responder, então não publica nada.

**De onde veio uma tarefa.** A tarefa registra isso como campos estruturados, `origin: {repo, kind, number, comment_id, branch, reply_comment_id}` (`kind`: `mention`, `issue_assigned`, `ci_failed`, `pr_review_comment`), em `crewmate_tasks`; nada é extraído de volta do prompt.

**Segurança.**

- O texto de uma issue, de um comentário, de um comentário de review ou de um log é escrito por outras pessoas. Ele chega ao agente só dentro do bloco de conteúdo não confiável ([Automações](#automações)), depois das instruções.
- **`authors` vale para todo mundo cujo texto chega ao prompt:** o autor da issue ou do PR *e* quem comenta. A menção de um collaborator na issue de um estranho não faz nada. O padrão é `["owner", "member", "collaborator"]`.
- O feed de eventos do GitHub não traz `author_association`, então o hostd pergunta ao GitHub pela issue, pelo PR ou pelo comentário. Se essa consulta falha, o evento não é decidido nem marcado como visto: é perguntado de novo na próxima consulta, nunca permitido.
- A branch de uma tarefa de correção de CI ou de comentário de review entra nas instruções ("check out this branch, push there") só quando é um nome de ref git simples (letras, dígitos, `.`, `_`, `/`, `-`); qualquer outro nome fica no bloco não confiável.
- Uma tarefa do GitHub roda com `approval: ask` mesmo em um crewmate `auto`, a menos que a automação defina `allow_auto: true`. Defina só em repositórios onde estranhos não conseguem comentar.
- Os comentários do próprio crew nunca iniciam uma tarefa, pelo login `gh` dele e pelo marcador. O mesmo evento, ou o mesmo comentário visto de novo, nunca inicia uma segunda tarefa (`<crewmate dir>/<automation id>_seen.jsonl`).
- Uma automação nova do GitHub começa no topo do feed: os eventos que o GitHub ainda lista quando ela é criada são marcados como vistos e não iniciam nada (o arquivo seen é criado nesse momento).
- Um evento é gravado no arquivo seen antes de a tarefa ser criada. Se esse arquivo não pode ser gravado, nada dispara e a automação não é consultada de novo por 15 minutos, então um disco sem permissão de escrita nunca repete uma tarefa a cada 2 minutos.
- Um crew pausado ([Pausar e retomar](#pausar-e-retomar)) não consulta o GitHub.

## Telegram

Um bot do Telegram por crew server; todo chat vinculado vê todos os crewmates dele.

1. Crie um bot do Telegram com o [@BotFather](https://t.me/BotFather) e copie o token.
2. No Atlas Desktop, em **Configurações → Atlas Crew**, no servidor (ele precisa estar online), cole o token em **Telegram** e clique em **Salvar** (`crew_telegram_set`). O token é somente de escrita: a linha mostra só se há um token definido e quantos chats estão vinculados (`crew_telegram_status`).
3. Clique em **Vincular chat** para gerar um código de 6 dígitos válido por 10 minutos (`crew_telegram_link`) e envie ao seu bot `/start <code>` no Telegram. Repita para cada chat. **Bloqueio:** o quinto código errado (de qualquer chat) invalida o código ativo, e até o certo passa a ser recusado; clique em **Vincular chat** de novo para gerar outro. Quatro erros de digitação não bloqueiam.
4. **Remover** apaga o token; os chats vinculados deixam de receber mensagens.

Os chats vinculados recebem os resultados das tarefas (truncados em 4000 caracteres) e os pedidos de aprovação como botões Allow / Deny. Comandos:

| Comando | O que faz |
|---|---|
| `/crew` | lista os crewmates e seus agentes |
| `/mode` | lista o [modo de permissão](#modos-de-permissão) de cada crewmate; `/mode [<nome do crewmate>] <ask\|edits\|bypass>` troca o de um (o único crewmate, quando nenhum é nomeado). Só chats vinculados recebem resposta, e a troca é auditada com `telegram:<chat id>` como dispositivo |
| `/task <crewmate name> <text>` | enfileira uma tarefa |
| `/ui <message>` | a mesma mensagem para uma [tarefa de UI](#tarefas-de-ui-e-prova): `/ui /task <name> <text>`, ou `/ui <text>` para um único crewmate; também vale no início da legenda de uma foto |
| texto simples | vai para o único crewmate quando existe exatamente um |
| uma foto | vira uma tarefa com a imagem: a legenda é o prompt (`/task <name> <text>` escolhe o crewmate; sem isso, o único crewmate assume a tarefa; sem legenda: "Look at the attached image."). O maior tamanho da foto no Telegram é baixado pela Bot API (`getFile`), um arquivo acima de 5 MiB é recusado sem ser baixado, e o chat recebe o motivo da recusa. Veja [Imagens nas tarefas](#imagens-nas-tarefas) |
| uma mensagem de voz | só de um chat vinculado: transcrita no crew server e devolvida com os botões **Send as task**, **Edit** e **Cancel**; nada entra na fila antes de Send. Veja [Ditado](#ditado). Enquanto um pedido de permissão ou um card de escolha desse crewmate espera, a mensagem o responde, pelas [regras de resposta por voz](#respostas-por-voz-aos-cards) |

O Telegram vê o token do seu bot e as mensagens acima. Nada vai para a CodeAtlas.

## Ditado

Fale com um crewmate em vez de digitar. Um microfone no campo de mensagem do Atlas Desktop e do Atlas Mobile, e mensagens de voz no Telegram, colocam o que você disse **no campo de mensagem** (ou, no Telegram, em uma mensagem com botões). **Nada chega a um agente até você apertar Enviar**: o ditado só preenche o rascunho, e Esc (Desktop) ou o X (Mobile) restaura o que você tinha. Uma tarefa ditada avisa o agente uma vez: "This request was dictated by voice; expect homophones in identifiers and file names." (`crewmate_task {dictated: true}`); essa linha nunca aparece como palavras suas.

### Onde a fala é reconhecida

| Onde | Engine | Áudio |
|---|---|---|
| Atlas Desktop e Telegram | **whisper.cpp no crew server**, como usuário crew | vai para o seu crew server e para mais nenhum lugar; apagado quando o texto é gerado |
| Atlas Mobile | o reconhecedor do próprio celular, no aparelho | nunca é enviado ao hostd. O reconhecedor do sistema, que pode enviar áudio a um servidor, só é usado se você optar, e aparece identificado como tal |
| Opcional | **seu próprio endpoint compatível com OpenAI** (`POST <base_url>/audio/transcriptions`) com uma chave dos segredos do crew | vai para o host que você nomeou, e os apps avisam: o rótulo do engine sempre o nomeia |

A Web Speech API não é usada (o WebKitGTK não tem, a do WebView2 usa uma nuvem da Microsoft, a do WKWebView é parcial); por isso o desktop captura o áudio nativamente e o crew server reconhece.

**O engine em uso é sempre mostrado** ao lado do microfone (`crew_voice_status` dá o `label`, por exemplo `whisper.cpp (small)`), e cada transcrição diz qual engine a fez. **Um engine que falha é um erro, nunca uma troca silenciosa**: o hostd responde `crew.voice.engine_failed` com o nome do engine e as palavras dele, e não tenta o outro engine, porque quem escolheu "nas minhas máquinas" nunca deve descobrir que o áudio foi para uma nuvem.

### Ligar

A voz fica **desligada** até o dono ligá-la. São duas coisas: o engine no servidor e a chave geral do crew.

```bash
# o engine: compila o whisper.cpp no servidor (alguns minutos) e baixa o modelo
atlas crew voice install me@my-server            # modelo small (padrão); --model base é menor e mais rápido
# ou ao instalar um servidor
atlas crew install me@my-server --voice
```

Depois ligue a chave no Atlas Desktop (**Configurações → Atlas Crew**, no servidor) ou com `crew_voice_set {enabled: true}` (só o dono: Desktop, a CLI e o chat do Telegram vinculado do dono; um celular ou o processo de um crewmate recebe `crew.voice.forbidden`). Com a chave desligada toda transcrição é recusada com `crew.voice.off`; sem engine, `crew.voice.not_installed`. `crew_voice_set {engine}` escolhe `{"kind": "whisper_cpp", "model": "base"|"small"}` ou `{"kind": "openai_compatible", "base_url": "https://…", "model": "whisper-1", "key_secret": "<nome do segredo>"}`. A chave é lida pelo nome nos segredos do crew (`crewmate_secret_set`); ela nunca aparece em uma resposta, no `crew.json` nem na auditoria, e o texto de erro do provedor nunca é mostrado (ele pode repetir a chave). `http://` só é aceito para `127.0.0.1`, `localhost` ou `::1`, então a chave nunca atravessa uma rede em texto claro.

### Como o whisper.cpp é instalado

O whisper.cpp não publica binários para Linux, e `curl | sh` não é opção; então `atlas crew voice install` **compila no seu servidor a versão fixada do código-fonte**. As alternativas eram um binário pronto de terceiros (sem nada contra o que verificá-lo) e um pacote da distro (antigo e ausente em várias); compilar de um tarball conferido por hash é o único caminho em que cada byte é verificável e o resultado combina com a CPU do servidor.

- **Fixado e verificado.** O código-fonte (`WHISPER_SOURCE_URL`, versão 1.9.5) e cada modelo (`ggml-small.bin`, `ggml-base.bin`, baixados em um commit fixo de `ggerganov/whisper.cpp` no Hugging Face) têm um sha256 compilado no atlas (`atlas_host::crew::voice`). O `sha256sum -c` roda antes de qualquer coisa ser descompactada, movida ou executada; uma divergência apaga o arquivo e interrompe a instalação. Os downloads são só https (também nos redirecionamentos, TLS 1.2 ou mais novo). Mudar de versão é editar essas constantes.
- **O usuário crew faz tudo, menos um comando.** Download, descompactação, build com `cmake` (estático, só o `whisper-cli`, no máximo 4 jobs) e o download do modelo rodam como usuário crew; o código do whisper.cpp nunca roda como root. O root roda só `apt-get install -y --no-install-recommends` de nomes de uma lista fixa (`cmake`, `g++`, `make`, `ffmpeg`), e só para o que falta. Um host sem `apt-get` recebe a lista das ferramentas a instalar à mão.
- **Layout.** No diretório do hostd, `<crew home>/.atlas/voice/`: `bin/whisper-cli`, `models/ggml-<nome>.bin`, `VERSION` (a versão de que o binário foi compilado). A instalação é idempotente: um binário da versão fixada e um modelo cujo checksum confere são mantidos; uma versão nova recompila.
- **Multilíngue.** Os dois modelos são os multilíngues, então inglês e português funcionam (`language`: `en`, `pt` ou `auto`). `small` (cerca de 470 MB em disco) é o padrão; `base` (cerca de 140 MB) é mais rápido e mais impreciso. Uma transcrição roda por vez, com prazo de 15 minutos.
- **ffmpeg** só serve para transformar uma mensagem de voz do Telegram (Opus em Ogg) no áudio de 16 kHz que o engine lê.

### O áudio e o limite de 5 minutos

O formato é fixo: um arquivo WAV de **16 kHz, mono, PCM de 16 bits com sinal**, enviado em partes de até 512 KiB (`crewmate_voice_put`), a primeira com um cabeçalho canônico de 44 bytes. Um gravador que ainda grava escreve `0xFFFFFFFF` nos tamanhos; o hostd mede o áudio pelos bytes que recebeu, nunca pelo cabeçalho.

- **No máximo 300 segundos** nesse formato (9.600.000 bytes de PCM). Uma parte que passaria do limite é recusada com `crew.voice.too_long` e o envio mantém o que já tem, então **chegar ao limite ainda transcreve o que foi dito e não descarta nada**. Qualquer coisa fora do formato declarado é `crew.voice.bad_audio` e é removida.
- **Apagado após o uso.** Uma transcrição final apaga o áudio aconteça o que acontecer: o texto, uma falha do engine ou um cliente que sumiu. Um envio que ninguém terminou é apagado após **10 minutos** sem uma parte, e cada início do crew server remove o que uma queda deixou. No máximo 4 ditados inacabados por vez. O áudio nunca é copiado para o diretório de um crewmate, nunca vai para log, nunca vai para a auditoria.
- **Texto parcial.** `crewmate_transcribe {partial: true}` transcreve os últimos 30 segundos até agora, para a prévia esmaecida ao vivo, e deixa o envio aberto; é no melhor esforço e é pulado enquanto o engine está ocupado. Sem `partial` o envio precisa estar completo (`last: true`) e é consumido.

### Vocabulário de código

- **Dicas** vão ao engine como o prompt inicial dele, são automáticas e curtas (no máximo 400 caracteres): o nome do repositório e a branch do projeto, os nomes dos crewmates, e os arquivos e símbolos da tarefa mais recente **deste crewmate**. O texto de tarefa de outro crewmate nunca é usado: só o nome dele, que você fala em voz alta, é uma dica.
- **Substituições** são uma tabela aplicada depois da transcrição, determinística e só por palavras inteiras, nunca dentro de uma URL: "src slash main dot rs" vira `src/main.rs`, "foo underscore bar" vira `foo_bar`. Os padrões cobrem inglês (slash, backslash, dot, underscore, dash, hyphen, colon, double colon, semicolon, at sign) e português (barra, barra invertida, ponto, underline, sublinhado, traço, hífen, dois pontos, ponto e vírgula, arroba); a frase mais longa vence. `crew_voice_set {replacements: [{say, write, lang}]}` acrescenta linhas suas (`lang` é `en`, `pt` ou `any`; uma linha com as mesmas palavras e o mesmo idioma substitui a padrão) e `crew_voice_status` devolve a tabela efetiva. `raw` em uma resposta é o texto antes da tabela, `text` depois dela.

### Mensagens de voz no Telegram

Uma mensagem de voz de um chat vinculado é transcrita pelo engine do crew e devolvida como texto, com o nome do engine e três botões. **Send as task** enfileira uma tarefa (marcada como ditada), **Edit** pede o texto corrigido na sua próxima mensagem e o devolve de novo (ele nunca é enviado sozinho), **Cancel** descarta. Uma mensagem de qualquer outro chat é ignorada como qualquer outra; uma de mais de 20 MB (o limite do `getFile` da Bot API) é recusada sem ser baixada; uma de mais de 5 minutos é transcrita até o limite e o eco avisa. Com vários crewmates, coloque a legenda `/task <nome>` na mensagem. O eco espera na memória por uma hora; um toque depois de um reinício diz que ele não espera mais. O arquivo de áudio é apagado antes de o eco ser enviado.

### Privacidade

Com o whisper.cpp o áudio vai do seu dispositivo para o **seu crew server** e é apagado lá assim que o texto existe; nada vai para a CodeAtlas nem para terceiros. Com um endpoint hospedado ele vai para esse host, sob os termos desse provedor: os apps mostram o rótulo ao lado do microfone e o `crew_voice_status` o nomeia. No Telegram, a própria mensagem de voz passa pelos servidores do Telegram antes de chegar ao crew server, como qualquer mensagem que você envia lá.

## Respostas faladas e respostas por voz

Uso de mãos livres, pelo celular: ouvir a resposta curta de um crewmate e responder aos cards dele por voz onde isso é seguro. A leitura em voz alta acontece **no aparelho** (a voz e a velocidade do sistema, no idioma da resposta); reconhecer o que você diz é o [ditado](#ditado). O hostd é dono das regras que decidem o que uma resposta falada pode fazer, uma vez só, então nenhum cliente consegue enfraquecê-las.

### Respostas faladas

Desligadas por padrão. Um botão por aparelho, no chat de um crewmate, liga a leitura em voz alta das respostas dele; ligar também define `spoken_replies: true` nesse crewmate (`crewmate_update {id, spoken_replies}`, visível em `crewmate_list`), para que o agente dele escreva resumos. Desligar em um aparelho não limpa a flag: ela fica até você desligar nas configurações do crewmate.

- **Enquanto a flag está ligada**, o hostd acrescenta uma linha às instruções do turno: terminar a resposta com `spoken: <uma ou duas frases curtas para ouvir, sem código, sem caminhos, sem markdown>`.
- **Toda tarefa concluída tem `spoken`** (uma string, em `crewmate_tasks` e nos pushes de tarefa), calculada pelo hostd. A última linha `spoken:` do agente (em qualquer caixa, no início de uma linha, com no máximo 300 caracteres) é o resumo, e ela é **removida de toda cópia visível** da resposta: `result`, `reply`, `reply_full`, o Telegram e o comentário do GitHub. Sem ela, `spoken` são as primeiras frases da resposta visível, até 300 caracteres, sem blocos de código, código inline, URLs, marcas de markdown e caminhos de arquivo. Uma resposta que é só código dá `""`: nada é lido.
- A resposta completa sempre continua como texto.

### Respostas por voz aos cards

`agent_approve_voice {pane, req, transcript, lang?}` responde a um pedido de permissão e `crewmate_action_answer_voice {id, task, transcript, lang?}` a um card de escolha. O cliente envia **só o que foi ouvido** (`lang` é `en`, `pt` ou ausente para as duas tabelas); o hostd decide o que isso significa e responde `{verdict, choice?, comment?, reason?}`:

| `verdict` | Significado |
|---|---|
| `allow`, `deny` | decidido, como um toque decidiria (um allow vale **uma vez**, nunca para a tarefa) |
| `choice` | o card foi respondido com `choice`, o texto da opção como o card o tem |
| `comment` | nada foi decidido; `comment` é a transcrição, para o cliente oferecer como comentário a confirmar com um toque |
| `needs_tap` | nada foi decidido; `reason` é `bypass`, `rail`, `destructive`, `untrusted`, ou `null` para um card que não é de escolha |
| `unclear` | nada a ouvir, ou nenhuma opção bateu |

- **Aprovações só aceitam frases exatas.** Depois de tirar espaços, passar para minúsculas, desconsiderar acentos e remover a pontuação final: `allow`, `approve`, `yes allow` / `deny`, `reject`, `no deny`; em português `permitir`, `permite`, `aprovar`, `aprova` / `negar`, `nega`, `recusar`, `recusa`. Qualquer outra coisa, mesmo que contenha uma dessas palavras (`allow but only this file`, `don't allow`, `allow allow`, `não permitir`, `permitir nesta tarefa`), é um `comment`. "Allow, but only once" não é uma aprovação: é oferecida como comentário, e só um toque decide.
- **Algumas aprovações sempre precisam de um toque na tela**, porque são as que você precisa ver: qualquer pedido de um crewmate em **Bypass**; um pedido retido por um **rail** (um caminho protegido, ou uma tarefa iniciada pelo GitHub ou por um webhook); um comando **destrutivo** (`approval::destructive`, baseado em palavras como os rails: `rm -r`/`-rf`, `git push --force`/`-f`/`--force-with-lease`, `git reset --hard`, `git clean -f`, `git branch -D`, `DROP TABLE|DATABASE`, `TRUNCATE`, `mkfs`, `dd of=`, `chmod -R 777`, um redirecionamento para `/dev/sd*`; ele também lê cada string da chamada e um comando dado como lista). Cada recusa é auditada como `approval_voice_refused {reason, transcript, device}`. **Negar por voz é sempre permitido** (negar é seguro), exceto de uma fonte não confiável.
- **Cards de escolha** são respondidos por uma palavra de número ou dígito (`1`, `one`, `um`, `first`, `primeira`), um ordinal (`the second`, `a segunda`) ou o rótulo da opção (sem diferenciar caixa e acentos, o rótulo inteiro ou um prefixo único de pelo menos 3 caracteres). Palavras ambíguas (um rótulo que também é o número de outra opção, dois rótulos com o mesmo prefixo, dois números) são `unclear`: o hostd nunca adivinha. Segredos, logins, recusas, provas e automações são `needs_tap`.
- **Fontes não confiáveis não respondem nada.** Uma transcrição de um chat do Telegram que não é o chat vinculado do dono, ou do processo de um crewmate, é `needs_tap` com `untrusted`. Os clientes do dono (Desktop, um celular com o escopo `approve` para pedidos e `act` para cards) são confiáveis.
- Uma resposta por voz decidida é auditada como um toque, mais `via: "voice"` e a transcrição (no máximo 200 caracteres): `approval_decision` para um pedido, `card_answered` para uma escolha.
- **Telegram:** uma mensagem de voz do chat do dono, enquanto um pedido ou um card de escolha desse crewmate espera, passa pelas mesmas funções e nunca vira tarefa; o chat é informado do que foi decidido, ou do motivo de nada ter sido. Com vários crewmates, ele escolhe o que está esperando (uma legenda `/task <nome>` nomeia um); com mais de uma coisa esperando, pede que você responda no card.

## Notificações push

Quando uma tarefa termina ou um crewmate precisa da sua aprovação, o crew server envia um push para o seu celular.

- **Ainda não está ativo:** o relay de push não foi publicado e o app é compilado sem Firebase, então nenhum celular recebe push hoje; o código abaixo é o que roda quando eles existirem (estado e passos para ativar: [Atlas Mobile → Notificações push](mobile.md#notificações-push)).
- Depois do pareamento, com um build que tem a configuração de push, o registro é automático: o app obtém do relay um **grant** para o seu token FCM (`POST /v1/register`) e registra `{token, grant}` em cada crew server (`push_register`, `push_unregister`). Isso acontece a cada abertura e conexão e sempre que o FCM renova o token.
- O relay em `push.codeatlas.com.br` recebe o token, o grant e o tipo do evento (`task_done` ou `needs_you`), nunca o conteúdo. Celulares conectados naquele momento são ignorados.
- O relay só notifica quem tem o grant do token, então saber um token não basta para enviar um push a um celular. O crew server descarta um token que o relay recusa (`403`) ou que o FCM não tem mais (`410`).
- **Atualização de uma alpha anterior:** um token registrado antes de existirem os grants não tem grant. O crew server o mantém, mas não envia nada para ele; abra o app atualizado uma vez e ele se registra de novo com um grant. Até lá, esse celular não recebe push.
- Para desativar: defina `Environment=ATLAS_PUSH_RELAY=off` na unit e rode `sudo systemctl daemon-reload && sudo systemctl restart atlas-crew`. Ou aponte para um relay próprio (`services/push-relay/`, veja o README dele para `RELAY_GRANT_KEY`); compile o app com `--dart-define=ATLAS_PUSH_RELAY=<url>` para combinar.

## Atlas Desktop

Abra o workspace do Atlas Crew com **⌘⇧B** (comando *Atlas Crew*, atalho reconfigurável). Duas visões, alternadas com **⌘⇧C** ou pela barra na borda superior:

- **Chat**: a lista de crewmates (pesquisa, ⌘1…⌘9 para alternar, estado do servidor no rodapé) e a conversa do crewmate selecionado: tarefas, passos ao vivo, cards de aprovação e de ação embutidos, e um campo de mensagem que enfileira enquanto outra tarefa roda. Uma janela de configurações (Geral, Automações, Engine, Memória, Uso, Auditoria) salva cada campo separadamente. Um servidor pausado mostra um banner (**Crew pausado** ou **Pausado: …** com o nome do servidor) sobre o workspace com um botão **Retomar**; o menu do rodapé tem **Pausar tudo** / **Retomar tudo**.
- **Board**: colunas Na fila, Trabalhando, Precisa de você e Concluídas com todos os crewmates, com filtros por crewmate, origem e texto, um intervalo para as concluídas e um clique que abre o chat naquela tarefa.

Configurações → Atlas Crew guarda os crew servers (destino SSH + nome, listados em `~/.atlas/desktop/crew-servers.json`). O desktop acessa cada servidor por SSH e por um túnel `ssh -L` até o `hostd.sock` dele. O servidor já precisa ter rodado `atlas crew install`; o Atlas Desktop pode executar a instalação para você e pede a senha do sudo pelo stdin (nunca pelo argv).

**Servidores que você já tem no `~/.ssh/config`.** A tela de instalação lista esses servidores em um seletor pesquisável acima do endereço SSH (alias e depois `user@host:port` como o `ssh -G` resolve, então os padrões de `Host *` e as regras `Match` contam). Ao escolher um, o endereço é preenchido como `user@alias`, junto com a porta. O que vai no endereço é o alias, não o nome do host, então o `IdentityFile`, o `Port` e o `ProxyJump` dessa entrada continuam valendo para toda chamada ssh ao servidor, como em `ssh <alias>`. As linhas `Include` são seguidas (globs, relativos a `~/.ssh`, no máximo 16 níveis); padrões `Host` com curinga ou negados, blocos `Match` e aliases que não podem ser um endereço SSH (um espaço, um `-` no início, um usuário que contém `@`) não aparecem na lista. A lista é lida quando a tela abre e roda fora da thread da UI: no máximo oito `ssh -G` ao mesmo tempo, 5 s cada, e nenhum deles se conecta. Sem `~/.ssh/config`, com um arquivo ilegível ou sem `ssh` no PATH, aparece só o campo de texto livre.

**Capturas de tela para trabalho de interface.** A tela de instalação tem uma opção que você ativa por conta própria, desligada por padrão: *Capturas de tela para trabalho de interface (Chromium + WebKit)*, com o motivo (as ferramentas `capture`, UI gate e `locate` precisam dela) e o custo (Node.js 20+ no servidor, cerca de 500 MB de downloads). Marcá-la executa `atlas crew install … --browsers` e adiciona uma etapa ao checklist da instalação. Em um servidor existente, Configurações → Atlas Crew tem a mesma linha (*Capturas de tela para trabalho em UI*): o campo **Endereço SSH de administrador** já traz o endereço com que você instalou (`ubuntu@host` ou `ubuntu@host:2222`): a instalação o salvou como `admin` em `~/.atlas/desktop/crew-servers.json` (`{"target": "ubuntu@host", "port": 2222}`, sem `port` para 22). A instalação dos navegadores roda `sudo -u <crew user>`, que o usuário de serviço do crew com o qual o desktop se conecta não consegue fazer, então ela precisa dessa conta. O campo é editável e uma edição confirmada (sair do campo, Enter ou uma verificação que funcionou) é salva de volta; um endereço digitado pela metade nunca substitui um bom. Um servidor adicionado antes de esse endereço ser salvo, ou adicionado à mão, começa vazio e guarda o que você digitar depois de uma verificação que funcionou. O arquivo guarda um endereço como o `Host` de um `~/.ssh/config`, nunca uma senha ou chave. Pressione **Verificar** (`atlas crew browsers status`: instalado, ou uma linha para cada peça que falta) e **Instalar** (`atlas crew browsers install`, a saída vai para um log). Um servidor sem Node.js 20 ou mais novo avisa isso claramente e o **Instalar** fica desativado até você instalar o Node lá. Quando o sudo pede uma senha, a linha a pede e a envia pelo stdin, como na instalação.

## Atlas Mobile

O celular fala só com os Macs pareados; o hostd do Mac repassa os pedidos para os crew servers da lista do desktop. Detalhes e escopos: [Atlas Mobile → Crew no celular](mobile.md#crew-no-celular).

A aba **Crew** lista todos os crewmates de todos os computadores pareados (ponto de estado, última mensagem, hora). Cada crew server tem um botão **Pause all** e, enquanto está pausado, um banner **Crew paused** com **Resume**. Os crewmates que esperam por você aparecem em um card "needs you" no topo das abas Computers e Crew. Um crewmate abre como um chat (balões de prompt e resposta, passos recolhidos, Allow / Deny embutidos, campo de mensagem que enfileira) e uma tela de perfil: função, aprovação, automações, memória (View / Edit) e exclusão.

Quando você adiciona um Mac no Atlas Desktop, os celulares recebem um push `mobile_invite` (evento `computer_invite`) e mostram uma tela **Pair**; dispensá-la chama `mobile_invite_dismiss`.

## Ferramentas MCP para o agente

O agente de cada crewmate recebe um servidor MCP stdio, `atlas-crew-mcp` (`atlas mcp --crewmate <id>`; `--bot` é um alias oculto), registrado para a tarefa. Ambiente: `ATLAS_HOOK_PORT`, `ATLAS_HOOK_TOKEN`, `ATLAS_CREWMATE_ID`, `ATLAS_PANE_KEY`.

Um engine headless recebe o mesmo servidor, descrito uma vez (`crew/engine/launch.rs`) e renderizado para cada engine, só para aquela execução e nunca no seu repositório: o `claude` lê `<crewmate dir>/mcp-config.json` (`--mcp-config`); o `codex exec` recebe os overrides `-c mcp_servers.atlas.{command,args,env}` mais `default_tools_approval_mode="approve"`, porque o `codex exec` não tem ninguém a quem perguntar e, sem isso, cancelaria toda chamada do servidor ("user cancelled MCP tool call", openai/codex#29857); o `opencode run` recebe a configuração inline em `OPENCODE_CONFIG_CONTENT`, no formato `mcp.<name>` que o opencode 1 e 2 carregam (o 2 lê nativamente, o 1 não consegue ler o `mcp.servers` do 2); o `omp` tem as próprias ferramentas e chega ao hostd pela extensão de aprovação. O processo de cada engine recebe o ambiente `ATLAS_*` de uma única função, definido depois dos segredos do crewmate, então um segredo com um desses nomes (ou `OPENCODE_CONFIG_CONTENT`) nunca substitui o valor do hostd.

| Ferramenta | Uso |
|---|---|
| `ask_user` | publica uma pergunta no Desktop, no Mobile ou no Telegram e bloqueia até ser respondida (até 30 min). Implementada por `POST /crew/<id>/ask` |
| `set_profile` | muda o próprio nome ou a função deste crewmate. Implementada por `POST /crew/<id>/set_profile` |
| `set_ui` | marca esta tarefa como [tarefa de UI](#tarefas-de-ui-e-prova): para ficar pronta, ela precisa de duas capturas feitas pelo agente. Implementada por `POST /crew/<id>/set_ui` |
| `capture` | screenshots de uma URL em navegadores reais, anexados à tarefa ([Capturas](#capturas)). Implementada por `POST /crew/<id>/capture` |
| `locate` | qual arquivo de rota, componente e seletor desenham uma região de um screenshot ([Localizar](#localizar)). Implementada por `POST /crew/<id>/locate` |

| `ui_gate` | o UI gate rápido: captura só o que uma mudança exige, reexecuta só as falhas, reproduz bugs do passado ([UI gate](#ui-gate)). Implementada por `POST /crew/<id>/ui_gate` |
| `approve` | responde a um prompt de permissão do Claude (`--permission-prompt-tool atlas:approve`) |

As rotas de hook ficam no mesmo servidor HTTP local dos hooks do agente e exigem o token dele.

## Um usuário Linux por crewmate

Issue [#200](https://github.com/codeatlasdev/atlas/issues/200). O `atlas crew install` (rode de novo em um servidor existente) configura dois serviços root ativados por socket, ambos `atlas crew-spawn`, ambos acessíveis só pelo usuário crew (modo do socket 0660, grupo = usuário crew e uma checagem de `SO_PEERCRED`):

| serviço | o que faz | limites |
|---|---|---|
| `atlas-crew-spawn` | inicia um binário de engine (`claude`, `codex`, `omp`, `opencode`, pelo nome simples, a partir de um diretório fixo) como `atlas-cm-<id>`, entrega ao hostd o pty ou um socketpair, envia sinais a ele, informa a saída e o diretório de trabalho, encerra o que sobrou; e roda o **git como o crewmate** para o hostd (uma requisição, uma allowlist de subcomandos, veja [git na árvore do agente](#um-usuário-linux-por-crewmate)) | `CAP_SETUID/SETGID/CHOWN/KILL/DAC_OVERRIDE`, sistema de arquivos somente leitura, exceto `/var/lib/atlas-cm`; toda requisição é validada (formato do usuário, binário, cwd dentro do diretório do crewmate, sem `LD_*`/`PATH`/`HOME` no ambiente, lista de sinais) |
| `atlas-crew-provision` | cria e remove o usuário e os diretórios de um crewmate, reescreve as regras de egress | duas operações, sem texto livre (um usuário no formato exato `atlas-cm-<12 hex>`); escreve só em `/etc` e `/var/lib/atlas-cm` |

O hostd continua sendo o usuário crew sem privilégios, sob `NoNewPrivileges`. Ele pede ao helper cada processo e recebe de volta o master do pty por `SCM_RIGHTS`.

A unit do helper não tem `NoNewPrivileges=` (no systemd 255 ele tira o próprio `CAP_SETUID` do helper) nem `MemoryDenyWriteExecute=`; o que o primeiro daria, o helper define em todo processo que inicia (`PR_SET_NO_NEW_PRIVS`, depois da troca de uid). O segundo é um filtro seccomp que todo agente e toda ferramenta que ele roda herdariam, e o V8 do Node aborta sob ele na inicialização (npm, vite e tsc são Node; verificado com Node 26 sob `systemd-run -p MemoryDenyWriteExecute=yes`, enquanto o Bun e os engines nativos iniciam), então só o serviço de provisionamento, que nunca roda um agente, o tem. O `ProtectProc=invisible` (systemd 247) esconde os processos de outros usuários de todos os agentes (o helper mantém `CAP_SYS_PTRACE` para vê-los ao encerrar processos e para ler o diretório de trabalho de uma sessão), então um crewmate não consegue ler o `/proc/<pid>/cmdline` nem o `environ` de outro; o `ProcSubset=pid` não é usado porque o Node lê `/proc/meminfo`. Cada crewmate recebe o próprio `TMPDIR` (`<dir>/tmp`, `0700`) dentro do `/tmp` privado compartilhado do helper, e um terminal é criado com close-on-exec desde o início (`posix_openpt` + `TIOCGPTPEER`), então um fork em outra thread não consegue levá-lo ao processo de outro crewmate. O socket de escuta que o systemd entrega (fd 3) é marcado como close-on-exec: nenhum agente e nenhum hook do git o segura.

Estrutura em `/var/lib/atlas-cm/<id>/`:

| caminho | proprietário | conteúdo |
|---|---|---|
| o próprio `/var/lib/atlas-cm/<id>/` | root, 0711 | nada próprio: o crewmate e o hostd só passam por ele, e ninguém consegue renomear ou substituir o que está dentro. Antes ele pertencia ao crewmate, que podia trocar `memory.md` ou `work` por um link e fazer o serviço root de provisionamento mudar o proprietário ou o modo do arquivo apontado (`/etc/passwd`, o diretório de outro crewmate); agora o serviço recusa um link em qualquer entrada e muda proprietário e modo só pelo arquivo aberto |
| `home/` | crewmate, 0700 | `$HOME`: os logins dos agentes |
| `work/` | crewmate, grupo = o grupo próprio do usuário crew, 2770 (setgid) | o checkout (um clone completo, já que uma worktree apontaria para dentro do home do crew); o hostd escreve nele por esse grupo (os grupos dele são fixados quando ele inicia, então um grupo criado depois para cada crewmate nunca poderia ser um deles) |
| `crew-runtime/` | usuário crew, grupo crewmate, 0750 | o que o agente lê e não pode alterar: configuração do MCP, extensão de aprovação do omp, arquivo do endpoint de hook, configuração do git, o espelho do projeto do hostd (`<project>.git`) |
| `memory.md` | crewmate, grupo crew, 0660 | o agente acrescenta, o hostd edita para os apps (ambos o abrem sem seguir link) |

Outros comportamentos com o isolamento ativado:

- **Endpoint de hook.** O servidor de hook escuta em uma porta de loopback que o firewall nomeia (47631; aleatória quando ocupada, e então os crewmates isolados não conseguem reportar). Cada crewmate tem o próprio token (`<crewmate dir>/hook-token`, no ambiente dele e em `crew-runtime/hook-endpoint.env`) que abre só as rotas dele: toda rota voltada ao agente do próprio id (`ask`, `set_profile`, `capture`, `locate`, `ui_gate`, `set_ui`: a lista única `AGENT_ROUTES` em `agents/hook_auth.rs`) e os eventos de hook dos próprios painéis. A rota de outro crewmate dá `403`, a rota de webhook nunca é aberta a um agente, um token errado dá `401`. O token global não muda e não é dado a agentes isolados.
- **Egress.** Uma chain do nftables por usuário de crewmate (`meta skuid`): respostas, DNS, a porta de hook no loopback, depois um drop no resto do loopback, `10/8`, `172.16/12`, `192.168/16`, `169.254/16` (metadados da nuvem), `100.64/10`, `fc00::/7`, `fe80::/10`, `64:ff9b::/96` e o `64:ff9b:1::/48` de uso local (NAT64, RFC 8215: eles carregam qualquer endereço IPv4, inclusive privados, então em uma rede só IPv6 um crewmate não alcança hosts só IPv4), depois TCP 22/80/443 e então drop. O arquivo é `/etc/atlas-crew/egress.nft`, reaplicado no boot pelo `atlas-crew-egress.service`, que tem `PartOf=` e `After=nftables.service` para que um restart do nftables (o `/etc/nftables.conf` do Debian começa com `flush ruleset`) carregue a tabela de novo junto, e `ReloadPropagatedFrom=nftables.service` com um `ExecReload` que a carrega, então um `systemctl reload nftables` também a carrega. Enquanto a tabela `atlas_crew` não existe, o spawn helper não inicia nada para um crewmate (nem um agente nem uma requisição git; ele checa com `nft list table inet atlas_crew`, e por isso a unit dele mantém `CAP_NET_ADMIN`; um processo de crewmate nunca a tem) e a tarefa falha citando a tabela. A política é a mesma para todo crewmate hoje (`Policy` aceita portas por usuário e destinos extras; ainda não há configuração para isso).
- **Engines** são instalados no sistema inteiro com a linha de instalação própria de cada fornecedor em `/usr/local/lib/atlas-crew/engine-home` e ligados por link em `/usr/local/lib/atlas-crew/engines`, o primeiro diretório que o helper pesquisa. Atualizar um engine é a atualização do próprio fornecedor, rodada ali como root.
- **Criação e exclusão.** O `crewmate_create` cria o usuário primeiro e não salva nada se isso falhar (por exemplo, falta de uids de sistema: o `useradd --system` tem cerca de 900). O `crewmate_delete` encerra os processos dele e remove o usuário e o diretório. Um crewmate que existe sem usuário (importado, ou criado antes da instalação) ganha um quando a primeira tarefa dele começa.
- **Entrar** como o crewmate: `atlas crew login <server> <agent> --crewmate <id>` (por ssh como o seu usuário admin no servidor, onde o hostd do usuário crew abre o terminal de login do engine, a mesma requisição que o login do celular envia; o spawn helper o inicia como o crewmate sob o mesmo confinamento dos agentes dele: sem um shell `sudo -u` próprio do crewmate, então um hook ou servidor MCP que o crewmate plantou no home dele nunca roda fora do sandbox). O registro de hooks (`atlas setup <agent> --apply`) também é por usuário.
- **Entrar pelo celular, por crewmate.** O `crew_engine_login {engine, crewmate}` abre o terminal de login do engine como aquele crewmate pelo helper (no home dele), e o `crew_engine_status {refresh?, crewmate}` roda o comando de status do engine como ele. Sem `crewmate`, valem para o usuário crew, como antes. Em um servidor sem o helper, um `crewmate` é recusado (nunca respondido com o login do usuário crew), e uma tarefa que falha com o erro de autenticação do próprio engine muda a linha daquele crewmate, com `{"ev":"crew_engine_auth","engine","state","crewmate"}`.
- **Um só launcher.** Um turno headless (`claude`, `codex`, `opencode`, `omp` e o resumidor de respostas) é montado uma vez em `engine/launch.rs` (`TurnCtx::command`: diretório de trabalho, segredos do crewmate, depois os `ATLAS_*` do hostd, que vencem) e iniciado por `launch::output` / `launch::rpc`, que o rodam como o usuário crew ou, pelo helper, como o crewmate. Cancelar, pausar e o timeout de 2 h encerram a árvore de processos nos dois casos: o `signal` do helper vai para o grupo de processos da sessão (cada spawn é a própria sessão), e ele nunca usa `reap`, que encerra todos os processos do crewmate, inclusive um agente de terminal.
- **O git na árvore do agente roda como o crewmate, nunca como o hostd.** O checkout e o `.git/config` dele pertencem ao crewmate, e o git executa o que essa configuração nomeia (`core.fsmonitor`, filtros clean/smudge, `core.sshCommand`, credential helpers, `protocol.ext.allow`); `--no-ext-diff` e flags parecidas não os cobrem. O hostd (o usuário crew) é dono do `hostd.token` e lê os segredos de todos os crewmates, então toda chamada git que ele faz na árvore de um crewmate (o `diff`, o `ls-files` e o `log` das proteções, a checagem de repositório do gate, a branch para a resposta no GitHub, o fetch antes de uma tarefa, o clone) é uma requisição `git` ao spawn helper: `git` em um diretório fixo, como o crewmate, com o subcomando de uma lista fixa (`clone fetch pull status rev-parse symbolic-ref remote diff ls-files log merge-base worktree`, nunca uma opção primeiro), em um diretório daquele crewmate, com ambiente fixo, um prazo que encerra o grupo de processos dele e um limite de tamanho na saída (`CrewStore::git_in`, `crew/gitrun.rs`, `spawn/git.rs`). O clone é feito do mesmo modo, então o checkout é do próprio crewmate (ele pode fazer commit e editar todos os arquivos). **O login do GitHub do usuário crew (`gh auth token`) nunca é dado a um processo que roda como um crewmate**: o crewmate controla o checkout, o `~/.gitconfig` e o `/proc/<its git>/environ`, e esse token alcança todos os repositórios que a conta do operador alcança. Um crewmate com um segredo `GITHUB_TOKEN` próprio clona e busca a URL real com esse token (credencial própria, alcance próprio). Qualquer outro recebe um **espelho**: o hostd cria e atualiza um espelho bare do projeto (`refs/heads` e tags) em `crew-runtime/<project>.git` com o login do usuário crew, no próprio processo, com ambiente limpo, sem hooks (`core.hooksPath=/dev/null`, `core.fsmonitor=`) e sem configuração git global nem do sistema (`crew/mirror`), e o crewmate clona e busca `file://<mirror>` sem token nenhum; o `origin` dele busca do espelho e faz push (`remote.origin.pushurl`) para a URL do projeto. O `crew-runtime/` é do usuário crew e `0750` com o grupo do crewmate, então o crewmate pode ler o espelho e não muda nada nele, e um projeto é espelhado por crewmate: outro crewmate não consegue entrar nele. O espelho é atualizado antes de cada tarefa, então um crewmate sem token próprio vê o GitHub como ele estava no início da tarefa. O fetch dele roda no próprio `work/` (nunca em um diretório que o próprio `git worktree list` nomeie), e uma requisição git, como um agente, roda com o leaf de cgroup do crewmate, `no_new_privs`, só o stdio aberto e o diretório de trabalho acessado **depois** da troca de uid (o `Command::current_dir` o acessava como root e seguia qualquer link nele). O hostd não mantém configuração git para a árvore (o `safe.directory` some junto), e tira os screenshots preparados do `git add -A` com um `.gitignore` dentro de `.atlas/attachments/` em vez de editar `.git/info/exclude`. Excluir um crewmate remove o clone dele junto com o usuário, não pela lista de worktrees de um clone compartilhado.
- **Falha fechada.** Se o servidor tem o isolamento instalado (o diretório dos crewmates ou um usuário `atlas-cm-*` existe) e o socket do spawn helper não existe (uma unit de socket parada, um helper que nunca iniciou), o hostd não roda crewmates como o usuário crew: toda tarefa falha com esse motivo e o comando que resolve. Só uma máquina sem nenhum vestígio da instalação roda crewmates do jeito antigo.
- **Migração.** Os crewmates existentes ganham usuários durante a instalação. Os checkouts antigos são renomeados para `work.pre-isolation` (eram worktrees de um clone no home do crew que o usuário do crewmate não consegue ler; a próxima tarefa clona de novo), o `memory.md` muda na primeira tarefa (o arquivo antigo fica como `memory.md.pre-isolation`), e os agentes precisam entrar de novo por crewmate (o login compartilhado deles continua com o usuário crew). Nada é excluído.
- **Um limite de memória por agente** ([#296](https://github.com/codeatlasdev/atlas/issues/296)). A unit do helper delega o cgroup dela (`Delegate=memory pids`, `DelegateSubgroup=helper`; o único caminho de cgroup que ela pode escrever está em `ReadWritePaths=`, e o `ProtectControlGroups=yes` continua) e mantém um leaf por crewmate, `cm-<id>`, com `memory.max` = o limite, `memory.oom.group=1` (o kernel mata a árvore de processos inteira do agente, nunca metade dela), sem swap e `pids.max=4096`. O agente entra no leaf antes de trocar de usuário, então nunca roda sem limite. Um agente que passa do limite morre sozinho: o hostd, os outros crewmates e o servidor continuam de pé, enquanto antes só o `MemoryMax=80%` do serviço inteiro podia pará-lo, levando todo mundo junto. O limite é `agent_memory_max` quando definido; senão, 75% da memória do servidor dividida por `max_agents`, nunca abaixo de 1 GiB (16 GiB e 4 agentes: 3 GiB cada). Mudar `agent_memory_max` ou `max_agents` reescreve o `memory.max` dos leaves que existem, e o hostd informa o limite ao helper antes de cada agente iniciar, então um restart do helper nunca deixa agentes com um limite desatualizado. Quando o kernel mata um agente, a tarefa dele falha com **"ran out of memory (cap 3 GiB); raise agent_memory_max"** em vez de um código de saída simples (o helper compara a contagem `oom_kill` do `memory.events` do leaf antes e depois da sessão). Sem cgroup v2, sem delegação (systemd anterior ao 236, ou uma unit que a instalação não reescreveu), um Mac ou um helper sem root significa sem limite, uma linha no journal do helper e `memory_cap: {available: false, reason}`; os agentes continuam rodando. O `atlas crew install` reescreve as units e faz `try-restart` em um helper em execução (os agentes sobrevivem: `KillMode=process`), então rode-o de novo para ter o limite em um servidor existente. Os agentes agora vivem no cgroup do helper, não sob a unit do hostd, então o `MemoryMax=80%` do `atlas-crew.service` não os conta mais; os limites por agente contam.
- **A exportação** acompanha a memória para o diretório do crewmate. Os logins dos agentes e os checkouts nunca foram exportados.

Não coberto: o `ProtectProc=invisible` exige systemd 247 **e** Linux 5.8 (`hidepid=invisible`); em systemd anterior ao 247 (Ubuntu 20.04, focal) ou em um kernel mais antigo ele é ignorado, então os crewmates podem ler o `/proc/<pid>/cmdline` uns dos outros (o prompt da tarefa, os comandos que as ferramentas deles rodam com as variáveis expandidas); um prompt ainda trafega na linha de comando do engine; o login do GitHub (`gh`) ainda é do usuário crew e compartilhado por todo crewmate que não tem token próprio (o hostd o usa, no próprio processo, para preencher os espelhos: um projeto privado que um crewmate tem fica ao alcance de quem a conta do operador alcança, só não dos crewmates); as allowlists de egress por crewmate não têm configuração; os engines em `engine-home` são compartilhados e instalados como root. Também não coberto (revisão de segurança 4, limites conhecidos):

- **A porta de hook pode ser ocupada (S-A).** A porta 47631 está acima de 1024 e o hostd recorre a uma porta aleatória quando ela está ocupada. Um processo que um crewmate destacou (cancelar uma tarefa mata o grupo de processos, nunca `reap`) pode ocupá-la enquanto o hostd reinicia; os hooks de todos os crewmates então param (aprovações inclusas), e um agente que sobreviveu ao restart com `ATLAS_HOOK_PORT=47631` enviaria o token de hook e os eventos a esse processo. A janela é curta (os engines costumam morrer com o terminal deles). A correção é o systemd segurar a porta (`ListenStream=127.0.0.1:47631`) e encerrar os processos que sobraram de um crewmate quando a tarefa dele termina; nenhuma das duas está feita.
- **Namespaces do kernel compartilhados.** O `/proc/net/{tcp,unix}` e os `cgroup.procs` e `memory.*` dos outros leaves mostram pids, portas e nomes de socket de outro crewmate (sem conteúdo). Sockets unix abstratos não têm permissões e são compartilhados (um network namespace; nada no Atlas escuta em um), e o mesmo vale para o SysV IPC e as filas de mensagens POSIX. Arquivos que um crewmate escreve em um `/tmp/x` literal ficam fechados para outros crewmates (`umask 007`), mas o `/tmp` em si e o `/dev/shm` são do helper, compartilhados.
- **O prompt fica na linha de comando**, visível a outros logins do host fora do helper (`ps`). O `<crewmate dir>/tmp` (o `TMPDIR` de um crewmate) nunca é limpo.
- **O hostd e a árvore de um crewmate (a classe das revisões 4 e 5).** O hostd roda como o usuário crew, que alcança a árvore de todos os crewmates, e um crewmate isolado controla a própria (links, `.git/config`, `atlas.yaml`). Então, para um crewmate isolado, o hostd nunca executa nada com um cwd, configuração ou script dessa árvore (o git passa pelo helper como o crewmate, `gitrun::Git`; um engine por `Request::Spawn`) e abre os arquivos dela só com os helpers `*_beneath`/`*_nofollow` do `spawn::layout`, que recusam um link em qualquer componente. Consequências visíveis: o UI gate lê só o bloco `affected:` do `atlas.yaml` (abaixo da raiz) e **recusa `affected.command`** para um crewmate assim (rodaria como o usuário crew; passe `routes` ou declare `affected.rules`), lista os arquivos da chave de cache com `git ls-files` como o crewmate e não guarda nada em cache (captura de novo) em uma árvore que não é um checkout git, e lê e escreve `.atlas/ui-regressions.json` abaixo da árvore de trabalho (no lugar, não de forma atômica); o `locate` abre a imagem abaixo da árvore e não diz mais se um caminho fora dela existe; um evento de hook de um crewmate não pode nomear um `session_file` para o hostd abrir. Uma proteção por grep (`crew/tests/structure.rs`) falha quando o código dos arquivos que tocam a árvore acessa o sistema de arquivos ou inicia um programa sem uma linha `// tree-access:` dizendo por que é seguro, quando o git é iniciado fora do `gitrun`, ou quando a configuração do projeto ou o plugin de affected é usado fora do `Tree` do gate. Não coberto: um caminho de projeto definido por um operador (não a árvore de um crewmate) e um crewmate que não roda como o próprio usuário.
- **`capture`, `locate` e o UI gate rodam como o usuário crew.** Para um crewmate isolado, o navegador inicia atrás de um proxy que recusa endereços privados, link-local e de metadados e toda porta desta máquina em que não escuta o próprio usuário daquele crewmate (`/proc/net/tcp{,6}`), em cada requisição e cada salto de um redirect, conectando ao endereço que checou; uma URL com um host proibido é recusada antes de qualquer navegador iniciar. Não coberto: o endereço público do próprio servidor quando há NAT (um IP elástico não está em nenhuma interface, então parece um host remoto), uma porta que o servidor de um crewmate abre depois de a captura começar e o UDP (o QUIC dos navegadores fica desligado atrás de um proxy). Cada engine primeiro prova que envia o loopback (`127.0.0.1`, `localhost`, `[::1]`, `127.0.0.2`) pelo proxy e não é usado se não enviar (o chromium envia, verificado; o webkit não passou por essa verificação nos testes, que não têm webkit).
- **Mais limites do proxy de captura (revisão 5).** A checagem e a conexão são dois passos: um crewmate que abre uma porta e deixa uma captura dela começar, e um loop de restart do servidor de outro crewmate que ocupa a porta nessa janela, é atendido (S-2; difícil de usar: o outro servidor precisa abrir a porta exatamente então, e a proteção teria de ser consultada sobre isso). O WebRTC envia UDP direto para fora sob a política padrão do Chromium: permite sondagem STUN/TURN e varredura de portas, não leitura de conteúdo. O endereço público do servidor por um hairpin NAT alcança o listener `0.0.0.0` de outro crewmate, mas só em uma porta já aberta para a internet. Os agentes têm `AF_NETLINK` e nada restringe user namespaces, então o código nf_tables do kernel fica ao alcance deles: isso é hardening do kernel (`kernel.unprivileged_userns_clone`, `user.max_user_namespaces=0`), não algo que o Atlas possa acrescentar. `[::]` e `0.0.0.0` são recusados de cara (uma conexão a eles vai para o loopback), e um listener curinga IPv4 nunca representa um endereço IPv6.
- **Não coberto (revisão 6, a corrigir em seguida).**
  - **N4: o navegador de captura não tem sandbox do SO.** O `runner.mjs` inicia o Chromium só com as opções de proxy, então o `chromiumSandbox` do Playwright fica desligado (`--no-sandbox`), e ele roda como o **usuário crew**. Uma página do servidor do próprio crewmate (ou qualquer URL pública que ele escolha) que explore um bug do navegador vira o usuário crew e alcança todos os crewmates. O Chromium fixado só avança quando o operador roda de novo `atlas crew browsers install`. Plano: rodar o runner como o crewmate (pelo helper; as regras de egress então precisam de uma exceção para a porta do proxy) ou como um usuário de captura dedicado. Até lá, trate `capture`, `locate` e `ui_gate` como uma forma de rodar o JavaScript do crewmate com o alcance do usuário crew, e mantenha o Chromium atualizado.
  - **L2: `capture` e `locate` não estão sob o limite de cgroup do crewmate** (o node e os navegadores rodam como o usuário crew) e o número de chamadas simultâneas não é limitado.
  - **L3: um trigger confirmado pelo dono do crew server.** Um trigger `Service` lê `<project_dir>/.logs/<service>.log` com um open bloqueante que segue links (um FIFO ali trava um worker; um link é um oráculo de um bit), e um trigger `File` faz polling com `std::fs::metadata`. Os dois só depois de o dono do crew server confirmar o card do trigger.
  - **L4: a proteção de PR revela quais issues existem em um repositório privado.** O `infer_github_repo` lê o `origin` da configuração git do crewmate, e o `invented_refs` checa `#N` com o login do GitHub do operador, então um crewmate pode descobrir quais números de issue existem em um repositório privado que o operador enxerga.
  - **L5: as ferramentas do próprio dono do crew server na árvore de um crewmate.** O `mobile/shell.rs` abre ali o login shell do usuário crew, e as APIs gerais do dono do crew server `git_status`, `git_diff`, de arquivos e de language server recebem um `root`: apontadas para o `work/` de um crewmate isolado, elas rodam o git como o usuário crew, inclusive o `core.fsmonitor` desse crewmate. Os hooks de prompt e essas chamadas são barrados pela checagem de dubious-ownership do git (nenhum `safe.directory` global é definido), e nenhum cliente abre a árvore de um crewmate assim por conta própria; é uma ação do próprio dono do crew server. Não aponte essas ferramentas para a árvore de um crewmate.
  - **Outros prefixos NAT64 e 6to4.** O prefixo NAT64 próprio de um operador dentro do espaço de endereços global e o `2002::/16` (6to4) não são recusados; em uma rede que os usa, acrescente-os ao arquivo de egress à mão.
  - **Um `workdir` definido pelo operador (ou um projeto por caminho) não pode ser gravável pelo crewmate.** O `git_in` decide pelo caminho, então para esses o git roda como o usuário crew mesmo na árvore de um crewmate isolado, e o hostd o abre com `std::fs` simples. Aponte para uma pasta em que só o usuário crew escreve.
- **O drop do loopback também bloqueia os servidores `localhost` do próprio agente**: testes que usam supertest ou um banco local falham dentro de um agente isolado. Isso não é uma falha de segurança, e sim o custo de descartar o loopback para o resto do servidor.
- **`recv_with_fds`** no helper ainda chama `recvmsg(…, 0)`; só o hostd envia nesse socket, então nada pode usá-lo.

## Capturas

A ferramenta `capture` deixa um crewmate olhar para uma UI em vez de adivinhar: ela abre uma URL no **chromium e no webkit do Playwright**, em várias larguras e nos temas claro e escuro, e devolve um PNG por combinação. Uma captura que retorna está completa: se faltar alguma combinação pedida ou alguma falhar, a ferramenta retorna um erro que nomeia cada uma (`webkit/390px/dark`), nunca um conjunto parcial.

```json
{"url": "http://localhost:5173/settings", "widths": [390, 1440], "themes": ["light", "dark"], "engines": ["chromium", "webkit"], "slow_network": false, "full_page": true, "steps": [{"do": "click", "selector": "#menu"}]}
```

Só `url` é obrigatório. Cada campo e o que significa:

| Campo | Padrão | Significado |
|---|---|---|
| `url` | | somente `http://` ou `https://`. `localhost` e endereços da LAN são permitidos de propósito (a página costuma ser um servidor de desenvolvimento no crew server), exceto para um crewmate que roda como o próprio usuário Linux: ele só pode carregar um servidor em que **o próprio usuário** escuta (o servidor de desenvolvimento em `localhost`), nunca o de outro crewmate, uma rede privada ou link-local, nem o endereço de metadados da nuvem (veja "Não coberto" acima); `file:`, `data:`, `javascript:` e similares são recusados |
| `widths` | `[390, 1440]` | larguras do viewport em px, de 200 a 4000: um celular e um desktop |
| `themes` | `["light", "dark"]` | o `prefers-color-scheme` da página |
| `engines` | `["chromium", "webkit"]` | chromium é Chrome/Edge/Android; webkit é Safari, iOS e o WKWebView do Tauri, onde moram a maioria dos bugs de layout do tipo "na minha máquina funciona" |
| `slow_network` | `false` | cada requisição é atrasada em 400 ms, em todos os engines |
| `full_page` | `true` | rola até o fim (para carregar o conteúdo sob demanda), depois captura a página inteira; `false` captura só a primeira tela |
| `steps` | nenhum | no máximo 20 ações antes da captura: `click`, `fill`, `press`, `wait_for`, `wait_ms`. Só seletores e texto, sem script |
| `dom` | `false` | grava também as caixas dos elementos da página ao lado de cada PNG (`<engine>-<width>px-<theme>.dom.json`): para cada elemento visível, a tag, o role, o nome acessível, o texto visível (cortado em 120 caracteres), o `data-testid` e outros atributos `data-*`, um seletor CSS estável (`[data-testid=…]`, depois `#id`, depois uma cadeia curta de `tag:nth-of-type`) e `x`, `y`, `w`, `h` em px CSS da página inteira naquela largura. Uma célula sem o arquivo é uma célula com falha. Os arquivos ficam ao lado dos PNGs e cada captura na resposta lista seu `dom` (nome do arquivo) e `dom_path`. É isso que o [`locate`](#localizar) lê |
| `checks` | `false` | executa também os [detectores de UI](#detectores-de-ui-temporário) em cada página depois que ela carrega e grava os achados ao lado de cada PNG (`<engine>-<width>px-<theme>.findings.json`, uma lista JSON de `{detector, severity, message, rect?, selector?}`; `[]` significa limpo). Cada captura na resposta então lista `findings` inline, mais `findings_file` e `findings_path`. Uma célula sem o arquivo é uma célula com falha. O [UI gate](#ui-gate) sempre pede isso |

Uma chamada pode pedir no máximo **16 combinações** (engines × larguras × temas; repetições contam uma vez): divida matrizes maiores em várias chamadas. O runner é interrompido após 5 minutos. O padrão são 8 capturas (2 engines × 2 larguras × 2 temas).

**Para onde vão as capturas.** Elas são anexadas à tarefa como as imagens do agente (`source: "agent"`, por `crew::attachments`, então ficam em `attachments/<task id>/`, são buscadas com `crewmate_attachment` e aparecem no chat como qualquer outra imagem), e a resposta da ferramenta lista cada combinação visitada com o nome do arquivo. São a prova de que uma [tarefa de UI](#tarefas-de-ui-e-prova) precisa.

**Requisitos** no crew server, só para este recurso:

- **Node.js 20 ou mais novo** com npm (`node`, `npm` e `npx` no `PATH`), o mínimo do Playwright fixado. O `apt install nodejs` no Ubuntu 24.04 instala o Node 18 e é recusado: use o Node do [nodesource](https://github.com/nodesource/distributions) ou de outra fonte. O `atlas crew browsers status` diz isso claramente (`Node 18.19.1 found, need >= 20`). Nada mais no Atlas Crew precisa de Node.
- `atlas crew browsers install <ssh-target>` (ou `atlas crew install <ssh-target> --browsers`), uma vez, em Debian ou Ubuntu. **Nenhum código npm roda como root.** Em ordem:
  1. como o usuário do crew, o `package.json`, um `package-lock.json` embutido no atlas (`crates/atlas-host/src/crew/capture/package-lock.json`), o `runner.mjs` e o `detectors.mjs` (o runner o importa) são gravados em `<hostd dir>/capture/` (`/var/lib/<crew user>/.atlas/capture/`);
  2. como o usuário do crew, `npm ci`: instala exatamente o `playwright` e o `playwright-core` do lock (uma só versão, `PLAYWRIGHT_VERSION` em `crates/atlas-host/src/crew/capture/mod.rs`) e confere cada tarball com o hash de integridade do lock, então um registry que serve outra coisa faz a instalação falhar em vez de rodar;
  3. como o usuário do crew, `npx --no-install playwright install chromium webkit` baixa os navegadores para `<capture dir>/browsers/`, legíveis por todos os usuários do servidor;
  4. como o usuário do crew, `npx --no-install playwright install-deps --dry-run chromium webkit` só *informa* quais bibliotecas do sistema faltam (`Missing system dependencies (N):` e um nome de pacote por linha);
  5. o atlas lê esse relatório e aceita apenas nomes de pacote que casam com `^[a-z0-9][a-z0-9.+-]*$` (então sem espaço, aspas, `;`, `&`, `|`, `$`, acento grave nem `-` no início), uma contagem igual à da lista e nada mais: qualquer outra linha é um erro mostrado a você e nada roda como root. **O Atlas monta sozinho o único comando root**, `sudo apt-get update && sudo apt-get install -y --no-install-recommends <those names>`: nada que o npm imprime é executado como comando.
  Rode de novo para reparar, ou depois de um `atlas crew update` que muda a versão fixada.
- Cerca de 1 GB de disco para os dois navegadores e algumas centenas de MB de RAM por captura.

`atlas crew browsers status <ssh-target>` imprime uma linha por peça que falta, e um crewmate que chama `capture` em um servidor sem os navegadores recebe a mesma instrução como erro da ferramenta.

## Localizar

"Aumente este botão" chega com uma captura de tela, não com o nome de um arquivo. A ferramenta `locate` transforma uma região de uma captura de tela nos arquivos a editar: ela lê a **página ao vivo** (as caixas dos elementos vêm do mesmo runner do Playwright de [Capturas](#capturas), com `dom: true`) e a **worktree**, nunca os pixels, então não há OCR nem palpite a partir da imagem.

```json
{"image": ".atlas/attachments/task9/shot.png", "rect": [235, 615, 170, 58], "url": "http://localhost:5173/pricing", "width": 1440}
```

| Campo | Significado |
|---|---|
| `image` | obrigatório. Caminho da captura de tela **dentro da worktree do crewmate**, onde os anexos de imagem são colocados (`.atlas/attachments/<task>/<name>`). Um caminho que resolve para fora da worktree (`..`, um caminho absoluto em outro lugar, um symlink para fora) é recusado |
| `url` | obrigatório. A página `http(s)` que a imagem mostra (um servidor de desenvolvimento no crew server serve). As caixas não podem vir de uma imagem, então sem isso a ferramenta avisa |
| `rect` | `[x, y, w, h]` em pixels da **imagem**. Omita para perguntar "o que é esta página": o landmark principal da página vem primeiro |
| `width` | largura do viewport para abrir a página. Padrão: a largura da própria imagem quando é 390 ou 1440 (as duas larguras padrão do `capture`), senão 1440. Um `rect` de uma imagem de outra largura (uma captura retina) é escalado para a página |

**O que faz**, em ordem: abre `url` no chromium em `width` (tema claro, página inteira) com `dom: true`; mantém os elementos cuja caixa **cruza** `rect` (a página inteira sem ele); ordena esses elementos; mapeia o caminho da URL e cada candidato para arquivos de código; responde.

**Ordenação.** Para um `rect`, a pontuação de um elemento é a média geométrica entre a fração do elemento que a região cobre e a fração da região que o elemento preenche, então um botão em volta do qual a região foi desenhada vence o container de página inteira que apenas o contém. Isso é multiplicado por uma *especificidade* (`data-testid` 1,0, role mais nome acessível 0,7, texto visível 0,4, nada 0,1) e aumentado em até 20% pela profundidade (um elemento mais fundo é uma resposta mais estreita). Sem `rect`, o landmark `<main>` / `role=main` vem primeiro, depois os maiores elementos com nome.

**Arquivos de rota** são encontrados casando os segmentos da URL com o caminho de cada arquivo da worktree (prefixos de monorepo como `apps/web/` funcionam; `node_modules`, `target`, `build`, pastas com ponto e symlinks não são lidos):

| Framework | Arquivos | Observações |
|---|---|---|
| SvelteKit | `src/routes/**/+page.svelte` | `[id]` é um segmento, `[...rest]` um ou mais, `[[lang]]` opcional, pastas `(group)` não fazem parte da URL |
| Next app router | `app/**/page.tsx` (também `.jsx`, `.ts`, `.js`, `.mdx`, e dentro de `src/`) | mesmas regras de segmento, pastas `@slot` não fazem parte da URL |
| Next pages router | `pages/**` (`index` é a própria pasta; `_app` e `pages/api/**` não são páginas) | |
| Flutter | `GoRoute(path: '/computers/:id')` em arquivos `.dart` | `:id` é um parâmetro; um caminho aninhado relativo (`settings`) casa com o fim da URL. Informado como `file:line` |

Um segmento estático vence um parâmetro que também casa (`/settings` sobre `/[tab]`). O **arquivo de componente** é o primeiro arquivo, além da rota, que contém o `data-testid` do candidato, senão o texto visível dele (3 caracteres ou mais); quando só o arquivo de rota o contém, essa é a resposta.

**Resposta.**

```json
{"url": "…", "width": 1440, "image_width": 1440, "ambiguous": false,
 "candidates": [{"route_file": "src/routes/pricing/+page.svelte", "component_file": "src/lib/PlanCard.svelte",
                 "selector": "[data-testid=\"buy-button\"]", "testid": "buy-button", "text": "Buy now", "score": 0.912}]}
```

No máximo 5 candidatos, o melhor primeiro. **`ambiguous: true` quando as duas maiores pontuações estão a menos de 10% uma da outra**, e a resposta então traz uma linha `next`; a descrição da ferramenta diz ao agente para usar `ask_user` com os candidatos (seletor e texto) como opções e só então editar, nunca escolher um. Um `rect` que não toca nenhum elemento, uma `url` que falha ao carregar e uma imagem fora da worktree são erros que o agente lê.

Ela precisa dos navegadores do capture ([Requisitos](#capturas)) e só funciona no painel de tarefa do próprio crewmate, já que lê os arquivos desse crewmate.

## UI gate

O `capture` olha uma página. A ferramenta `ui_gate` decide **quais** páginas uma mudança exige, lembra o que falhou e nunca deixa um bug de UI já corrigido voltar sem ser notado. Ela usa o `capture` por baixo, então precisa dos mesmos navegadores ([Capturas](#capturas)).

Um **cenário** é uma URL em uma matriz de engine x largura x tema (padrão: chromium + webkit, 390 + 1440 px, claro + escuro). Tem um id estável (12 dígitos hex da URL e de suas células ordenadas).

| Chamada | O que faz |
|---|---|
| `ui_gate {action: "run", base_url}` | planeja, captura e responde passou/falhou por cenário, com os nomes das capturas. `base_url` é onde o app é servido (`http://localhost:5173`); as rotas são juntadas a ele. Opcionais: `routes` (páginas a verificar no lugar das que o diff implica), `files` (arquivos alterados no lugar do diff do git), `force` (ignora o cache) |
| `ui_gate {action: "rerun_failed"}` | captura só os cenários que falharam da última vez nesta tarefa. Os que passaram mantêm o veredito e não são tocados |
| `ui_gate {action: "add_regression", url, note?, widths?, themes?, engines?}` | registra um bug de UI que o crewmate acabou de corrigir |

**O plano.** Os arquivos alterados são o diff da worktree (`atlas_core::affected::changed_files`: com commit desde o merge base com `origin/main`, mais os arquivos em staging, modificados e não rastreados). As rotas que eles tocam vêm do mesmo grafo da ferramenta MCP `atlas_affected`: o bloco `affected:` do `atlas.yaml` (um `command` de plugin, ou `rules` com `routes:`), senão nada. Para um crewmate que roda como o próprio usuário Linux, só as `rules` são usadas: o `command` do plugin é recusado (veja "Não coberto" na seção de isolamento). Cada rota vira um cenário da matriz padrão. **Toda regressão entra sempre**, qualquer que seja o conteúdo do diff. Uma rota que não é um caminho de página (um curinga, `GET /api`) é um erro que a nomeia, não uma página ignorada. Uma mudança que não toca nenhuma rota, em um projeto sem regressões, não planeja nada e passa.

```yaml
affected:
  rules:
    - match: ['src/routes/docs/**']
      routes: ['/docs']
```

**Resultados por tarefa.** O veredito de cada cenário (passou/falhou, nomes das capturas, os problemas encontrados) é salvo em `<crewmate dir>/ui-gate/<task id>/ui-gate.json` depois de cada cenário, então uma reinicialização, ou uma execução interrompida, ainda sabe o que falhou. As capturas ficam ao lado como `<scenario id>-<engine>-<width>-<theme>.png`.

**Cache.** O veredito de um cenário fica no [cache de tarefas](cli.md) (`~/.atlas/cache`, compartilhado com toda worktree e com o `atlas run --cache`), com chave formada pelo id do cenário, pelas verificações registradas e pelos bytes de todo arquivo do projeto que o git não ignora, edições não commitadas incluídas. Um cenário que já passou sobre os mesmos bytes é um **hit**: o runner não inicia e a resposta diz `"source": "cached"`. Só os que passam são salvos, então um cenário que falha é sempre capturado de novo; um hit cujas capturas já não estão na pasta da tarefa conta como miss. O cache não enxerga os dados do app em execução: passe `force: true` quando os dados por trás de uma página mudarem.

**Registro de regressões.** `.atlas/ui-regressions.json` no projeto:

```json
{ "regressions": [ { "url": "http://localhost:5173/pricing", "widths": [390], "themes": ["dark"], "note": "toggle clipped at 390" } ] }
```

`add_regression` grava o arquivo (o mesmo cenário de novo só atualiza a nota). **O crewmate faz commit do arquivo junto com a correção**, e é assim que toda outra worktree e todo outro crewmate repete o bug. Um arquivo que não é JSON válido, tem um campo desconhecido ou uma entrada que o `capture` recusaria é um erro que nomeia o arquivo e o que está errado; o gate nunca trata um registro ilegível como vazio, e o `add_regression` nunca sobrescreve um arquivo que não conseguiu ler. As regressões são salvas com a URL como foi escrita, então mantenha o `base_url` estável entre execuções (um servidor de desenvolvimento em outra porta é outro cenário).

**Critério para passar.** Um cenário passa quando toda combinação pedida foi capturada sem erro e nenhuma verificação registrada encontrou problema. As verificações são o ponto de extensão: implemente `crew::uigate::Check` (`fn name`, `fn run(combo, png) -> Result<Vec<Finding>, String>`) e registre em `uigate::check::registered()`. Há uma registrada, `ui-detectors`: os [detectores de UI](#detectores-de-ui-temporário) rodam em cada página durante a captura e um achado de severidade `error` reprova o cenário, informado por célula como `webkit/390px/dark: empty-band: 554 px of the page … hold no content`, com o seletor do elemento quando há um; um `warn` fica no arquivo de achados e não o reprova. Uma verificação que não consegue rodar reprova o cenário (um gate que não consegue olhar não pode passar: uma célula sem arquivo de achados, ou com um arquivo que não contém uma lista de achados, cai nesse caso), e os nomes das verificações registradas fazem parte da chave do cache, então adicionar um detector captura de novo o que passou sem ele.

## Detectores de UI (temporário)

> **Temporário.** Os detectores pertencem ao `@codeatlasdev/ui` ([codeatlasdev/ui#1290](https://github.com/codeatlasdev/ui/issues/1290), pedido pela [#215](https://github.com/codeatlasdev/atlas/issues/215)): o Atlas e todo app do kit devem compartilhar as mesmas regras. Esse pacote ainda não foi lançado, então `crates/atlas-host/src/crew/capture/detectors.mjs` é uma implementação local da API dele, escrita para ser removida. Tem os mesmos nomes e o mesmo `Finding`, e não depende de nenhum kit de componentes (o Atlas roda nos apps dos clientes).

```ts
type Finding = { detector: string; severity: "error" | "warn"; message: string; rect?: DOMRectInit; selector?: string };
detect(page: Page, opts?: { only?: string[] /* + any threshold below */ }): Promise<Finding[]>
```

`capture {checks: true}` e o UI gate rodam isso uma vez por célula, depois da captura de tela, e gravam `<cell>.findings.json`. Os detectores, com a heurística de cada um (eles preferem deixar passar um achado a gerar um falso: um gate que dá alarme falso é desligado):

| Detector | Falha (`error`) quando | Ajustes (padrão) |
|---|---|---|
| `empty-band` | um trecho da altura da página não tem conteúdo: texto, mídia, controles de formulário e caixas folha pintadas contam; um wrapper vazio ou uma caixa pintada que ocupa um terço da página não contam, e barras fixas são ignoradas. Uma página que termina antes do fim da tela não é uma faixa | `emptyBandPx` (490 em ≤ 500 px de largura), `emptyBandScreens` (0,6 da altura do viewport em telas mais largas) |
| `gutter` | em um viewport com largura ≤ `mobileMaxWidth`, texto (medido nas próprias caixas de linha), controles ou mídia ficam mais perto que `gutterPx` da borda esquerda ou direita. Uma imagem ou vídeo de sangria total pode encostar nela; o que está fora da tela (um carrossel, um drawer) é ignorado | `gutterPx` (12), `mobileMaxWidth` (500) |
| `sibling-overlap` | dois elementos filhos em fluxo do mesmo pai compartilham pelo menos `overlapMinPx2` px² e `overlapMinShare` da menor caixa. `absolute`, `fixed`, `sticky`, floats, diálogos, tooltips, menus, listboxes e popovers são overlays e são ignorados | `overlapMinPx2` (16), `overlapMinShare` (0,15) |
| `raw-text` | um título, botão, `role=button\|tab\|menuitem`, label, cabeçalho de tabela, legend ou summary é só um identificador: `snake_case`, `kebab-case` de duas ou mais palavras, um UUID ou um id hex de 16+ dígitos. Uma frase, uma palavra isolada ou qualquer coisa com espaço não é | nenhum |
| `action-hierarchy` | um grupo tem mais de uma ação **primária** visível e habilitada. Primária significa `data-variant`, `data-kind`, `data-intent`, `data-emphasis` ou `data-priority` igual a `primary`; ou um token de classe `primary`, `cta` ou terminado em `-primary`/`_primary`/`-cta`; ou um botão `type=submit` sem nenhum desses atributos. Um grupo é o `form`, `[role=group\|toolbar\|dialog]`, `dialog`, `.actions`, `.button-group`, `.btn-group`, `footer`, `nav`, `section`, `article` ou `header` mais próximo, senão o pai | nenhum |
| `cls` | as mudanças de layout sem entrada recente do usuário somam `clsError`; a partir de `clsWarn` é um `warn`. O achado nomeia o elemento que se moveu (`selector`) e onde ele está (`rect`). O Chromium informa as mudanças sozinho; o WebKit não, então o runner mede a mesma grandeza à mão (fração de impacto × fração de distância da maior caixa que se moveu, por frame) desde antes de a página carregar. Os dois engines concordam na fixture | `clsError` (0,1), `clsWarn` (0,02) |
| `focus-after-tap` | depois de focar um controle que faz algo (botão, `a[href="#…"]`, summary, `[tabindex]`, `role=button\|tab\|menuitem`) e ativá-lo com Enter, o foco está no body da página. Foco que vai para o que o controle abriu, ou o controle ser removido, não é problema. Links que navegariam para fora e botões de submit não são ativados. O pressionar de tecla, e não de ponteiro, é de propósito: o WebKit e o Safari tiram o foco do elemento ativo ao pressionar o ponteiro e nunca focam um botão, o que é coisa do engine e não da página | `tapMax` (6 controles) |

**Fixtures** (`crates/atlas-host/src/crew/capture/fixtures/`): `<detector>.fail.html` e `<detector>.fixed.html` para cada um, mais `clean.fixed.html`, uma página comum (navegação, hero, cards, um formulário com seu próprio primário) que precisa continuar limpa em 390 e 1440 px. O `cargo test -p atlas-host crew::capture` roda todas em chromium e webkit reais (ele pula, informando o motivo, em uma máquina sem node ou sem os navegadores): a página com falha deve dar um `error` do próprio detector e só dele, e a corrigida nada. `empty-band.fail.html` é a faixa de 490 px da #215.

**Plano de remoção**, quando o ui#1290 for lançado:

1. adicionar `@codeatlasdev/ui` ao `package.json`/`package-lock.json` da pasta capture (vizinho de `PLAYWRIGHT_VERSION` em `capture/mod.rs`), fixado como o Playwright;
2. no `runner.mjs`, trocar `import { detect, observeShifts } from "./detectors.mjs"` pelo import do pacote (a API é a mesma: `detect(page, opts)` e o init script que registra as mudanças); o arquivo de achados, o lado Rust (`capture/detectors.rs`, `uigate/check.rs`) e os testes não mudam;
3. excluir `detectors.mjs`, `DETECTORS_MJS` (`capture/mod.rs`), a linha dele em `atlas crew browsers install` e `status` (`atlas-cli/src/commands/crew/browsers.rs`) e o enquadramento "temporário" desta seção;
4. manter `fixtures/` e o teste real como a aceitação do pacote no Atlas (movê-los para o upstream se o ui#1290 quiser).

Até lá, um limite que precisa mudar muda em `detectors.mjs` e é informado no ui#1290 para o pacote receber o mesmo.

## Modelo de segurança

- **Usuário dedicado.** O usuário do crew (`atlas-crew`, ou o `atlas-bot` reaproveitado) não tem sudo; escreve só na própria home e no que os agentes criam em `workdir`.
- **Proteções do systemd.** `NoNewPrivileges=true`, `ProtectSystem=strict`, `ProtectHome=read-only` com `ReadWritePaths=<crew home>`, `PrivateTmp=true`, `ProtectKernelTunables=true`, `ProtectControlGroups=true`, `RestrictSUIDSGID=true`, `MemoryMax=80%`, `TasksMax=4096` ([a unit](#a-unit-do-systemd)). Com o helper instalado, cada crewmate tem o próprio usuário ([abaixo](#um-usuário-linux-por-crewmate)); sem ele, os crewmates ainda compartilham o usuário do crew.
- **Um usuário Linux por crewmate** ([detalhes](#um-usuário-linux-por-crewmate)). Cada crewmate roda como `atlas-cm-<id>`: os outros não conseguem ler o `secrets.json`, os logins nem o checkout dele, e uma política de saída de rede limita onde ele se conecta. Só vale em um servidor em que o `atlas crew install` configurou o helper; senão todos os crewmates compartilham o usuário do crew como antes.
- **Chave SSH restrita.** O `atlas crew install` acrescenta a sua chave só ao `authorized_keys` do usuário do crew, nunca ao do usuário admin.
- **sudo uma vez.** A instalação e a desinstalação precisam de sudo; o serviço em execução nunca. A senha do sudo, quando o Desktop a passa, vem por stdin e é descartada depois que o cache de credenciais é preenchido.
- **Nenhum código npm como root.** O `atlas crew browsers install` roda o `npm ci` (de um lock embutido no atlas, com integridade verificada), o download dos navegadores e o relatório de dependências do Playwright como o usuário do crew. Só um comando roda como root: `apt-get install -y --no-install-recommends <names>`, que o atlas monta sozinho a partir de nomes de pacote que casaram com `^[a-z0-9][a-z0-9.+-]*$` no relatório; qualquer outra coisa no relatório é um erro e nada roda como root. Veja [Capturas](#capturas).
- **Segredos.** Os segredos de cada crewmate ficam em `<crewmate dir>/secrets.json` (modo 0600), são injetados como variáveis de ambiente só no agente desse crewmate e nunca são devolvidos em nenhuma resposta. Terminais e segredos nunca são repassados a um celular.
- **Auditoria.** Todo crewmate tem um log só de acréscimo, escrito pelo hostd e nunca pelo agente: cada chamada de ferramenta que o agente informa (nome completo da ferramenta e entrada, não o passo de 80 caracteres), cada aprovação pedida e como terminou (`allow`, `deny` ou `timeout`, e quem respondeu: `desktop`, `phone:<node id>`, `telegram:<chat id>` ou `none`), o ciclo de vida da tarefa e as mudanças no modo de aprovação, nos triggers, no projeto, no engine e nos segredos (o nome de um segredo, nunca o valor). Veja [Auditoria](#auditoria).
- **A voz fica nas suas máquinas.** O [ditado](#ditado) reconhece a fala com um whisper.cpp fixado e conferido por hash, compilado no crew server; o áudio é apagado quando o texto é gerado (e após 10 minutos se um envio for abandonado), a chave de um engine hospedado opcional nunca sai dos segredos do crew, e um engine que falha nunca é repetido em outro.
- **Nenhum servidor da CodeAtlas.** A única chamada de saída opcional é o relay de push (só o tipo). Detalhes: [PRIVACY.md](https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md).

## Auditoria

O que um crewmate fez e quem decidiu o quê, salvo no crew server em `<crewmate dir>/audit.jsonl` (modo 0600, um objeto JSON por linha: `seq`, `ts`, `kind`, `crewmate_id`, mais os campos do tipo). Eventos do crew inteiro (um crewmate criado ou excluído, o crew pausado ou retomado) vão para `<hostd dir>/crew-audit.jsonl`, que sobrevive a um crewmate excluído.

| `kind` | Escrito quando | Campos |
|---|---|---|
| `tool_call` | o agente informa uma chamada de ferramenta (agentes de terminal e engines headless igualmente) | `task_id`, `tool_name`, `tool_input` |
| `approval_asked` | uma chamada fica retida para uma pessoa | `task_id`, `req`, `tool_name`, `tool_input`, `rail` (o que forçou a pergunta, se foi uma barreira) |
| `approval_decision` | **toda** decisão sobre uma chamada de um crewmate que consegue perguntar, quem quer que a tome | `task_id`, `req` (chamadas retidas), `tool`, `tool_input`, `decision` (`allow`/`deny`), `source` (`mode`, `grant`, `person`, `rail`, `timeout`), `device`, `wait_ms`, `rail`, `comment` |
| `approval_mode_changed` | o modo de um crewmate foi trocado (ou o Bypass foi retirado do crew) | `from`, `to`, `device` |
| `task_queued`, `task_started`, `task_finished` | ciclo de vida da tarefa | `task_id`, `source`, `prompt`; `state` e `error` no fim |
| `config_changed` | modo, engine, projeto, agente, nome, tamanho do role, respostas curtas, triggers, rotinas | `field`, `from`, `to` (os tokens de webhook ficam de fora) |
| `task_refused` | as proteções recusaram uma tarefa concluída | `task_id`, `reasons` |
| `chain_refused` | uma automação "Após o crewmate" não foi disparada porque o crewmate dela já estava na cadeia | `trigger_id`, `from_crewmate`, `from_task`, `chain` |
| `refusal_overridden` | o dono do crew server aceitou o trabalho recusado | `task_id` |
| `task_unproven_ui` | uma tarefa `ui` terminou sem duas capturas do agente | `task_id`, `reason`, `captures` |
| `secret_set` | `crewmate_secret_set` | `name` |
| `audit_gap` (nível do crew) | o hub de agentes descartou eventos sob carga | `dropped` |

Tipos no nível do crew: `crewmate_created`, `crewmate_deleted`, `paused`, `resumed`, `audit_gap`.

**Quem respondeu.** `device` é `desktop` para qualquer cliente no socket do servidor, o que inclui a CLI e, para um crew server, o relay do Mac: uma aprovação que um celular envia pelo relay a um crew server aparece como `desktop` lá. Um celular conectado direto a um hostd é `phone:<node id>`; o Telegram é `telegram:<chat id>`. Uma chamada que um modo ou uma permissão da tarefa liberou não tem `device` (a troca de modo que decidiu uma chamada em espera nomeia quem trocou).

**Remoção de segredos.** Os valores dos segredos do crewmate e o token do hook são trocados por `[secret:NAME]` / `[secret:hook_token]` antes de qualquer gravação (os segredos são lidos a cada gravação, então um definido há um minuto também é coberto; valores com menos de 4 caracteres não são tratados como segredos). Uma string com mais de 8 KiB é cortada com um marcador que diz quantos bytes havia.

**Limites.** Um arquivo é rotacionado em 1 MiB (`audit.jsonl.1` … `.4`); o mais antigo de cinco é descartado, então um log nunca passa de cerca de 5 MiB. Uma linha que não pode ser gravada vai para o log (`atlas crew logs`), nunca é ignorada em silêncio. Se o hub de agentes descarta eventos sob carga, uma entrada `audit_gap` avisa.

Leia com `crewmate_audit` e `crew_audit` (escopo `read` do Mobile); o Atlas Desktop mostra o log nas configurações de um crewmate → **Auditoria** (filtre por chamadas de ferramenta, aprovações, tarefas ou configurações; copie uma entrada ou tudo o que está na tela). O Atlas Mobile não tem tela de auditoria.

## Solução de problemas

| Sintoma | Causa e solução |
|---|---|
| Os comandos `crewmate_*` respondem `this computer is not a crew server (atlas crew install)` | O hostd roda sem `ATLAS_CREW_HOST=1`. A unit define essa variável; não rode `atlas hostd` à mão em um crew server |
| `info.caps` não tem `"crew"` | mesma causa; os clientes verificam isso antes de mostrar a interface do Crew |
| `atlas crew install` para com `a unidade <unit> está mascarada no servidor e o Atlas não conseguiu desmascará-la` | A unit é um symlink para `/dev/null` (`systemctl mask`) e o `sudo systemctl unmask` da instalação foi recusado. Rode no servidor o comando que ele exibe (`sudo systemctl unmask <unit>`) e depois rode a instalação de novo. Quando o unmask funciona, a própria instalação o faz e informa `A unidade <unit> estava mascarada: o Atlas a desmascarou e gravou a unidade real` |
| O prompt de aprovação é negado na hora | ninguém pode responder agora: nenhum cliente desktop ou celular conectado e nenhum chat do Telegram vinculado ([Aprovações](#aprovações)) |
| Um crewmate `ask` nunca pergunta | confira se a aprovação dele é mesmo `ask` (uma tarefa do GitHub ou de webhook também pergunta, veja [Automações](#automações)) e se alguém pode responder ([Aprovações](#aprovações)). Os engines `codex` e `opencode` não conseguem perguntar, então `ask` é recusado para eles |
| `Não foi possível conectar a … por SSH` | o próprio SSH falhou (conexão recusada, timeout, chave rejeitada), não o sistema operacional do servidor; corrija o SSH primeiro |
| `O sudo em … pede senha` | conceda sudo sem senha, ou rode a instalação pelo Atlas Desktop (ele envia a senha a cada chamada privilegiada, e assim o cache de sudo por terminal do Ubuntu não a perde) |
| A desinstalação diz que um usuário do crew ainda está em uso | a desinstalação verifica antes e não removeu nada: feche o Atlas Desktop e qualquer sessão ssh ou processo rodando como o usuário do crew, e rode de novo |
| A desinstalação diz que precisa de uma decisão | sem terminal e sem `--yes`: rode em um terminal, ou passe `--yes` depois de `atlas crew export` |
| A exportação ou a importação diz que o atlas do servidor é antigo demais | `atlas crew install <ssh-target>` atualiza o binário do servidor; depois tente novamente |
| Uma primeira tarefa mostra `Baixando <repo>…` (no Atlas Mobile, `Downloading <repo>…`) | o projeto é clonado antes de a tarefa começar; uma pasta inexistente é recusada quando você a define |
| Comando antigo `atlas bot` em um script | continua funcionando; passe para `atlas crew` quando puder |

### Falha ao adicionar um crew server

O formulário do Atlas Desktop diz o que falhou e traz um link para cá; as mesmas linhas valem para `atlas crew install`.

| O que a tela diz | Causa e solução |
|---|---|
| O servidor **recusou a chave** `<path>` | o servidor não tem essa chave em `~/.ssh/authorized_keys`. Entre uma vez com o método **Senha** (o Atlas adiciona a chave), escolha outra chave ou adicione você mesmo a chave pública. Uma chave com passphrase precisa estar carregada antes no `ssh-agent` |
| Este servidor **não aceita senhas** | o `sshd` lista só `publickey`. Use o método **Chave SSH**; alguém com acesso precisa adicionar antes a chave pública deste computador |
| O servidor não aceitou essa senha | senha errada (Caps Lock ativado?). A senha é usada uma vez e nunca é armazenada |
| Este usuário **precisa de senha para o sudo** | a instalação precisa de sudo. Digite a senha do sudo quando o formulário pedir (ela vale só para esta instalação) ou conceda sudo sem senha |
| `Failed to enable unit … is masked` (#518) | o `atlas-crew.service` está mascarado (um symlink para `/dev/null`). A CLI exibe a correção: `sudo systemctl unmask atlas-crew.service` no servidor, depois tente novamente |
| Sem **systemd** (`O systemd não está rodando como PID 1`) | o servidor é um container ou usa OpenRC/runit. Um crew server precisa de um host com systemd |
| O **cliente OpenSSH** não está instalado (Windows) | Configurações → Sistema → Recursos opcionais → Adicionar um recurso → Cliente OpenSSH, ou `Add-WindowsCapability -Online -Name OpenSSH.Client~~~~0.0.1.0` em um PowerShell como administrador |
| A identidade do servidor mudou (verificação da **host key** falhou) | o servidor foi reinstalado, ou há alguém no meio da conexão. Se era esperado, esqueça a chave antiga: `ssh-keygen -R <host>`, depois tente novamente |
| Não foi possível conectar a `<host>` / `<host>` não respondeu / O endereço `<host>` não existe | confira o endereço e a porta, se o `sshd` está rodando, firewalls e VPNs |

Logs: `atlas crew logs <ssh-target> -f`. Estado: `atlas crew status <ssh-target>`.

## Aparência do crewmate

Todo crewmate é um personagem animado construído a partir do globo do Atlas: ele pisca, olha ao redor, respira, pula quando precisa de você e cochila quando está offline. Você escolhe a aparência em **Personalizar visual** (diálogo de novo crewmate, ou clique no avatar dele nas configurações). Fonte da verdade: `apps/desktop/src/lib/crew/look.ts`.

Layout do editor: uma prévia grande ao vivo que dá um pulo a cada escolha, botões de estado da prévia, **Sortear** (todas as linhas de uma vez, animado, sem corte seco), **Kits de função** e depois uma linha por parte. Cada linha mostra o rótulo e o nome escolhido; as opções ficam coloridas quando selecionadas; a primeira opção de cada linha é **Nenhum** ou **Nenhuma**, exceto em Cor. Cada escolha entra animada (o chapéu cai com um quique, a roupa surge numa transição, a ferramenta é jogada na mão, os óculos encaixam).

| Linha | Opções | Padrão |
|---|---|---|
| Cor | 9 amostras (azul do Atlas `#3d5bff` primeiro) mais um hex personalizado | escolha estável a partir do id do crewmate |
| Olhos | Redondos, Pontos, Felizes, Viseira, Ciclope, Sonolentos | Redondos |
| Chapéu | Nenhum, Chapéu de palha, Boné, Gorro, Cartola, Coroa, Chapéu de festa, Chapéu de mago, Fones de ouvido, Antena, Bicórnio com mapa, Bandana | Chapéu de palha |
| Ferramenta (mão direita) | Nenhuma, Bússola, Luneta, Chave inglesa, Pena, Lupa, Megafone, Alfinete de mapa, Lanterna | Nenhuma |
| Roupa | Nenhuma, Cachecol, Casaco de capitão, Avental de ferramentas, Bolsa a tiracolo, Gravata | Nenhuma |
| Rosto | Nenhum, Tapa-olho, Monóculo, Óculos, Óculos de sol, Óculos de proteção | Nenhum |

- **Chapéu de palha** (padrão): aba larga e reta, textura trançada, uma faixa na cor de destaque do crewmate (nunca vermelha), um arco fino de meridiano do globo na copa, uma leve inclinação com uma animação secundária de repouso na ponta da aba. Escala de 24 a 140 px.
- **Kits de função** definem chapéu, ferramenta, roupa e óculos com um clique e mantêm a cor e os olhos: Capitão, Vendas, Revisor, Vigia, Escriba, Faroleiro.
- Ferramentas e roupas são desenhadas a partir de 40 px; os ícones de lista continuam só com o rosto. Os óculos acompanham o olhar em qualquer tamanho. Enquanto trabalha, cada ferramenta tem o próprio gesto (a chave inglesa gira, a pena rabisca, a luneta varre), movido por um sistema de molas (`rig.ts`). Prévia: `/crew-demo` em dev.
- Aparências salvas e remotas não são confiáveis: o `normalizeLook` mantém cada campo válido e troca cada campo inválido pelo padrão daquele crewmate. O catálogo fica nos clientes, então um id desconhecido aparece como o padrão.
- O hostd guarda a aparência no crew server e devolve todos os campos em `crewmate_list` (`color` como `#rrggbb`; `eyes`, `hat`, `tool`, `outfit`, `face` como `[a-z0-9_-]`, de 1 a 24 caracteres), então uma aparência escolhida em um cliente aparece em todos eles.

## Teste end-to-end em systemd real

O `crates/atlas-cli/tests/crew_install.rs` roda `atlas crew install` contra um servidor simulado (`ssh`, `sudo` e `systemctl` falsos), o que fixa o que a CLI envia. Ele não mostra se as units sobem, se `useradd` e `nft` fazem o que os scripts esperam, nem se uma tarefa cancelada não deixa nenhum processo para trás. O `scripts/crew-e2e.sh` mostra, em um Linux com systemd de verdade, e o workflow `.github/workflows/crew-e2e.yml` o executa (à mão, toda noite, em pull requests que mexem em `crates/atlas-host/src/crew/**`, `crates/atlas-cli/src/commands/crew/**` ou no próprio teste, e como workflow reutilizável do `release.yml`).

O servidor é um container Ubuntu 24.04 privilegiado, com systemd como PID 1 e sshd em uma porta fora do padrão (`scripts/crew-e2e/Dockerfile`: sudo, curl, tar, git, nftables, um usuário admin com sudo sem senha, nada do Atlas). O cliente é a CLI `atlas` compilada a partir do commit, mais o protocolo do próprio hostd por um túnel de socket `ssh -L` como o usuário do crew, o mesmo caminho do Atlas Desktop. O único item falso é o agente: o `scripts/crew-e2e/fake-claude.sh` faz o papel do `claude` em `/usr/local/lib/atlas-crew/engines` e imprime o resultado JSON que o módulo engine lê, então não precisa de chave de API. Em ordem:

| Etapa | Verifica |
|---|---|
| `atlas crew install --port … --yes --binary` (duas vezes) | informa que o crew server está pronto; `atlas-crew.service` e os dois sockets auxiliares estão ativos; `atlas.prev` existe depois da segunda execução; `atlas crew status` diz que está atualizado; a chave de quem chama permite entrar por ssh como `atlas-crew`; o hostd responde `info` com a capability `crew` |
| um crewmate e uma tarefa | o crewmate recebe o próprio usuário Linux e a própria pasta de trabalho; a tarefa termina `done` com o resultado do engine falso, timestamps e sem erro; o resultado está em `tasks.jsonl` e o log de auditoria não está vazio |
| um engine que sai com código 1 | a tarefa termina `failed` com um erro |
| uma tarefa travada, `crewmate_cancel` | o processo filho do engine (`sleep`) roda como o usuário do crewmate, depois a tarefa fica `cancelled` e nenhum processo da sua árvore sobra (#290) |
| uma tarefa travada, `atlas crew pause` | a tarefa fica `cancelled` com o erro `paused` e a árvore dela desaparece; uma tarefa enviada nesse meio-tempo continua `queued`; depois de `atlas crew resume`, ela roda até `done` |
| `systemctl restart atlas-crew` no meio de uma tarefa | a tarefa termina `done`, marcada como `resumed` com um reinício, e a execução que o reinício cortou não fica rodando (#199) |
| `atlas crew export`, depois `atlas crew uninstall` | o arquivo exportado começa com `manifest.json` e contém o crewmate e o histórico dele, sem `secrets.json`; a desinstalação sem terminal e sem `--yes` recusa e não muda nada; com `--yes` ela informa o que excluiu e não deixa usuário, grupo, unit, arquivo, socket, tabela de firewall nem processo do crew server |

Qualquer verificação que falhe faz o job falhar. O journal do servidor, o `hostd.log`, a lista de processos, o ruleset do `nft` e os arquivos dos crewmates são enviados como o artefato `crew-e2e-server-logs`, inclusive em caso de falha. Localmente você precisa de docker, ssh, jq e python3: `cargo build -p atlas-cli && scripts/crew-e2e.sh` (`CREW_E2E_KEEP=1` deixa o container no ar para você investigar). O teste nunca toca em `~/.ssh/config`: o ssh roda por um shim que adiciona a própria configuração e chave.

## Mapa do código

Para quem contribui. Um conceito por arquivo; o comentário de documentação do módulo, no topo de cada arquivo, diz o que ele cobre.

| Camada | Onde | Cobre |
|---|---|---|
| hostd (crew server e Mac) | `crates/atlas-host/src/crew/` | roteamento de comandos (`mod.rs`) para `handlers/` (crewmates, tarefas, configurações); `hub.rs` (o hub do crew); `store/` (tipos em disco, a migração do layout antigo, `archive.rs`: o arquivo de exportação e importação); `attachments.rs` (imagens em uma tarefa: limites, a verificação de tipo pelos bytes, nomes seguros, armazenamento, a cópia do agente, envio e busca em pedaços); `voice/` (ditado: `audio` o envio, o limite de 300 s e a exclusão, `engine` o whisper.cpp e o endpoint hospedado, `vocab` dicas e substituições; o lado do Telegram é `telegram/voice_notes.rs`); `runner/` (uma tarefa: fila, `pty`, `conversation`, `prompt`, `action`, `outcome`, `summary`, `recover` para tarefas que um reinício interrompeu, `guard`/`gate`/`tape` para as proteções contra o agente); `events.rs` (todo evento push, um builder para cada); `pane.rs` (a chave de painel `crew:`); `engine/` (omp, claude, codex, opencode); `triggers/` e `schedule.rs` (automações); `github/` (login e API do `gh`; `github/app/`: o GitHub App, veja [GitHub App](#github-app)); `telegram.rs`; `push.rs`; `project.rs` (checkout); `mcp.rs` (`atlas-crew-mcp`); `locate/` (a ferramenta `locate`: `route.rs` caminho da URL para arquivos de rota por framework, `rank.rs` as caixas de elementos da página e seu ranking, `mod.rs` a requisição, a captura com `dom: true` e o mapeamento para o código-fonte); `capture/` (a ferramenta `capture`: `spec.rs` regras de requisição e de relatório, `detectors.mjs` + `detectors.rs` + `fixtures/` os detectores de UI temporários, `runner.mjs` o script embutido do Playwright, `package-lock.json` o lock do qual o `npm ci` instala); `uigate/` (a ferramenta `ui_gate`: plano por mudança, nova execução do que falhou, regressões); `relay/` (do celular para o crew server); `tests/` (testes de integração por área, helpers compartilhados em `tests/fixture.rs`) |
| rotas de hook | `crates/atlas-host/src/agents/hook_server.rs` | `/crew/<id>/ask`, `/crew/<id>/set_profile`, `/crew/<id>/capture`, `/crew/<id>/locate`, `/crew/<id>/ui_gate`, `/crew/<id>/set_ui`, `/crew/<id>/hook/<token>` |
| CLI | `crates/atlas-cli/src/commands/crew/` | `atlas crew install\|update\|uninstall\|export\|import\|status\|logs\|login\|pause\|resume\|pair\|browsers\|voice`; o texto da unit, o plano de migração, o asset da release por arquitetura, os scripts de atualização e rollback e a decisão de saúde (`binary.rs`), os scripts e decisões de exportação, importação e desinstalação (`transfer.rs`), os scripts de instalação e status dos browsers do capture (`browsers.rs`) e o leitor do relatório de dependências do Playwright que monta o único `apt-get` de root (`apt.rs`) são funções puras com testes |
| Backend do Desktop | `crates/atlas-desktop/src/crew.rs` | o registro de servidores (`crew-servers.json`), os comandos Tauri `crew_servers_*` (`crew_servers_set_admin` guarda o endereço de administrador), `crew_server_install`, `crew_browsers_status`, `crew_browsers_install`, `crew_agent_*` |
| Interface do Desktop | `apps/desktop/src/lib/crew/` e `apps/desktop/src/lib/stores/crew.svelte.ts` | workspace, quadro, editor, configurações, onboarding, automações, o catálogo de aparências |
| Mobile | `apps/mobile/lib/features/crew/` | aba do crew, chat, perfil, automações, transporte do relay |
| Relay de push | `services/push-relay/` | o relay opcional |

**Regra de nomes no protocolo.** Uma operação em um crewmate é `crewmate_<verb>` (`crewmate_create`, `crewmate_task`, `crewmate_triggers_set`); uma operação no nível do crew é `crew_<verb>` (`crew_settings_get`, `crew_github_login_start` e, no relay do celular, `crew_servers`, `crew_call`, `crew_event`). Os eventos push seguem a mesma regra (`crewmate_updated`). Os campos JSON usam `crewmate_id`, e as listas usam `"crewmates"`. Os comandos Tauri do desktop são `crew_servers_*`, `crew_server_install`, `crew_browsers_*`, `crew_agent_*`. As chaves de painel PTY são `crew:<crewmate id>:<task id>`.

**No disco** (`<hostd dir>` é `~/.atlas` em um Mac, `<crew home>/.atlas` em um crew server):

```text
<hostd dir>/crew.json                  crew-level settings
<hostd dir>/crew-audit.jsonl            crew-wide audit (crewmates created/deleted, pause/resume)
<hostd dir>/capture/                      package.json, package-lock.json, runner.mjs, detectors.mjs, node_modules/playwright, browsers/ (atlas crew browsers install)
<hostd dir>/crew/<crewmate id>/
    crewmate.json                      definition (0600)
    tasks.jsonl                        task log: a full record per state change (last one per id wins) plus one `{"step_of", "step"}` line per live step; compacted to one record per task once it has doubled past 4 MiB
    tasks.idx                          the task log's index (order and latest state per task, plus a fingerprint of the log), so a restart parses only what was appended since; a missing or mismatching one is rebuilt from the log
    attachments/<task id>/<name>       a task's images (0600): sent with it, or screenshots the agent took (`capture`)
    uploads/<upload id>/               pieces of an image on its way to a task (removed when the task takes it, or after 1 hour without a piece)
    memory.md                          memory, 64 KiB max
    session.json                       agent session to resume
    secrets.json                       secrets (0600)
    usage.jsonl                        per-task usage records; past 1 MiB it is rotated into usage-<YYYY-MM-DD>.jsonl.gz (the day of its newest record), 400 rotated files kept; usage totals sum all of them
    audit.jsonl (+ .1 … .4)            append-only audit, rotated at 1 MiB
    ui-gate/<task id>/                 the UI gate's shots and ui-gate.json
    dom/<task id>/                     element boxes of `capture {dom: true}` (JSON, one per combination)

    work/                              git worktree of the project
    <trigger id>_seen.jsonl            GitHub events already handled (newest ids kept, 256 KiB max)
<hostd dir>/projects/<slug>/           shared clone per Git project
~/.atlas/desktop/crew-servers.json     Atlas Desktop's list of crew servers ([{host, name, admin?: {target, port?}}])
```

Os logs `*.jsonl` resistem a falhas: os leitores vão linha por linha e decodificam com perda, então uma linha cortada ou um byte que não é UTF-8 custa só aquele registro e nunca os seguintes; quem escreve primeiro encerra uma última linha cortada, para que o próximo registro não fique colado nela; o `tasks.jsonl` é lido por um índice em memória (o arquivo é analisado uma vez por processo, depois só o que foi acrescentado), então listar crewmates e a verificação de cancelamento de 30 s não leem nada do disco; os arquivos gravados de forma atômica (`crewmate.json`, `crew.json`, `secrets.json`, um log compactado) são sincronizados com o disco, junto com a pasta deles, antes da renomeação. Uma linha que uma versão não consegue ler é mantida pela compactação, não descartada.

Testes que fixam o contrato: testes em Rust gravam as fixtures JSON a partir dos serializadores reais (`UPDATE_FIXTURES=1 cargo test -p atlas-host`) em `apps/mobile/test/fixtures/` (`crewmate_list.json`, `crewmate_tasks.json`, `crewmate_task_summarized.json`, `crewmate_task_resumed.json`, `crewmate_task_event.json`, `crewmate_task_attachments.json`, `crewmate_task_prove_ui.json`, `task_actions.json`, `crew_github_status.json`, `crew_engine_status.json`, `crew_engine_login.json`) e em `apps/desktop/src/__fixtures__/` (`crewmate_tasks.json`, `crewmate_task_attachments.json`, `crewmate_task_prove_ui.json`, `crew_telegram_status.json`); os testes do desktop e do mobile as leem, então um campo renomeado em um lado falha no outro. Nunca edite uma fixture à mão.

## Referência

### Ambiente

| Variável | Onde | Efeito |
|---|---|---|
| `ATLAS_CREW_HOST=1` | a unit | liga o subsistema do crew; sem ela, todo comando `crewmate_*` e `crew_*` dá erro |
| `ATLAS_HOSTD_IDLE_SECS=never` | a unit | desativa a saída do hostd por inatividade, para o serviço continuar no ar |
| `ATLAS_PUSH_RELAY` | a unit | `off` desativa o push; uma URL seleciona um relay próprio (o build do app precisa usar o mesmo) |
| `ATLAS_CREW_BINARY` | sua máquina | o mesmo que `atlas crew install --binary` |
| `ATLAS_CREWMATE_ID` | dentro de uma tarefa | o crewmate que a executa (lido por `atlas-crew-mcp`) |

### A unit do systemd

```ini
[Unit]
Description=Atlas Crew (atlas hostd)
After=network-online.target
Wants=network-online.target
[Service]
User=atlas-crew
Environment=ATLAS_HOSTD_IDLE_SECS=never
Environment=ATLAS_CREW_HOST=1
Environment=PATH=<home>/.atlas/bin:<home>/.bun/bin:<home>/.local/bin:<home>/.opencode/bin:<home>/.npm-global/bin:/usr/local/bin:/usr/bin:/bin
ExecStart=/usr/local/lib/atlas-crew/atlas hostd
Restart=always
RestartSec=5
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=<home> -/var/lib/atlas-cm
PrivateTmp=true
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictSUIDSGID=true
MemoryMax=80%
TasksMax=4096
[Install]
WantedBy=multi-user.target
```

`User=` é `atlas-bot` e `<home>` é `/var/lib/atlas-bot` em um servidor que já tinha esse usuário.

O que o hardening faz e o que ele custa (toda diretiva existe no systemd 245, Ubuntu 20.04, o servidor mais antigo que o instalador aceita):

| Diretiva | Efeito |
|---|---|
| `ProtectSystem=strict` | todo o sistema de arquivos fica somente leitura, exceto `ReadWritePaths` e `/dev`, `/proc`, `/sys` |
| `ProtectHome=read-only` | `/home`, `/root` e `/run/user` ficam somente leitura: um crewmate não consegue alterar ali os arquivos de outros usuários |
| `ReadWritePaths=<home> -/var/lib/atlas-cm` | a home do crew continua gravável, onde quer que esteja, e a pasta pai dos crewmates também, quando o [isolamento](#um-usuário-linux-por-crewmate) criou uma (`-`: sem erro quando ela não existe). Tudo que os agentes instalam fica dentro dela e está no `PATH` (`~/.atlas`, `~/.bun`, `~/.local`, `~/.opencode`, `~/.npm-global`), então os agentes continuam funcionando |
| `ProtectKernelTunables=true`, `ProtectControlGroups=true` | `/proc/sys`, `/sys` e a árvore de cgroups ficam somente leitura |
| `RestrictSUIDSGID=true` | o serviço não consegue criar arquivos setuid nem setgid |
| `MemoryMax=80%` | o kernel mata o serviço (o hostd e todos os agentes sob ele) quando passa de 80% da memória do servidor, em vez de derrubar o host. Exige cgroup v2 (o padrão desde o Ubuntu 21.10 e o Debian 11); no cgroup v1 o limite é ignorado |
| `TasksMax=4096` | no máximo 4096 processos e threads, o que barra uma fork bomb. Um build muito paralelo pode chegar a esse limite: aumente-o com um drop-in (`systemctl edit atlas-crew`) |

Para mudar um limite sem reinstalar, use um drop-in; o `atlas crew install` reescreve o arquivo da unit, mas não o `/etc/systemd/system/atlas-crew.service.d/`.

### Comandos do protocolo

O mesmo socket NDJSON de todo comando do hostd (`~/.atlas/hostd.sock`, ou o do crew server). Os clientes verificam antes se `info.caps` tem `"crew"`.

| Comando | Argumentos | Resposta |
|---|---|---|
| `crewmate_list` | `{}` | `{crewmates:[…], paused}` |
| `crew_pause`, `crew_resume` | `{}` | `{}`; veja [Pausar e retomar](#pausar-e-retomar). Emite o push `{"ev":"crew_paused","paused":bool}` (escopo `act` do Mobile) |
| `crewmate_create` | `{name, role, agent, approval?, mode?, workdir?, look?, engine?, project?}` | o crewmate; sem `mode` (nem o legado `approval`) começa no `default_approval` do crew (`bypass` para um engine que não consegue perguntar) |
| `crewmate_update` | `{id, name?, role?, agent?, approval?, look?, spoken_replies?, …}` | o crewmate; `spoken_replies` pede ao agente dele uma linha `spoken:` ([Respostas faladas](#respostas-faladas)) |
| `crewmate_set_approval` | `{id, mode}` (`ask`, `edits`, `bypass`) | o crewmate; só clientes do dono (escopo `approve` no Mobile); recusas codificadas `crew.approval.bypass_disabled`, `crew.approval.unsupported_engine`, `crew.approval.forbidden`; veja [Modos de permissão](#modos-de-permissão) |
| `agent_approve` | `{pane, req, allow, scope?, comment?}` | `{}`; `scope` é `once` (padrão) ou `task`; `comment` chega ao agente |
| `agent_approve_voice` | `{pane, req, transcript, lang?}` | `{verdict, comment?, reason?}`; escopo Mobile `approve`. Veja [Respostas por voz aos cards](#respostas-por-voz-aos-cards) |
| `crewmate_action_answer_voice` | `{id, task, transcript, lang?}` | `{verdict, choice?, reason?}`; escopo Mobile `act` |
| `crewmate_delete` | `{id}` | `{}`; cancela as tarefas em execução e na fila e espera o turno em execução terminar, remove a worktree do Git e as branches `atlas/<id>/…` do clone compartilhado, e depois a pasta do crewmate. Um turno que ainda grave depois disso é recusado, então a pasta não é recriada |
| `crewmate_task` | `{id, prompt, source, tz?, attachments?, ui?, dictated?}` | `{task}`; `ui: true` a torna uma [tarefa de UI](#tarefas-de-ui-e-prova); `dictated: true` avisa o agente de que o prompt foi ditado ([Ditado](#ditado)); `attachments` são até 4 imagens, `[{name, data_b64}]` ou `[{name, upload}]`, veja [Imagens nas tarefas](#imagens-nas-tarefas) |
| `crewmate_attachment_put` | `{id, upload, seq, data_b64, last?}` | `{staged, last}`; um pedaço (no máximo 512 KiB brutos) de uma imagem grande demais para uma linha do protocolo, com os pedaços em ordem a partir de `seq` 0. Escopo `act` do Mobile |
| `crewmate_attachment` | `{id, task, name, offset?, limit?}` | `{name, mime, bytes_len, offset, data_b64, last}`; um pedaço (no máximo 512 KiB brutos) de uma imagem armazenada, sendo `name` um dos `attachments` da tarefa. Escopo `read` do Mobile |
| `crewmate_tasks` | `{id, limit?}` | `{tasks:[…]}` da mais nova para a mais antiga, padrão 50 |
| `crewmate_cancel` | `{id, task}` | `{}` |
| `crewmate_answer` | `{id, ask_id, key?, text?}` | responde a uma pergunta de `ask_user` |
| `crewmate_action_answer` | `{id, task, value, attachments?}` | responde a uma ação em espera: um segredo, um texto, uma escolha, `add`/`skip`, `override`/`retry` para uma recusa; `attachments` (até 4 imagens, como as de uma tarefa) só com `retry` em um card `refused` ou `prove_ui` |
| `crewmate_memory`, `crewmate_memory_set` | `{id}`, `{id, text}` | `{text}`, `{}` (64 KiB no máximo) |
| `crewmate_routine_set` | `{id, routines}` | `{}`; substitui a lista, um cron inválido é rejeitado |
| `crewmate_triggers_set` | `{id, triggers[]}` | `{}`; veja [Automações](#automações) |
| `crewmate_secret_set` | `{id, key, value}` | `{}` |
| `crewmate_usage` | `{id?, since?}` | registros de uso |
| `crewmate_audit` | `{id, since?, limit?, kind?}` | `{entries:[…]}` da mais nova para a mais antiga, padrão 200; `since` em segundos unix. Escopo `read` do Mobile |
| `crew_audit` | `{since?, limit?, kind?}` | o log de todo o crew, no mesmo formato |
| `crew_telegram_status`, `crew_telegram_set`, `crew_telegram_link` | `{}`, `{token?}`, `{}` | `{configured, chats}`, `{}`, `{code}` (só no desktop; o token nunca é devolvido) |
| `crew_voice_status` | `{}` | `{enabled, engine:{kind, label, model, ready, detail}\|null, max_seconds, audio, languages, replacements}`; escopo `read` no Mobile |
| `crew_voice_set` | `{enabled?, engine?, replacements?}` | o mesmo de `crew_voice_status`; só o dono (`crew.voice.forbidden`), recusado para celulares. Veja [Ditado](#ditado) |
| `crewmate_voice_put` | `{upload, piece_b64, last?}` | `{upload, bytes}`; `upload: ""` abre um (o id é do hostd), partes de até 512 KiB de um WAV PCM mono de 16 kHz; recusado para celulares. Falhas `crew.voice.too_long`, `crew.voice.bad_audio` |
| `crewmate_transcribe` | `{id, upload, language?, partial?}` | `{text, raw, engine:{kind, label}, language, duration_ms, partial}`; falhas `crew.voice.off`, `crew.voice.not_installed`, `crew.voice.engine_failed` |
| `crew_settings_get`, `crew_settings_set` | `{short_replies, short_replies_model?}` | configurações do crew |
| `crew_engine_status`, `crew_engine_login` | `{refresh?}`, `{engine}` | `{engines:[{engine, state, reason?}]}` com `state` `signed_in` / `needs_signin` / `not_installed` / `unknown`; `{engine, pty}` (um terminal que roda só o login daquele engine; `unknown engine` para qualquer nome fora da lista de permitidos). Escopo `read` / `act` do Mobile; o login nunca passa pelo relay. Push `{"ev":"crew_engine_auth","engine","state"}`. Veja [Entrar pelo celular](#entrar-pelo-celular) |
| `crew_github_status`, `_login_start`, `_login_poll`, `_logout`, `_connect_resume` | `{}` | veja [GitHub](#github) |
| `push_register`, `push_unregister` | `{token, platform:"fcm", grant}`, `{token}` | `{}`; `grant` é o que o relay emitiu para o token (opcional no protocolo para apps mais antigos que a autenticação do relay, que então não recebem notificações) |
| `mobile_invite`, `mobile_invites`, `mobile_invite_dismiss` | `{url, name}`, `{}`, `{url}` | convites só do desktop para celulares (TTL de 120 s) |
| `crew_servers`, `crew_call`, `crew_event` | `{}`, `{server, cmd, args}`, push | relay do celular para o crew server |

### Objeto crewmate

```json
{
  "id": "a1b2c3d4",
  "name": "Reviewer",
  "role": "Review PRs and leave inline comments.",
  "agent": "claude",
  "approval": "ask",
  "mode": "ask",
  "state": "idle",
  "queued": 0,
  "running_task": { "id": "e5f6a7b8", "pane": "crew:a1b2c3d4:e5f6a7b8" },
  "last": { "text": "Done. Left 3 comments.", "state": "done", "ts": 1729612800 },
  "pending_approval": { "req": "r1", "tool_name": "Bash", "tool_input": {}, "pane_key": "crew:a1b2c3d4:e5f6a7b8" },
  "triggers": [
    {
      "id": "t1", "kind": { "type": "schedule", "cron": "0 9 * * 1-5" },
      "enabled": true, "prompt": "Review the open PRs and summarize.",
      "tz": "America/Sao_Paulo", "last_fired": 1729612800, "next_fire": 1729699200
    }
  ]
}
```

- `state`: `idle`, `working` (uma tarefa está rodando) ou `needs_you` (uma aprovação de ferramenta está pendente, ou uma tarefa está parada em um card).
- `running_task` existe só enquanto uma tarefa roda; `pane` é a chave para anexar um terminal.
- `last`: o resultado, o erro ou o prompt da tarefa mais recente (os primeiros 140 caracteres), o estado final e a hora.
- `pending_approval` existe só em `needs_you`; uma tarefa parada em qualquer card (`waiting_task`) também é `needs_you`, embora o runner dela continue ocupado, então `running_task` está presente ali e o Stop funciona. Um servidor anterior a isso (1.5.0-alpha.26 e antes) dizia `working`; o desktop corrige isso onde a resposta ainda mostra o card.
- `waiting_task` (ausente, a menos que uma tarefa esteja parada em um card): `{id, action, result, attachments}`, o card em que a tarefa está (a que o runner segura, ou a parada mais recente) e as imagens que o agente tirou para ela (só as do agente, como registros). O Captain's call do Atlas Mobile lista a partir dele um crewmate com um card `refused` ou `prove_ui`, com Retry e Override ali mesmo. A presença dele faz `state` ser `needs_you`, seja qual for o tipo de card.
- `spoken_replies` (bool, sempre presente): o agente é instruído a terminar as respostas com uma linha `spoken:`.
- `triggers` está sempre presente; `next_fire` é calculado pelo servidor.

### Objeto de tarefa

```json
{
  "id": "e5f6a7b8",
  "ts": 1729612800,
  "source": "mobile",
  "prompt": "Review the open PRs.",
  "state": "done",
  "pane": "crew:a1b2c3d4:e5f6a7b8",
  "result": "Left 3 comments on #42.",
  "error": null,
  "started_at": 1729612801,
  "finished_at": 1729612900,
  "steps": [{ "ts": 1729612820, "text": "Reading PR diff…" }]
}
```

- `source`: `mobile`, `desktop`, `telegram`, `routine` ou `trigger`.
- `state`: `queued`, `running`, `waiting_action`, `done`, `failed` ou `cancelled`.
- `steps` pode estar vazio; `started_at` e `finished_at` ficam ausentes até serem atingidos. Outros campos opcionais: `reply`, `reply_full`, `summarized`, `title`, `preparing`, `ask`, `action`, `answer`, `usage`, `trigger_id`, `tz`, `resumed` (um reinício a interrompeu e ela começou de novo) e `restarts` (quantos).
- `attachments` (ausente sem imagens): as imagens enviadas com o prompt, `[{name, mime, bytes_len, sha256, path, source?}]`. `name` é um nome de arquivo seguro e único dentro da tarefa, `mime` vem dos bytes do próprio arquivo, `path` é onde o agente a encontra na pasta de trabalho dele (`.atlas/attachments/<task id>/<name>`). Os bytes nunca fazem parte de uma tarefa: busque-os com `crewmate_attachment`. Veja [Imagens nas tarefas](#imagens-nas-tarefas). `source` é `"agent"` para uma captura que o crewmate fez e fica ausente para o que você enviou.
- `spoken` (string, sempre presente, `""` até a tarefa terminar ou quando não há nada a ler): o que um cliente lê em voz alta ([Respostas faladas](#respostas-faladas)).
- `ui` (ausente, a menos que seja true): a tarefa é uma [tarefa de UI](#tarefas-de-ui-e-prova).
