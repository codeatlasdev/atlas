---
title: "Instalação"
source: installation.md
source_sha: "4ba679a5bffe54737f967ed9dd30de9ce8ad8db9"
translation: machine
---

# Instalação

## Requisitos

| | |
|---|---|
| SO | macOS 13+ em Apple silicon, Linux x86_64 com glibc 2.35+ (Arch, Manjaro, Fedora, Debian 12, Ubuntu 22.04 e mais novos) ou Windows 10 (1809+) / 11 em x64, para a CLI pronta e o Atlas Desktop. Macs Intel e Linux em ARM estão a caminho; enquanto isso, [compile a partir do código-fonte](#compilar-a-partir-do-código-fonte). |
| `docker` | Opcional. Só é necessário quando o seu `atlas.yaml` tem um bloco `infra:` (OrbStack e Docker Desktop funcionam). |
| `git`, `gh` | Opcional. O Atlas Desktop usa o `git` para worktrees e a CLI `gh` para pull requests e checks. |

## Instalar uma versão

O instalador baixa o binário da última versão estável no GitHub, confere o SHA-256 e roda o `atlas install` do próprio binário: ele copia o `atlas` para `~/.atlas/bin/atlas`, adiciona esse diretório ao `PATH` de todo shell detectado (fish, zsh, bash) e instala as skills de agente que vêm no pacote. Quando essa versão estável não tem build para a sua plataforma (nenhuma versão Stable tem build para Linux ainda: o Linux sai a partir da 1.5.0-alpha.7), ele parte do build mais novo do canal padrão (Alpha); com `ATLAS_CHANNEL=stable` ou `beta`, ele para e avisa. Precisa de `curl`, `tar` e `sha256sum` (ou `shasum`), nunca usa `sudo` e encerra com uma mensagem em qualquer sistema que não seja macOS em Apple silicon ou Linux x86_64.

```bash
curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh
```

No Linux com sessão gráfica (`WAYLAND_DISPLAY` ou `DISPLAY` definido), o script também instala o Atlas Desktop (`atlas desktop --no-open`: o AppImage em `~/.local/bin/atlas-desktop` e uma entrada no lançador), então o app já está no lançador quando ele termina. Pule essa etapa com `ATLAS_NO_DESKTOP=1`; um download que falha nunca derruba a instalação da CLI e o script avisa para rodar `atlas desktop` depois. Servidores (sem display) e macOS recebem só a CLI, como antes.

No Windows, no PowerShell (veja [Windows](#windows)):

```powershell
irm https://atlas.codeatlas.com.br/install.ps1 | iex
```

O `scripts/bootstrap.sh`, no repositório, vai além: ele também roda o `atlas install` (entradas no PATH e skills do pacote) e oferece configurar qualquer agente que encontrar (omp, Claude Code, Codex). Ele também precisa só do `curl`, sem `gh`:

```bash
curl -fsSL https://raw.githubusercontent.com/codeatlasdev/atlas/main/scripts/bootstrap.sh | bash
```

### Instalar uma versão beta ou alpha

`ATLAS_CHANNEL=stable|beta|alpha` escolhe o [canal de versões](versioning.md#canais). Sem ela, você recebe o **canal padrão, que é o Alpha enquanto o Atlas está em alpha** (o build mais novo do `main`; o Stable quando existir um Stable de verdade), então o comando simples acima já instala o alpha mais novo. Os dois instaladores aceitam a variável:

```bash
curl -fsSL https://atlas.codeatlas.com.br/install.sh | ATLAS_CHANNEL=stable sh   # só builds já testados
curl -fsSL https://raw.githubusercontent.com/codeatlasdev/atlas/main/scripts/bootstrap.sh | ATLAS_CHANNEL=beta bash
```

O `install.sh` instala o último Stable (ou, quando ele não tem build para a sua plataforma e você segue o Alpha, o build mais novo do canal padrão) e depois roda `atlas update` (canal padrão, nada salvo) ou `atlas update --channel <name>` (escolha explícita, salva), então o build mais novo chega pelo mesmo caminho de atualização verificado. O `bootstrap.sh` escolhe a versão certa sozinho.

O Beta instala o beta ou release candidate mais novo, o Alpha instala o alpha, beta ou release candidate mais novo; os dois voltam para o último Stable quando ele é mais novo (ou quando não há mais nada publicado), e o Stable instala só versões finais. O instalador lê apenas o feed público `releases.atom` (páginas antigas com `?after=` quando preciso) e o redirecionamento `releases/latest` (sem API do GitHub, então sem limite de requisições) e escolhe a maior precedência SemVer. Stable é a versão mais nova sem rótulo de pré-lançamento, nunca só o redirecionamento: enquanto o Alpha é o padrão, o "Latest" do GitHub é o alpha mais novo. Um `ATLAS_CHANNEL` explícito é salvo em `~/.atlas/update-channel`, então o `atlas update` continua seguindo esse canal; sem ele, nada é salvo e o `atlas update` segue o padrão. O que cada canal promete: [Versionamento e canais de versão](versioning.md).

Depois confira se funcionou (abra um novo shell se você mudou o `PATH`):

```bash
atlas version
```

### Instalação manual

Baixe `atlas-<version>-<platform>.tar.gz` e o `.sha256` dele na [página de versões](https://github.com/codeatlasdev/atlas/releases) (`aarch64-apple-darwin` para macOS, `x86_64-unknown-linux-gnu` para Linux) e rode:

```bash
shasum -a 256 -c atlas-*.tar.gz.sha256      # Linux: sha256sum -c atlas-*.tar.gz.sha256
tar xzf atlas-*.tar.gz
./atlas install
```

O `atlas install` copia o binário para `~/.atlas/bin/atlas` (mude com `--dir`), coloca esse diretório no `PATH` de todo shell que encontrar e instala em `~/.agents/skills/` as skills de IA que vêm no binário. Onde cada shell recebe a entrada:

| Shell | Arquivo | Por que lá |
|---|---|---|
| bash (Linux) | `~/.bashrc`, criado se não existir | um terminal inicia um bash sem login, que lê só o `~/.bashrc`; o `~/.bash_profile` das distribuições o carrega |
| bash (macOS) | o primeiro entre `~/.bash_profile`, `~/.bash_login` e `~/.bashrc` que existir | o Terminal inicia shells de login |
| zsh | `$ZDOTDIR/.zshenv`, ou `~/.zshenv` | todo zsh lê esse arquivo |
| fish | `$XDG_CONFIG_HOME/fish/conf.d/atlas.fish` (`fish_add_path -g`) | global, não universal: nada sobrevive ao arquivo |
| nushell 0.102+ | `$XDG_CONFIG_HOME/nushell/autoload/atlas.nu` | carregado em toda inicialização interativa |

Quando o `~/.local/bin` já está no seu `PATH` (a maioria dos desktops Linux), o install também cria o link `~/.local/bin/atlas` para o binário: assim o `atlas` funciona no shell de onde você rodou o instalador, em `bash -c` não interativo (agentes, scripts; o `~/.bashrc` retorna cedo nesses casos) e em apps iniciados pelo lançador. Um arquivo seu com qualquer um desses nomes nunca é sobrescrito; o instalador avisa e mostra a linha que você deve adicionar. Ele termina com o comando que coloca o `atlas` no `PATH` do shell atual, na sintaxe do seu shell de login. Não rode com `sudo`: isso instala para o root.

Os binários das versões são assinados com o Developer ID da CODEATLAS LTDA (Team ID `58UYCA3ZYA`) e notarizados pela Apple, então o Gatekeeper abre o `.dmg` e o `Atlas.app` baixados sem o aviso "não foi possível verificar", e o firewall do macOS lembra do `atlas` entre atualizações em vez de perguntar de novo. Um binário solto não carrega o ticket da Apple, então o Gatekeeper confere online, na primeira execução, um binário que você baixou pelo navegador; sem internet, rode `xattr -d com.apple.quarantine ./atlas` antes. Um build de fork, ou de uma versão feita antes de a assinatura ser configurada (veja [Publicar uma versão](release.md#assinatura-e-notarização-no-macos)), recebe assinatura ad-hoc e sempre precisa desse comando. O script instalador não precisa dele.

## Instalar o Atlas Desktop

```bash
atlas desktop
```

Na primeira vez, o comando baixa o app que combina com a versão da sua CLI, confere o checksum, instala e abre. Nas vezes seguintes, só abre.

- **macOS:** o `Atlas.app` vai para `/Applications`. Você também pode baixar à mão o `Atlas-<version>-aarch64-apple-darwin.dmg` na página da versão.
- **Linux:** em uma distribuição com pacote da versão, o `atlas desktop` instala **esse pacote** e não o AppImage: Arch e derivadas (pacote do pacman), Debian, Ubuntu e Mint (`.deb`), Fedora e RHEL (`.rpm`), openSUSE (`.rpm`), escolhido por `ID` / `ID_LIKE` em `/etc/os-release` e só quando esse gerenciador de pacotes existe na máquina e a CPU é x86_64. Ele baixa o arquivo, confere o checksum, mostra o comando (por exemplo `sudo pacman -U …`), pergunta `[Y/n]` e roda como root pelo `sudo` em um terminal, ou pelo `pkexec` sem terminal; `--yes` pula a pergunta. O pacote instala o `atlas-desktop` em `/usr/bin` com uma entrada no menu, e o seu gerenciador de pacotes o atualiza. Sem terminal e sem `--yes`, quando você responde não, quando a instalação falha (uma distribuição com WebKitGTK mais antigo do que o pacote exige), no Manjaro (ele segura bibliotecas em relação ao Arch) e em qualquer outra distribuição, o AppImage vai para `~/.local/bin/atlas-desktop`, com uma entrada "Atlas" no menu de aplicativos, e o `atlas update` o mantém na versão da CLI. `atlas desktop --appimage` força o AppImage. O `install.sh` entrega o seu terminal (`/dev/tty`) ao `atlas desktop` para que ele possa perguntar. **O Linux aarch64 ainda não tem build do Atlas Desktop** (a versão traz a CLI para essa plataforma, para crew servers): o `atlas desktop` avisa e aponta para a página de versões, em vez de pedir um arquivo que não existe; `atlas desktop --from <AppImage>` instala um build seu, e o `atlas update` o deixa como está.
- **Windows:** o instalador por usuário da versão roda em silêncio em `%LOCALAPPDATA%\Atlas` (sem direitos de administrador), com uma entrada no menu Iniciar e um desinstalador em Configurações › Aplicativos. O `atlas update` o mantém na versão da CLI.

Veja detalhes em [App desktop](desktop.md).

## Pacotes para Linux

Toda versão também traz o Atlas como pacotes nativos para Linux x86_64. Cada um instala **os dois**, a CLI `atlas` e o Atlas Desktop (`/usr/bin/atlas`, `/usr/bin/atlas-desktop`, uma entrada no menu), e puxa o WebKitGTK pelo seu gerenciador de pacotes. Baixe na [página de versões](https://github.com/codeatlasdev/atlas/releases/latest):

| Distribuição | Arquivo | Instalar |
|---|---|---|
| Arch Linux, Manjaro, EndeavourOS | `codeatlas-atlas-bin-<version>-1-x86_64.pkg.tar.zst` | `sudo pacman -U codeatlas-atlas-bin-*.pkg.tar.zst` |
| Debian, Ubuntu, Mint, Pop!_OS | `atlas_<version>_amd64.deb` | `sudo apt install ./atlas_*_amd64.deb` |
| Fedora, RHEL, openSUSE | `atlas-<version>-1.x86_64.rpm` | `sudo dnf install ./atlas-*.x86_64.rpm` (openSUSE: `sudo zypper install`) |
| Qualquer distribuição | `Atlas-<version>-x86_64-unknown-linux-gnu.AppImage` | `chmod +x Atlas-*.AppImage && ./Atlas-*.AppImage` |

No Arch, você também pode montar o pacote por conta própria: a versão anexa o `PKGBUILD` de onde ela foi gerada (`makepkg -si` ao lado dele); ele reempacota o `.deb` da versão.

No Arch, o mesmo pacote está no AUR como `codeatlas-atlas-bin` (`yay -S codeatlas-atlas-bin`), publicado pelo workflow de release a partir desse `PKGBUILD` (configuração do mantenedor: [Publicar uma versão](release.md#aur-codeatlas-atlas-bin)). Todo pacote declara o que o Atlas Desktop precisa em tempo de execução, e o CI lê essa lista do pacote gerado antes de instalá-lo: WebKitGTK 4.1, GTK 3, os plugins base e good do GStreamer (sons e mídia), a biblioteca da bandeja, `git` e `ripgrep` (pesquisa no projeto).

O gerenciador de pacotes é dono do que instalou, e o `atlas update` trabalha por meio dele (nunca sobrescreve o `/usr/bin/atlas` por conta própria). Ele pergunta a `pacman -Qo`, `dpkg -S` ou `rpm -qf` quem é dono do binário (um binário em `/usr/bin` que nenhum banco reivindica usa o primeiro gerenciador que encontrar), baixa o arquivo da versão que combina (`codeatlas-atlas-bin-*.pkg.tar.zst`, `atlas_*_amd64.deb`, `atlas-*.x86_64.rpm`) e o `.sha256`, **verifica antes de qualquer pedido de privilégio**, mostra o comando exato, pergunta `[y/N]` (`--yes` pula a pergunta, e fora de um terminal ela é obrigatória) e roda como root pelo `sudo` (ou pelo `pkexec` quando não há terminal; o botão de atualizar no app mostra o comando com um botão de copiar, e um "Instalar agora (pede sua senha)" que usa o `pkexec` depois que você confirma): `pacman -U`, `apt-get install`, `dnf install` ou `zypper install`. Ele segura o lock de atualização enquanto roda e depois atualiza as skills e as configurações de OMP / Claude Code / Codex com o binário **instalado**. `atlas update --check` sempre informa a versão instalada e a mais recente. O `atlas desktop` abre o app empacotado em vez de baixar um AppImage. Escolha um método de instalação por máquina (pacotes, ou `install.sh` + `atlas desktop`); com os dois, vale o `atlas` que vem primeiro no seu `PATH`.

O AppImage precisa do FUSE 2 para iniciar (`fuse2` no Arch, `libfuse2` no Debian/Ubuntu). Sem ele, rode `./Atlas-*.AppImage --appimage-extract-and-run`.

No Linux, o Atlas Desktop desenha os terminais com xterm.js (WebGL) em vez do terminal GPU do macOS, e as ferramentas de captura e gravação (`atlas desktop capture`, `record`) e as instâncias de teste de agente (`desktop:agent`) existem só no macOS.

## Windows

O Windows 10 (1809 ou mais novo) e o 11 em x64 recebem a mesma CLI, TUI, servidor MCP e Atlas Desktop (máquinas ARM64 rodam o build x64 emulado). Instale pelo PowerShell, sem direitos de administrador:

```powershell
irm https://atlas.codeatlas.com.br/install.ps1 | iex
$env:ATLAS_CHANNEL = 'stable'; irm https://atlas.codeatlas.com.br/install.ps1 | iex   # ou escolha um canal
```

O instalador roda tanto no Windows PowerShell 5.1, que acompanha o Windows, quanto no PowerShell 7, e precisa do `curl.exe` e do `tar.exe` que o Windows 10 1803 e mais novos incluem. Se o Stable mais novo não tem build para Windows (o 1.4.0 não tem), ele avisa e instala o alpha mais novo; com `ATLAS_CHANNEL=stable`, ele recusa e diz como pegar o alpha. O `atlas` funciona na mesma janela assim que termina; terminais que já estavam abertos precisam ser reabertos. O `irm | iex` não roda um arquivo de script, então a política de execução não atrapalha.

Ele confere o checksum da versão, coloca o `atlas.exe` em `%USERPROFILE%\.atlas\bin`, adiciona essa pasta ao `PATH` do seu usuário (abra um novo terminal depois) e instala as skills de agente do pacote. Depois o `atlas desktop` instala e abre o app. Você também pode baixar o `Atlas-Setup.exe` na página da versão e dar dois cliques: uma janela pequena instala o Atlas Desktop e, se você ainda não tem o comando `atlas`, o comando `atlas` também (`%USERPROFILE%\.atlas\bin`, adicionado ao seu PATH), e abre o Atlas. Sem direitos de administrador, sem assistente. Os arquivos para Windows **ainda não têm assinatura de código** (a assinatura pela SignPath Foundation aguarda aprovação, veja [Política de assinatura de código](#política-de-assinatura-de-código)), então o SmartScreen pode mostrar "O Windows protegeu seu computador": clique em **Mais informações** e depois em **Executar assim mesmo**. Depois que as versões forem assinadas, o SmartScreen ainda pode alertar sobre um arquivo que poucas pessoas baixaram: a reputação cresce com os downloads, e a assinatura sozinha não desliga o alerta. Desinstale em Configurações › Aplicativos (Windows 11: Configurações › Aplicativos › Aplicativos instalados) como qualquer programa: em uma instalação feita com o `Atlas-Setup.exe`, a mesma janela pergunta **Keep your Atlas data?**. Escolher apagar remove `%USERPROFILE%\.atlas` (configurações, a lista de projetos, as worktrees que o Atlas criou com qualquer alteração sem commit e o comando `atlas`) e pergunta uma segunda vez antes de fazer isso; as pastas dos seus projetos nunca são tocadas, e manter é o padrão. Uma instalação feita pelo `atlas desktop` ou pelo `atlas update` mostra o desinstalador comum do Windows. Se algo falha, a janela diz o que foi e oferece **Copy details** (também Ctrl+C); o log fica em `%TEMP%\atlas-setup.log`. Em scripts, `Atlas-Setup.exe --silent` roda sem janela (e `--uninstall --silent` remove o app e mantém os dados, a menos que você passe `--remove-data`); o instalador NSIS simples `Atlas-<version>-x86_64-pc-windows-msvc-setup.exe` (`/S`) é o que o `atlas desktop` e o `atlas update` rodam.

O que muda em relação a macOS e Linux:

- **Os comandos do `atlas.yaml` rodam pelo `cmd.exe`** (`cmd /d /s /c`, como o npm roda scripts), então `&&`, `>` e `%VAR%` funcionam; sintaxe só de POSIX (`FOO=1 cmd`, `$VAR`, `;`) não funciona. Escreva comandos que os dois shells entendam (`bun run dev`, `docker compose up -d`), ou envolva um em `bash -c '…'` se você usa o Git Bash. `worktree.setup` e `archive` rodam do mesmo jeito; `worktree.shared` liga pastas com um symlink (Modo de Desenvolvedor) ou uma junction.
- **Os terminais** rodam sobre o ConPTY e são desenhados com xterm.js (o terminal GPU existe só no macOS). Uma nova aba inicia o PowerShell 7 (`pwsh`) quando instalado, senão o Windows PowerShell, senão o `cmd.exe`; escolha outro shell ou programa por aba como de costume. Agentes instalados pelo npm (`claude`, `codex`) são encontrados pelos shims `.cmd`.
- **O terminal host não é atualizado ao vivo.** O Windows não consegue passar um console em execução para um novo processo, então depois de uma atualização o host reinicia sozinho quando nenhum terminal está rodando e, caso contrário, pergunta antes, como um host antigo em qualquer plataforma.
- O Atlas guarda caminhos com barras normais internamente (`C:/Users/me/app`, como o git os imprime).
- Não existem no Windows: captura e gravação (`atlas desktop capture`, `record`), instâncias de teste de agente (`desktop:agent`), `atlas link --desktop` e o alvo `local` do `atlas deploy` (os alvos de deploy são servidores Linux acessados por SSH, o que funciona a partir do Windows). O `atlas link` cria um symlink, que exige o Modo de Desenvolvedor.

### Política de assinatura de código

Assinatura de código gratuita fornecida pela [SignPath.io](https://about.signpath.io), certificado da [SignPath Foundation](https://signpath.org).

- **Escopo:** `atlas.exe`, `atlas-desktop.exe`, o instalador NSIS e o `Atlas-Setup.exe` de cada versão, gerados pelo GitHub Actions a partir da tag da versão e assinados pela SignPath depois que um mantenedor aprova o pedido. A assinatura **aguarda a aprovação da Foundation**; até lá, esses arquivos não são assinados.
- **Papéis da equipe:** committers e revisores são os mantenedores do repositório ([GOVERNANCE.md](https://github.com/codeatlasdev/atlas/blob/main/GOVERNANCE.md)). Os aprovadores são os membros do time do GitHub [`atlas-maintainers`](https://github.com/orgs/codeatlasdev/teams/atlas-maintainers).
- **Privacidade:** o Atlas não transfere dados para outros sistemas a menos que você peça. Sem telemetria, analytics, relatórios de falha ou contas. A única requisição que ele faz por conta própria é uma verificação de atualização, que lê as versões públicas de `codeatlasdev/atlas` no GitHub (o Atlas Desktop tem um interruptor *Verificar automaticamente* em Configurações → Geral → Atualizações); instalar ou atualizar baixa a versão do GitHub. Nada é enviado à CodeAtlas nem à SignPath. A lista completa está em [PRIVACY.md](https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md).

## Atualizar

```bash
atlas update                     # baixa a versão mais nova do seu canal, troca o binário, atualiza skills e a configuração dos agentes
atlas update --check             # só informa se existe uma versão mais nova
atlas update --check --json      # o mesmo como um objeto JSON, para scripts
atlas update channel             # lista os canais e o que cada um oferece agora (seletor numerado em um terminal)
atlas update channel beta        # passa a seguir o beta
atlas update --channel alpha     # salva o alpha e atualiza a partir dele
atlas update --downgrade         # instala o build mais antigo que o seu canal oferece (pergunta antes; veja abaixo)
```

O Atlas segue um de três [canais de versão](versioning.md#canais): `stable`, `beta` ou `alpha` (o padrão enquanto o Atlas está em alpha). A escolha fica em `~/.atlas/update-channel` e é compartilhada com o Atlas Desktop; `ATLAS_UPDATE_CHANNEL=beta` a substitui em um único processo. As atualizações nunca voltam versões por conta própria: se você muda para um canal mais conservador do que o build em uso, o `atlas update` avisa e espera até o canal passar da sua versão, a menos que você rode `atlas update --downgrade` (builds mais antigos podem não ler dados que os mais novos gravaram; ele pede confirmação, e `--yes` pula a pergunta).

O `atlas update` também leva um Atlas Desktop instalado para a mesma versão. O Atlas procura uma versão nova do seu canal em segundo plano, no máximo a cada 6 horas, e imprime um aviso de uma linha depois dos comandos quando há uma; o `atlas dev` oferece atualizar antes de abrir o painel. Defina `ATLAS_NO_UPDATE_PROMPT=1` para silenciar a pergunta. Ele se recusa a rodar enquanto você usa um [build de desenvolvimento vinculado](#vincular-um-build-de-desenvolvimento). Referência completa: [`atlas update`](cli.md#atlas-update).

## Compilar a partir do código-fonte

Você precisa de:

- a toolchain do Rust fixada em [`rust-toolchain.toml`](https://github.com/codeatlasdev/atlas/blob/main/rust-toolchain.toml) (o `rustup` a instala sozinho no primeiro comando `cargo`);
- o [`bun`](https://bun.sh), para o frontend do Atlas Desktop e os scripts do repositório;
- para o app desktop, as Xcode command line tools no macOS (`xcode-select --install`), ou o WebKitGTK 4.1 e as dependências de build dele no Linux:

  ```bash
  sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libappindicator-gtk3 librsvg xdotool   # Arch
  sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev   # Debian/Ubuntu
  sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel libxdo-devel   # Fedora
  ```

  A CLI sozinha não precisa de nenhuma delas (`cargo build --release -p atlas-cli`).

```bash
git clone https://github.com/codeatlasdev/atlas.git
cd atlas
scripts/setup.sh                 # confere as ferramentas, instala as dependências do frontend, o cargo-tauri e o cargo-deny
```

### A CLI

```bash
cargo build --release -p atlas-cli
codesign -f -s - target/release/atlas      # macOS: assinatura ad-hoc, senão o Gatekeeper mata o binário
target/release/atlas install               # copia para ~/.atlas/bin e configura o PATH
```

Compile o `atlas-cli`, não só o `atlas-tui`: o `atlas-tui` é uma biblioteca, e o `atlas-cli` é o crate que produz o binário `atlas`.

### Atlas Desktop

```bash
bun run desktop:build            # macOS: .app + .dmg; Linux: .deb, .rpm e .AppImage; Windows: o .exe de setup NSIS; tudo em target/release/bundle/
atlas desktop --from target/release/bundle/macos/Atlas.app                 # macOS
atlas desktop --from target/release/bundle/appimage/Atlas_*.AppImage       # Linux
atlas desktop --from target/release/bundle/nsis/Atlas_*-setup.exe          # Windows (o instalador NSIS; o Atlas-Setup.exe o envolve: docs/release.md)
```

No Linux, compile a CLI primeiro (`cargo build --release -p atlas-cli`): o `.deb` e o `.rpm` incluem o `target/release/atlas`. No Windows, você precisa das Visual Studio Build Tools (carga de trabalho C++) e do WebView2 (já vem no Windows 11); o build da versão também coloca o `atlas.exe` no instalador (`release.yml`, `build-windows`).

Para o trabalho do dia a dia no app, use o ciclo com hot reload: `bun run desktop:dev`. Mais em [Contribuir](contributing.md#desenvolver-o-app-desktop).

### Vincular um build de desenvolvimento

Para fazer o `atlas` instalado (e o `/Applications/Atlas.app`) rodar o seu checkout:

```bash
atlas link                       # build + symlink da CLI de debug
atlas link --desktop             # também compila um Atlas.app de debug e o vincula
atlas unlink                     # volta para o build da versão
```

Enquanto está vinculado, o `atlas update` se recusa a sobrescrever o seu build.

## Desinstalar

```bash
atlas setup omp --uninstall --apply   # e claude-code / codex, se você os configurou
atlas uninstall --dry-run             # lista o que sairia; não muda nada
atlas uninstall                       # remove o Atlas, mantém seus dados (pergunta antes; -y pula)
atlas uninstall --purge               # também apaga ~/.atlas: configurações, projetos, worktrees
```

O `atlas uninstall` remove o que o Atlas instalou: o comando `atlas` (`~/.atlas/bin`), o app desktop (no macOS, o `Atlas.app`, ou só o link quando é um build de desenvolvimento do `atlas link --desktop`; no Linux, o AppImage, os arquivos `.atlas-desktop.prev` / `.atlas-desktop.new` que uma atualização interrompida deixou ao lado dele, e a entrada do lançador com o ícone, que saem mesmo que você já tenha apagado o AppImage à mão, desde que a entrada comece com `~/.local/bin/atlas-desktop`), as linhas `# Atlas CLI` que ele adicionou aos arquivos de inicialização do seu shell (`~/.zshenv` ou `$ZDOTDIR/.zshenv`, `~/.bashrc` ou o seu arquivo de login do bash, o `conf.d/atlas.fish` do fish e a entrada universal `fish_user_paths` que as versões até a 1.5.0-alpha.25 gravavam, o `autoload/atlas.nu` do nushell; uma linha sua que apenas menciona `.atlas` nunca é tocada), o link `~/.local/bin/atlas` quando aponta para esta instalação, o cache e o lock de atualização e os sockets do terminal host (ele pede a um host em execução que pare antes). Ele **mantém** `~/.atlas/desktop` (configurações e a lista de projetos), `~/.atlas/worktrees` (worktrees podem conter trabalho sem commit), o canal de atualização, os temas e o plugin do omp, e, no Linux, os dados do próprio app (`$XDG_DATA_HOME/br.com.codeatlas.atlas`, o armazenamento do WebKit com as configurações de interface do app, e `dev.codeatlas.atlas`, a cópia de antes da 1.6; `$XDG_DATA_HOME/atlas/crashes`, os relatórios de falha; `$XDG_CONFIG_HOME/br.com.codeatlas.atlas`, o estado das janelas) e avisa o que manteve; `--purge` apaga todo o `~/.atlas` e também essas pastas do Linux. Ele lista tudo primeiro e pergunta antes de mudar qualquer coisa (`--yes` pula a pergunta; sem terminal e sem `--yes`, ele se recusa). Uma etapa que não pode ser feita (um arquivo em uso) é relatada e o resto continua; rodar de novo termina o trabalho. Quando o `atlas` (ou o `/usr/bin/atlas-desktop`) pertence a um pacote da distribuição, o uninstall nunca apaga os arquivos do pacote: a lista mostra o pacote e o comando que o remove (`sudo pacman -R codeatlas-atlas-bin`, `sudo apt remove atlas`, `sudo dnf remove atlas`), e depois da sua confirmação ele roda essa remoção como root (`sudo` em um terminal, `pkexec` sem um; onde nenhum dos dois pode rodá-la, a etapa falha e imprime o comando para você rodar à mão). No Windows, desinstale o **Atlas** em Configurações › Aplicativos (ele pergunta se deve manter os seus dados), e o `atlas uninstall` também tira `~\.atlas\bin` do `PATH` do seu usuário. As skills copiadas para `~/.agents/skills/` e `~/.claude/skills/` ficam onde estão; remova-as com as ferramentas do seu agente se quiser apagá-las.
