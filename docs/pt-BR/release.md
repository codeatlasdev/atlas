---
title: "Publicar uma versão"
source: release.md
source_sha: "b05330889f2c69253477a2659b5490424b464ee7"
translation: machine
---

# Publicar uma versão do Atlas (mantenedores)

O GitHub Actions compila e publica as versões em runners hospedados quando um mantenedor envia uma tag `v*`, o que o `bun run release` faz. O CI roda em pull requests e em pushes para `main`/`release/**`; o workflow Release espera o CI do commit com a tag passar antes de compilar. O comando faz push de `main` (ou `release/X.Y`) e da tag, o que os guards de agente recusam (`docs/contributing.md`, seção sobre os guards de agente): rode como `ATLAS_MAINTAINER_OK=push bun run release <channel>`.

O que os canais significam para os usuários, e o que o Atlas promete em cada um, está em [Versionamento e canais de versão](versioning.md). Esta página mostra como criar cada versão.

O Atlas Mobile é publicado à parte, com tags próprias `mobile-v*`: `bun run mobile:release X.Y.Z`; depois, a aprovação de um mantenedor envia o build testado para as lojas. Passos: [Atlas Mobile, seção de versões](mobile.md#versões-testers-primeiro-depois-as-lojas).

## O fluxo

```text
main        ── alpha ── alpha ── alpha ──────────────────────────▶   bun run release alpha
   \
    release/1.4 ── beta ── fix ── beta ── (rc) ── stable ── fix ── patch
```

| Passo | Onde rodar | Comando | Resultado |
|---|---|---|---|
| Próxima alpha | `main` | `bun run release alpha` | `v1.4.0-alpha.1`, `.2`, … |
| Primeira beta | `main` | `bun run release beta` | cria `release/1.4` a partir de `HEAD` e cria nela a tag `v1.4.0-beta.1` |
| Próxima beta | `release/1.4` | `bun run release beta` | `v1.4.0-beta.2`, … |
| Release candidate (opcional) | `release/1.4` | `bun run release rc` | `v1.4.0-rc.1`, … (precisa de uma beta antes) |
| Promover para Stable | `release/1.4` | `bun run release stable` | `v1.4.0`, a beta que passou pelo soak mais os arquivos de versão |
| Hotfix | `release/1.4` | `bun run release patch` | `v1.4.1` |

Todo comando aceita `--dry-run`, `--skip-checks`, `--force-soak` e `--bump major`. Uma versão explícita também funciona (`bun run release 1.4.0-beta.2`) e segue as regras do seu canal; ela precisa ser um SemVer válido com o rótulo `alpha`, `beta` ou `rc` e um **ponto antes do contador**, e ser mais nova que toda tag existente.

## Comandos

Todos a partir da raiz do repositório (scripts do `package.json`, rodados com `bun run <name>`):

| Comando | O que faz |
|---|---|
| `bun run check` | O gate local (veja [Contribuir](contributing.md#o-gate)); o CI roda as mesmas verificações. |
| `bun run release <alpha\|beta\|rc\|stable\|patch\|X.Y.Z[-pre.N]>` | Cria e publica uma versão: exige CI verde em `HEAD`, aumenta a versão, faz commit, cria a tag e faz push; o push da tag inicia o workflow Release, que o script acompanha. |
| `bun run release:dry` | `scripts/release.sh --dry-run`: mostra cada passo e não muda nem envia nada. Passe o pedido depois dele: `bun run release:dry alpha`. |
| `bun run release:test` | Os testes do próprio ferramental de release: `bun test scripts` (regras de versão, branch, soak e diff-guard, scripts de CI) e `python3 -m unittest scripts/test_generate_manifest.py`. |
| `bun run release:publish vX.Y.Z[-pre.N]` | Compila e publica de novo uma tag existente: dispara o `release.yml` a partir de `main` com essa tag e acompanha a execução; depois faz o deploy do site. |

Você pode consultar as regras diretamente, sem mudar nada:

```bash
bun scripts/release-version.ts next --request alpha --json   # {"version":"1.4.0-alpha.1","tag":…,"channel":"alpha","errors":[]}
bun scripts/release-version.ts next --request stable --json  # na main: {"errors":["stable releases are cut from release/X.Y …"]}
bun scripts/release-version.ts diff-guard --from v1.4.0-beta.2   # lista os arquivos que uma promoção não pode alterar
```

## Pré-requisitos

- A branch certa, árvore de trabalho limpa, em sincronia com a branch de `origin` (o script verifica; o `--dry-run` relaxa as verificações de limpeza e sincronia, e a falta da seção no `CHANGELOG.md` vira um aviso).
- `gh` autenticado com acesso de escrita a `codeatlasdev/atlas` (`gh auth login`): ele lê o resultado do CI, dispara e acompanha a execução do Release. A publicação em si usa o `GITHUB_TOKEN` do próprio workflow, então nenhum token é salvo em lugar nenhum.
- `bun`, `cargo` e `git`. Nada é compilado na sua máquina.
- Uma **execução verde do `CI` em `HEAD`**: envie a branch (ou faça merge do pull request) e deixe o CI terminar antes. O `--skip-checks` dispensa essa exigência.
- **Uma execução completa, não uma pulada.** Em um push, o CI pula o que a mudança não toca: um `HEAD` só de docs (o commit das notas da versão) informa sucesso com todos os jobs pulados, e uma execução em `main` é cancelada quando chega o próximo push, então o código que entrou logo antes pode nunca ter tido uma execução completa na `main`. Antes de publicar, rode `gh workflow run CI --ref main` (um dispatch não tem base, então todo job roda em todo sistema operacional) e leia cada job. Em 2026-11-15 isso encontrou três bugs que as execuções por PR tinham deixado passar (#375).
- Escreva a seção `## [X.Y.Z[-pre.N]]` do `CHANGELOG.md` **antes** de rodar o comando: as notas da versão saem dela.

## O que o `bun run release` faz

1. **Verificação de retomada.** Se `HEAD` já é uma tag `vX.Y.Z[-pre.N]` enviada e o GitHub Release dela não existe (uma publicação anterior falhou), o script só dispara e acompanha o workflow Release dessa tag (o mesmo que `bun run release:publish vX.Y.Z`) e encerra. Ele nunca aumenta a versão de novo. Uma stable promovida mantém a procedência: o workflow lê de novo a linha `Promoted from:` da mensagem da tag.
2. **Plano** (`scripts/release-version.ts`, com testes unitários). Lê o `Cargo.toml`, toda tag `v*` (locais e em `origin`), as branches e a branch atual; depois decide a próxima versão ou diz por que não pode:
   - `alpha`: só na `main`. A base é a próxima minor depois da stable atual; ela sobe uma minor quando existe `release/<base>` ou qualquer tag `beta`, `rc` ou stable dessa base (`--bump major` inicia uma nova major). O contador é o maior `-alpha.N` existente **nas tags** mais um, não o que o `Cargo.toml` diz.
   - `beta`: na `main`, cria `release/X.Y` e cria nela a tag `X.Y.0-beta.1`; em `release/X.Y`, incrementa `beta.N`, e é recusada depois que existe um `rc`.
   - `rc`: em `release/X.Y`, depois de pelo menos uma beta.
   - `stable`: em `release/X.Y`; exige uma tag beta ou `rc` dessa base, o soak abaixo e nenhuma tag stable existente.
   - `patch`: só em `release/X.Y`, `X.Y.(Z+1)`, e só depois que `X.Y.0` saiu. Na `main` ele falha e explica o fluxo de hotfix. `minor` e `major` não existem mais: use `alpha` / `beta`.
   - Versões explícitas: as mesmas regras de branch do seu canal; contadores colados ao rótulo (`alpha10`), rótulos desconhecidos, zeros à esquerda e metadados de build são rejeitados.
3. **Pré-verificação.** Árvore limpa; a branch igual à sua branch de `origin` (na primeira beta, `main` igual a `origin/main`); a tag não existe nem localmente nem em `origin`; o `CHANGELOG.md` tem uma seção `## [<version>]`; para `stable`, a **guarda de promoção** abaixo.
4. **CI verde em HEAD** (`scripts/ci-passed.sh`, dispensado com `--skip-checks`). Ele espera a execução do `CI` de `HEAD` e se recusa a continuar se ela não passou; a versão não roda o gate de novo localmente.
5. **Branch.** A primeira beta roda `git switch -c release/X.Y`.
6. **Aumento de versão.** Todo `crates/*/Cargo.toml` (inclusive o `atlas-desktop`; o `tauri.conf.json` não tem versão própria e lê a do crate) recebe a mesma versão; `cargo update --workspace --offline` atualiza o `Cargo.lock`.
7. **Commit, tag, push.** Commit `chore(release): vX.Y.Z[-pre.N]`, tag anotada e depois `git push --atomic origin <main|release/X.Y> <tag>`: a branch e a tag chegam juntas ou não chegam.
8. **Push da tag; o GitHub roda o `release.yml`** (gate → builds para macOS, Linux e Windows, pacote Arch → verificação do Linux em containers → publicação). O script acompanha a execução (`gh run watch`) e depois faz o deploy do site (mantido; ele nunca faz a versão falhar).
9. **Assinar e publicar** (`scripts/release-sign.sh <tag>`, o mesmo para todo canal, inclusive uma promoção para stable). O `release.yml` deixa a versão como **rascunho**, então nada é público ainda. O script baixa o `SHA256SUMS`, imprime o arquivo, assina com a sua chave offline, confere a assinatura contra as chaves do repositório, envia o `SHA256SUMS.sig` e só então publica o rascunho ([Assinatura de versões](#assinatura-de-versões)). Com uma chave configurada, uma versão sem assinatura nunca é publicada e o site não recebe deploy. Depois ele faz o deploy do site.
10. **Avisar quem reportou** (`scripts/release-notify.ts <tag>`, por último, depois do site). O `Fixes #N` fecha uma issue no merge, dias antes de a alpha que a leva sair, e não diz o que rodar. O script comenta `Fixed in vX (alpha channel)`, com como obtê-la, em toda issue fechada como concluída (não "não planejada", nem `type: question`, `status: wontfix` ou `status: duplicate`) entre o horário do commit da tag da versão anterior e o desta, e nos reports marcados como duplicados delas. Uma issue que já tem um comentário dele (marcador oculto `<!-- atlas-fixed-in:vX -->`) é deixada em paz, então uma nova execução ou a próxima versão nunca avisa ninguém duas vezes. Ele nunca faz a versão falhar. Para ensaiar ou repetir à mão: `bun run release:notify <tag> --dry-run` imprime uma linha por comentário (e o texto completo uma vez), `bun run release:notify <tag>` os escreve.

### Guarda de promoção (stable)

Stable é **o pré-lançamento que passou pelo soak mais os metadados de versão**. Antes de aumentar a versão, o `release.sh stable` compara a tag `beta`/`rc` mais nova da linha com `HEAD` e só permite arquivos `Cargo.toml` (o da raiz e os `crates/<name>/Cargo.toml`), `Cargo.lock` e `CHANGELOG.md`. Qualquer outro é recusado, com a lista de arquivos:

```text
✗ stable must be v1.4.0-beta.2 plus version metadata, but these files changed since it:
crates/atlas-core/src/lib.rs
Re-cut a beta/rc that includes them and let it soak, or revert them from release/1.4.
```

A mensagem da tag registra `Promoted from: v1.4.0-beta.2` e o job `publish` a copia para o manifesto como `promoted_from`, então qualquer pessoa pode rodar a mesma comparação depois.

### Tempo de soak

Durante a era alpha, o mínimo é de **48 horas** entre a tag beta/`rc` mais nova da linha e a `stable` (contadas a partir da data de criação da tag). Uma `stable` dentro da janela falha, informando a idade e como forçar:

```text
✗ soak not met: the last 1.4.0 prerelease is 10h old, stable needs at least 48h (override with --force-soak; it is recorded in the release notes)
```

O `--force-soak` deixa passar, e a ferramenta acrescenta uma linha `> Soak override: promoted after 10h …` sob o título da versão no `CHANGELOG.md`, de modo que ela vai para as notas da versão. Uma data que não pode ser lida também exige `--force-soak`; o soak nunca é presumido como cumprido. Use para correções de segurança, não por impaciência.

## Hotfixes

Corrija primeiro na `main`; depois leve o commit para a branch de release, com a procedência:

```bash
git switch main && git pull           # entregue a correção na main por um PR normal
git switch release/1.4 && git pull
git cherry-pick -x <sha>              # -x registra "(cherry picked from commit …)"
# escreva a seção "## [1.4.1]" no CHANGELOG.md e faça commit dela
bun run release patch                 # v1.4.1, stable
```

Só a linha stable mais nova recebe correções. Se um hotfix toca em código que a próxima beta também carrega, ele chega ao Beta e ao Alpha pela `main`.

## O que o workflow de release faz

`.github/workflows/release.yml`, um punhado de jobs em runners hospedados pelo GitHub (o build do Windows é uma cadeia de sete, três deles opcionais). Ele roda no push de uma tag `v*` ou à mão (`workflow_dispatch` com a entrada `tag`, que é o que o `bun run release:publish` faz). Uma terceira forma, o [somente build](#somente-build-testar-a-main-do-início-ao-fim-antes-de-uma-versão), compila e verifica tudo e não publica nada. Todo job que compila faz checkout de `refs/tags/<tag>`, e a versão vem do nome da tag (`TAG`, que é `inputs.tag` ou a ref enviada). Todo job de build começa com a action composta `.github/actions/release-toolchain` (seus pacotes do Linux, o toolchain do Rust e os alvos de cross-compilação, e `SOURCE_DATE_EPOCH` = a hora do commit), então nenhum job tem a sua própria cópia dessa configuração.

1. **`gate`** (`ubuntu-latest`): o `scripts/ci-passed.sh` espera a execução do `CI` do commit da tag e faz a versão falhar se ela não terminou em `success`. A versão nunca roda os testes de novo; ela confia na única execução do CI desse commit exato.
2. **`build`** (`macos-latest`, depois do `gate`; o `macos-14` foi aposentado pelo GitHub): `cargo build --release --locked --target aarch64-apple-darwin -p atlas-cli` com `SOURCE_DATE_EPOCH` definido como a hora do commit, `codesign` ad-hoc (para o Gatekeeper não dar SIGKILL no binário; com os secrets do Developer ID definidos, a [assinatura Developer ID](#assinatura-e-notarização-no-macos) no lugar dele), empacota `atlas-<version>-aarch64-apple-darwin.tar.gz` mais um `.sha256`. Um SBOM CycloneDX das dependências da CLI (`cargo cyclonedx`, instalado com `--locked` quando falta; o passo avisa e segue em frente se não for possível) vira `atlas-<version>-sbom.cdx.json`. Depois vem o Atlas Desktop: `cargo tauri build --bundles app,dmg` (assinado ad-hoc por meio de `bundle.macOS.signingIdentity: "-"`, ou assinado com Developer ID e notarizado, veja [Assinatura e notarização no macOS](#assinatura-e-notarização-no-macos)), empacotado como `Atlas-<version>-aarch64-apple-darwin.app.tar.gz` (o que `atlas desktop` e `atlas update` instalam) e `Atlas-<version>-aarch64-apple-darwin.dmg` (download manual), cada um com um `.sha256`.
3. **`build-linux`** (`ubuntu-22.04`, depois do `gate`): a CLI (`atlas-<version>-x86_64-unknown-linux-gnu.tar.gz`) e o Atlas Desktop por `cargo tauri build` com `tauri.linux.conf.json`: um AppImage, um `.deb` e um `.rpm`, cada um com um `.sha256`.
   **`build-linux-arm64`** (`ubuntu-22.04-arm`, um runner arm64 nativo do GitHub, depois do `gate`, em paralelo com o `build-linux`): só a CLI, `atlas-<version>-aarch64-unknown-linux-gnu.tar.gz` mais o seu `.sha256`, compilada com `cargo build --release --locked -p atlas-cli` (sem app desktop, sem pacotes). Ele existe para crew servers em arm64 (AWS Graviton, Oracle A1, Hetzner CAX): `atlas crew install` e `atlas crew update` escolhem o arquivo pelo `uname -m` do servidor. `ubuntu-22.04-arm`, não 24.04, pelo mesmo motivo de o `build-linux` usar 22.04: o binário então roda em qualquer glibc >= 2.35 (Ubuntu 22.04, Debian 12). O `manifest.json` o lista em `aarch64-unknown-linux-gnu` só com `cli` e `daemon`, e o `publish` espera por ele.
4. **`build-arch`** (container `archlinux:base-devel`, depois do `build-linux`): reempacota o `.deb` com o `makepkg` (`packaging/arch/`) em um `.pkg.tar.zst` mais o `PKGBUILD` renderizado.
5. **`verify-linux`** (`ubuntu-latest`, quatro containers em paralelo: `archlinux:latest`, `fedora:latest`, `ubuntu:24.04`, `debian:stable`; depois do `build-linux` e do `build-arch`): nada do Linux é publicado sem ter sido executado antes (#283, #312). Em um container novo, a ferramenta da própria distro instala o seu pacote (`pacman -U` no `.pkg.tar.zst`, `dnf install` no `.rpm`, `apt install` no `.deb`, então uma dependência de runtime que falta quebra aqui); depois rodam `atlas --version` (que precisa conter a versão da tag) e `atlas doctor --json`, e então **o AppImage e o `atlas-desktop` do pacote são abertos** pelo `scripts/linux-launch-check.sh` em um sway headless privado com Mesa llvmpipe (sem GPU, sem display). O script falha quando o app trava ou encerra, quando uma janela nunca é mapeada, quando uma página fica de uma cor só, quando o `WebKitWebProcess` some ou é trocado, quando há marcador de crash/EGL/CSP/console no stderr, em qualquer linha de erro no `~/.atlas/desktop/app.log` do próprio app e em um coredump (códigos de saída e motivos: o cabeçalho do script). Capturas de tela e logs são enviadas como os artefatos `verify-linux-<distro>`, com sucesso ou falha. Um container vermelho bloqueia o `publish`.
6. **`windows-binaries` → … → `build-windows`** (`ubuntu-22.04`, depois do `gate`, `cargo xwin`): a CLI, o binário do Atlas Desktop, o instalador NSIS (`Atlas-<version>-x86_64-pc-windows-msvc-setup.exe`) e o `Atlas-Setup.exe` com esse instalador embutido (veja [Instalador do Windows](#instalador-do-windows-atlas-setupexe)), como uma cadeia de jobs com os jobs opcionais de assinatura entre eles ([Assinatura de código no Windows](#assinatura-de-código-no-windows-signpath-foundation)); o último, `build-windows`, empacota `atlas-<version>-x86_64-pc-windows-msvc.tar.gz` e os instaladores, cada um com um `.sha256`.
7. **`verify-windows`** (`windows-latest`, depois do `build-windows`): os arquivos do Windows são executados, não só compilados por cross-compilação (#312). O `scripts/windows-launch-check.ps1` confere cada `.sha256`, descompacta o arquivo da CLI e roda `atlas.exe --version`, instala com `Atlas-Setup.exe --silent`, abre o `atlas-desktop.exe` instalado (uma janela, ainda de pé depois de um tempo de espera, uma tela que não é de uma cor só, `~\.atlas\desktop\app.log` vazio), desinstala com `Atlas Uninstall.exe --uninstall --silent --remove-data`, e então faz o mesmo com o instalador NSIS simples `/S` (o que `atlas desktop` e `atlas update` rodam) e o seu `uninstall.exe /S`. Um primeiro passo roda a verificação em um programa que não é um app e exige que ela falhe. Capturas de tela e logs são o artefato `verify-windows`.
8. **`verify-macos`** (`macos-latest`, depois do `build`): o `scripts/macos-launch-check.sh` confere cada `.sha256`, roda o `atlas --version` do tarball da CLI, monta o `.dmg` e verifica a assinatura de código do `Atlas.app` dentro dele, descompacta o `.app.tar.gz` (o que `atlas desktop` instala) e exige a mesma versão do bundle; depois abre o app de cada um (uma janela na tela, ainda de pé depois de um tempo de espera, `~/.atlas/desktop/app.log` vazio). Mesmo controle negativo; a evidência é o artefato `verify-macos`. O app roda a partir de uma cópia em um diretório temporário, nunca de `/Applications`.
9. **`publish`** (`ubuntu-latest`, depois dos builds e dos três jobs `verify-*`): o `scripts/generate-manifest.py` escreve o `manifest.json` (`version`, `date`, `assets`, mais `channel`, `prerelease`, `commit` e, em uma promoção, `promoted_from`); um arquivo `SHA256SUMS` cobre cada asset e o manifesto; depois vem o GitHub Release (`softprops/action-gh-release`, com o `GITHUB_TOKEN` da própria execução). O `promoted_from` vem da linha `Promoted from: …` na mensagem da tag, que o `release.sh stable` escreve. O corpo é a seção da versão no `CHANGELOG.md` seguida das instruções de instalação e atualização; em um pré-lançamento, ele abre com uma linha dizendo em qual canal o build está (e, para um build do canal padrão que o GitHub marca como "Latest", que ele ainda não é oferecido no canal Stable).
   Um GitHub Release é criado como **rascunho** (o `GITHUB_TOKEN` da própria execução, `contents: write`); ele fica público quando um mantenedor assina o `SHA256SUMS` ([Assinatura de versões](#assinatura-de-versões)). O `SHA256SUMS` montado aqui também é salvo como o artefato de workflow `sha256sums`.
10. **`attest`** (`ubuntu-latest`, depois do `publish`, nunca em uma execução somente build; `contents: read`, `id-token: write`, `attestations: write`): `actions/attest` com `subject-checksums: SHA256SUMS`, então o GitHub registra uma atestação assinada para cada asset do arquivo. Ele não pode publicar nem assinar uma versão; é a segunda camada, para quem usa o `gh attestation verify`.

Uma versão é compilada a frio de propósito: não há cache do cargo, porque uma versão nunca é compilada a partir de um cache restaurado.

**O teste completo do crew server, do início ao fim** (`.github/workflows/crew-e2e.yml`, [o que ele verifica](crew.md#teste-end-to-end-em-systemd-real)) instala um crew server em um systemd real e roda tarefas nele. Ele roda toda noite e nos pull requests que mexem no crew, e é um workflow reutilizável. Para fazer uma versão esperar por ele, acrescente um job ao `release.yml` e coloque-o no `needs` e no `if` do `publish`:

```yaml
  crew-e2e:
    name: Crew server E2E
    needs: gate
    uses: ./.github/workflows/crew-e2e.yml
```

Antes de publicar, rode `gh workflow run crew-e2e.yml --ref main` (ele também está na agenda noturna) e leia o resultado: uma versão do crew é a única coisa que nenhum outro job instala em um servidor.

As flags do GitHub vêm do canal da tag comparado com o **canal padrão** (`"default"` em `crates/atlas-core/data/channels.json`, fixado em `#[default]` no `Channel` por um teste), por meio de `scripts/release-version.ts github-flags` (com testes unitários; o workflow não tem um palpite próprio com `contains('-')`):

| Tag | Padrão `alpha` (antes do GA) | Padrão `stable` (GA) |
|---|---|---|
| `vX.Y.Z` | versão normal, **Latest** | versão normal, **Latest** |
| `vX.Y.Z-alpha.N` | versão normal, **Latest** (`prerelease: false`, `make_latest: true`) | pré-lançamento, nunca Latest |
| `vX.Y.Z-beta.N` / `-rc.N` | pré-lançamento, nunca Latest | pré-lançamento, nunca Latest |

Assim, antes do GA, o selo "Latest" do GitHub e o `releases/latest` mostram o build mais novo do canal padrão: o que recebe quem nunca escolheu um canal. **Esse redirecionamento, portanto, não é mais o canal Stable.** Os clientes nunca o usam como "a tag stable": Stable é a tag mais nova sem rótulo de pré-lançamento entre as do `releases.atom` (páginas mais antigas com `?after=<tag>` quando uma sequência de alphas preenche a janela de 10 entradas) e o redirecionamento, que é só mais um candidato. O cabeçalho das notas da versão avisa isso em uma alpha do canal padrão. O `prerelease` do manifesto é o rótulo SemVer, não a flag do GitHub. No GA, o padrão volta a ser Stable (veja [versionamento](versioning.md#disponibilidade-geral)) e a coluna da direita da tabela passa a valer sem nenhuma outra edição.

### Instalador do Windows (`Atlas-Setup.exe`)

O build do Windows produz dois instaladores de propósito:

| Arquivo | O que é | Quem executa |
|---|---|---|
| `Atlas-Setup.exe` | `crates/atlas-setup`: uma janela pequena e sem borda (desenhada por nós, **sem WebView2**, que pode ainda não existir) com o globo do Atlas, uma barra de progresso e uma linha de status; ele embute o instalador NSIS abaixo, roda esse instalador em modo silencioso e abre o Atlas. `--uninstall` é a mesma janela para a remoção. | Pessoas (o link de download, um nome estável). |
| `Atlas-<version>-x86_64-pc-windows-msvc-setup.exe` | O instalador NSIS do Tauri (por usuário, `%LOCALAPPDATA%\Atlas`). Listado no `manifest.json` como `desktop`. | `atlas desktop` e `atlas update` (`/S /D=`), scripts. |

O setup é compilado depois do bundle NSIS, com o bundle como carga: `ATLAS_SETUP_PAYLOAD=<nsis exe> cargo xwin build --release --locked --target x86_64-pc-windows-msvc -p atlas-setup` (sem a variável ele ainda compila, então `cargo build --workspace` funciona, e informa em tempo de execução que não carrega nenhum instalador). A versão compila o instalador NSIS a partir dos binários já compilados (e, com a assinatura ligada, assinados) com `cargo tauri bundle`, e o setup a partir desse instalador (assinado), um job para cada; veja [Assinatura de código no Windows](#assinatura-de-código-no-windows-signpath-foundation). O progresso é real: o instalador NSIS é silencioso, então a janela acompanha a pasta de instalação crescer em relação ao tamanho descompactado do instalador; a barra para antes do fim até o código de saída ser 0 **e** o `atlas-desktop.exe` estar no lugar. Os erros nunca fecham a janela em silêncio: ela mostra o motivo, **Copy details** e o log (`%TEMP%\atlas-setup.log`).

Depois de uma instalação bem-sucedida, o setup se copia para a pasta de instalação como `Atlas Uninstall.exe` e aponta para essa cópia o `UninstallString`/`DisplayIcon` da entrada em Configurações › Aplicativos (o `QuietUninstallString` continua sendo o `uninstall.exe /S` do NSIS). O instalador do Tauri reescreve essa entrada a cada execução, inclusive a silenciosa do `atlas update`, então o `packaging/windows/installer-hooks.nsh` (`NSIS_HOOK_POSTINSTALL`) a coloca de volta quando a cópia existe. O `atlas update` deixa o `Atlas Uninstall.exe` no lugar enquanto troca os outros arquivos, então o hook o encontra. A janela de desinstalação pergunta **Keep your Atlas data?** e diz o que a exclusão remove (`%USERPROFILE%\.atlas`: configurações, lista de projetos, worktrees que o Atlas criou com quaisquer mudanças não commitadas, o comando `atlas`; nunca as pastas dos projetos). Manter é o padrão (o `--silent` nunca remove os dados, a menos que haja `--remove-data`); excluir pergunta uma segunda vez, e fechar a janela ou apertar Esc na pergunta cancela antes de qualquer coisa ser tocada. Depois ele roda `uninstall.exe /S _?=<dir>`, roda `atlas uninstall --yes` só quando remove os dados e apaga o que o NSIS deixa. A cópia que está rodando (Configurações › Aplicativos a inicia a partir da pasta de instalação) não consegue se apagar, então um `cmd` desanexado apaga a cópia e a pasta vazia alguns segundos depois de a janela fechar. O instalador NSIS simples, quando atualiza sobre uma instalação feita pelo `Atlas-Setup.exe`, roda o desinstalador registrado como `"Atlas Uninstall.exe" --uninstall /UPDATE /P _?=<dir>`: `/UPDATE`, `/P` e `_?=` são aceitos e fazem dele uma desinstalação silenciosa que mantém os dados.

Reparo na atualização (`crates/atlas-setup/src/repair.rs`): as versões 1.5.0-alpha.21 a 1.5.0-alpha.24 deixaram para trás um estado ruim conhecido, então, depois que o passo do NSIS tem êxito, o setup lê a máquina, planeja e executa as correções, uma linha de log para cada uma em `%TEMP%\atlas-setup.log`, e mostra "Updating your setup…" quando há algo a fazer. Um reparo que falha é registrado no log e nunca faz a instalação falhar. (1) Um `%USERPROFILE%\.atlas\bin\atlas.exe` mais antigo que o empacotado (pela precedência do SemVer, então `alpha.9 < alpha.24`) ou ilegível é substituído pelo `atlas.exe install --upgrade` empacotado, que renomeia uma cópia em execução em vez de matar os processos que a seguram; um symlink (`atlas link`) ou uma CLI mais nova é deixado como está. O hook do NSIS roda o mesmo `atlas install --upgrade` sempre que essa CLI existe, então `atlas update` e o `/S` do winget também a reparam, sem uma segunda implementação. (2) Um host de terminais que não é a versão empacotada passa pelo `atlas hostd --reconcile` empacotado: não existe upgrade ao vivo no Windows, então um host vazio é reiniciado e um com terminais é mantido (o app pergunta, como antes). (3) O `%APPDATA%\br.com.codeatlas.atlas\.window-state.json` perde `decorated` e toda entrada cujo retângulo está fora de todos os monitores. (4) O `~/.atlas/desktop/settings.json` recebe `onboarding.done: true` quando o `projects.json` lista projetos. (5) `%TEMP%\atlas-uninstalled-*.exe`, `atlas-setup-*`, `.atlas.*.__relocated__.exe` e `~nsu*.tmp`, e (6) um `~/.atlas/.update.lock` cujo pid está morto, são apagados. Nada além disso é tocado: outras configurações, projetos, worktrees e sessões nunca são reescritos nem apagados (`the_plan_never_touches_user_data`).

Para experimentar um localmente (Windows, depois de `cargo tauri build --ci --bundles nsis --config '{"bundle":{"resources":{"../../target/release/atlas.exe":"atlas.exe"}}}'` em `crates/atlas-desktop`): `ATLAS_SETUP_PAYLOAD=$PWD/target/release/bundle/nsis/Atlas_*_x64-setup.exe cargo build --release -p atlas-setup` e depois rode `target\release\Atlas-Setup.exe`. `ATLAS_SETUP_DUMP=<dir> cargo test -p atlas-setup dump_frames` escreve cada tela em RGBA bruto para a revisão de design.

### Assinatura de código no Windows (SignPath Foundation)

Os arquivos do Windows são assinados pela [SignPath Foundation](https://signpath.org) (gratuita para código aberto; o texto da política está no [README](../../README.md#code-signing-policy) e em [Instalação](installation.md#política-de-assinatura-de-código)). A ordem das assinaturas é imposta pelo que embute o quê: o instalador NSIS empacota o `atlas.exe` e o `atlas-desktop.exe`, e o `Atlas-Setup.exe` embute o instalador NSIS em tempo de build. Por isso o `build-windows` é uma cadeia de jobs, três deles pedidos de assinatura:

```text
windows-binaries ─▶ sign-binaries ─▶ windows-installer ─▶ sign-installer ─▶ windows-setup ─▶ sign-setup ─▶ build-windows
 atlas.exe,           (SignPath          NSIS from the        (SignPath         Atlas-Setup      (SignPath      archive, renamed copy,
 atlas-desktop.exe     `binaries`)        signed binaries      `installer`)      around it        `setup`)       .sha256 of the final bytes
```

- Cada job `sign-*` envia só um zip para a SignPath (`signpath/github-action-submit-signing-request`, fixado por SHA) e espera o resultado. Um pedido espera uma **aprovação** manual na SignPath, então cada um é um job próprio: a espera (até 6 horas) não conta contra o limite de 90 minutos de um job de build, e uma versão com a assinatura ligada precisa de três aprovações.
- O `build-windows` mantém o seu nome: ele empacota os bytes finais (`atlas-<version>-x86_64-pc-windows-msvc.tar.gz`, `…-setup.exe`, `Atlas-Setup.exe`) e escreve os arquivos `.sha256` **depois** da última assinatura, e é por isso que os checksums vêm deste job e não dos jobs de build. O `publish` e o nome de artefato `atlas-x86_64-pc-windows-msvc` não mudam.
- **A chave:** os jobs de assinatura só rodam quando a variável de repositório `SIGNPATH_ENABLED` é `true` (Settings › Secrets and variables › Actions › Variables). Sem definir, ou com qualquer outro valor: os três jobs são pulados e a mesma cadeia entrega os arquivos sem assinatura, com os mesmos nomes, então nada em uma versão depende da SignPath até você ligá-la. O caminho sem assinatura também passa pelos jobs separados (`cargo tauri build --no-bundle`, depois `cargo tauri bundle`), então a primeira versão depois desta mudança é a primeira a exercitar a separação; leia os jobs do Windows dela antes de confiar.
- Secrets: `SIGNPATH_API_TOKEN`, `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG`, `SIGNPATH_SIGNING_POLICY_SLUG`. As configurações de artefato para colar na SignPath, e a configuração única em signpath.org, estão em `packaging/windows/signpath/README.md` (`binaries.xml`, `installer.xml`, `setup.xml`).
- Se um pedido de assinatura é rejeitado ou expira, os jobs do Windows depois dele são pulados e o `publish` não roda; corrija a causa na SignPath e rode de novo os jobs que falharam, ou rode `bun run release:publish vX.Y.Z` para a mesma tag. Desligar `SIGNPATH_ENABLED` e rodar de novo entrega arquivos sem assinatura.

### Assinatura e notarização no macOS

Os arquivos do macOS levam uma assinatura Developer ID da equipe CODEATLAS LTDA (Team ID `58UYCA3ZYA`) e são notarizados pela Apple, **quando os seis secrets abaixo existem**. Sem eles (um fork, ou antes de serem configurados), o build é assinado ad-hoc exatamente como antes, e o passo `macOS signing setup` imprime um aviso dizendo isso. Por que importa: com o firewall de aplicativos do macOS ligado, uma identidade ad-hoc é o hash do binário, então toda atualização perguntava de novo "permitir conexões de entrada" para o `atlas hostd` (ele abre o gateway do celular em todas as interfaces enquanto um celular está pareado); uma identidade Developer ID é estável, então o firewall e o TCC (Screen Recording, Input Monitoring, Microphone) lembram dela entre as atualizações, e um `.dmg` baixado abre sem o aviso "cannot verify" do Gatekeeper.

| Arquivo | Assinado | Notarizado | Entitlements |
|---|---|---|---|
| `atlas` (dentro de `atlas-<version>-aarch64-apple-darwin.tar.gz`) | pelo job de release: `codesign --options runtime --timestamp` | sim, enviado como zip. Um binário puro não pode receber o staple, então o Gatekeeper consulta o ticket online na primeira vez que uma cópia baixada roda (`atlas install` e `atlas update` nunca a põem em quarentena) | `crates/atlas-desktop/Entitlements-cli.plist` (vazio de propósito) |
| `Atlas.app` (dentro de `….app.tar.gz` e do dmg) | pelo Tauri (`cargo tauri build`), hardened runtime, timestamp seguro | sim, pelo Tauri, depois com staple | `crates/atlas-desktop/Entitlements.plist` (`bundle.macOS.entitlements`) |
| `Atlas-<version>-aarch64-apple-darwin.dmg` | pelo Tauri | sim, pelo job de release, depois `xcrun stapler staple` **antes** de o `Package` escrever o `.sha256` (o staple muda os bytes) | nenhum (uma imagem de disco) |

**Como o job `build` faz isso.**

1. `macOS signing setup` (`id: sign`): decide, importa ou falha. Todos os seis secrets presentes e a execução é de uma versão ou de um somente build da `main` → ligado. Nenhum presente → ad-hoc mais um aviso. **Alguns, mas não todos → o job falha** nomeando os que faltam, para que um repositório configurado pela metade não entregue ad-hoc em silêncio. Ele decodifica o `.p12`, criptografa de novo com uma senha descartável (o `security import -P` não tem forma por stdin, então só essa senha descartável aparece em uma linha de comando, nunca a real), importa para um keychain temporário (criado, aberto com `security unlock-keychain` e mantido aberto por 6 horas, posto primeiro na lista de busca, com a lista de partições definida para o `codesign`), confere que o keychain tem uma identidade de assinatura de código *válida* com exatamente o nome `MACOS_SIGNING_IDENTITY` e escreve o `.p8` do App Store Connect em `$RUNNER_TEMP/notary.p8` (modo 600).
2. `Build desktop app`: só quando ligado, ele exporta `APPLE_SIGNING_IDENTITY`, `APPLE_API_KEY` (id da chave), `APPLE_API_ISSUER` e `APPLE_API_KEY_PATH` (os nomes do Tauri 2; o `APPLE_CERTIFICATE` não é usado, o keychain acima já tem a identidade). Eles só são exportados nesse caso porque o Tauri lê um `APPLE_SIGNING_IDENTITY` vazio como uma identidade chamada `""`. Um wrapper de `codesign` no começo do `PATH` acrescenta `--timestamp`: o Tauri 2.11 chama o `codesign` sem ele, o `codesign` não acrescenta nenhum por conta própria (conferido: nenhum `Timestamp=` no `codesign -dvv`), e a notarização recusa uma assinatura sem timestamp seguro.
3. `Developer ID signature, notarization and checks`: assina a CLI, notariza o zip dela e o dmg (`xcrun notarytool submit --wait --output-format json`; qualquer resultado diferente de `Accepted` imprime o log da Apple e falha), aplica o staple no dmg e então confere os arquivos reais: `codesign --verify --deep --strict` no app e na CLI, `xcrun stapler validate` no app e no dmg, `spctl -a -vvv -t exec` no app e `spctl -a -vvv -t open --context context:primary-signature` no dmg (ambos precisam dizer `source=Notarized Developer ID`), `TeamIdentifier`, `Authority=Developer ID Application` e um `Timestamp=` no `codesign -dvv` dos três, a flag `runtime` no app e na CLI, o entitlement de microfone no app e nenhum `get-task-allow`. O `spctl` em uma CLI pura não faz sentido (o Gatekeeper avalia apps e imagens), então a CLI é coberta pelas outras verificações.
4. `Package` assina ad-hoc só quando o Developer ID está desligado e depois empacota como antes.
5. `Remove the signing keychain and keys` (`if: always()`): apaga o keychain (o que também o tira da lista de busca) e todo `.p12`, `.pem` e `.p8`. O runner é uma VM nova de qualquer forma; isto serve para as falhas do próprio job.

**Secrets** (secrets do repositório, Settings › Secrets and variables › Actions; definidos pelo `scripts/macos-signing-secrets.sh`, nunca à mão): `MACOS_CERTIFICATE_P12_BASE64` (certificado e chave privada), `MACOS_CERTIFICATE_PASSWORD` (aleatória, gerada pelo script), `MACOS_SIGNING_IDENTITY` (`Developer ID Application: CODEATLAS LTDA (58UYCA3ZYA)`, lida do certificado), `APPLE_API_KEY_ID`, `APPLE_API_ISSUER`, `APPLE_API_KEY_P8` (a chave de API do App Store Connect que a versão mobile já usa, copiada de `~/.config/stores/apple.env`: `ASC_KEY_ID`, `ASC_ISSUER_ID`, o arquivo em `ASC_KEY_PATH`). Eles são do nível do repositório, não de um ambiente, então qualquer execução de workflow no repositório pode lê-los; é por isso que um somente build de uma branch tem o certificado recusado e só a `main` (protegida pelo ruleset) e as tags de release o recebem. Os valores chegam aos passos só por `env`, nunca por uma linha de comando nem por um log.

**Configuração única (um mantenedor, 5 minutos depois que o certificado existe).**

1. A chave privada e o CSR já existem em `~/.config/stores/devid/` (`devid.key`, `devid.csr`); a chave nunca sai desse Mac. O Account Holder cria um certificado **Developer ID Application** a partir do `devid.csr` em developer.apple.com › Certificates (um Admin não consegue) e envia o `.cer` baixado.
2. `scripts/macos-signing-secrets.sh ~/Downloads/<file>.cer --dry-run` confere cada arquivo e se o certificado pertence ao `devid.key`, monta o `.p12`, lê de volta e imprime só os seis nomes de secret. Sem `--dry-run`, ele os define com `gh secret set` (valor no stdin, `--repo` assume `codeatlasdev/atlas`) e imprime cada nome que definiu. Ele precisa do `gh` autenticado com permissão para definir secrets.
3. `gh workflow run release.yml --ref main -f build_only=true` e depois leia o job `build`: as verificações acima rodam de verdade, e o artefato `atlas-aarch64-apple-darwin` guarda os arquivos assinados. Essa execução é a primeira vez que a notarização é exercitada; rode antes de criar uma versão.

**Rotação.** Um certificado Developer ID Application dura cinco anos; os arquivos assinados antes de ele expirar continuam válidos (eles levam um timestamp seguro). Para trocá-lo (expiração, revogação, um `.p12` vazado): um CSR novo se a chave é suspeita (senão o `devid.key` continua), o Account Holder emite o novo certificado e você roda o script de novo com o novo `.cer`: ele sobrescreve `MACOS_CERTIFICATE_P12_BASE64`, `MACOS_CERTIFICATE_PASSWORD` e `MACOS_SIGNING_IDENTITY` (a senha muda a cada execução). Para trocar a chave de API, atualize o `apple.env` e rode o script de novo; a versão mobile lê os seus próprios secrets de ambiente e não é afetada. Para voltar ao ad-hoc, apague os seis secrets (apagar só alguns faz o build falhar de propósito).

**Entitlements, um por um.** O hardened runtime (exigido para a notarização) desliga toda capacidade que não está listada, então cada entrada foi conferida contra o código, e os arquivos carregam o raciocínio em um comentário. Presente: `com.apple.security.device.audio-input` no app (Configurações › Captura › Gravar o microfone: acesso do `AVCaptureDevice` e o `captureMicrophone` do ScreenCaptureKit; sob o hardened runtime, sem ele o sistema nunca pergunta e a gravação sai muda). Deliberadamente ausentes: `allow-jit` e `allow-unsigned-executable-memory` (o JIT do WebKit roda no processo WebContent, que é da Apple; o Atlas não tem JIT, wasm nem mecanismo de script próprio), `disable-library-validation` (nenhum `dlopen`, nenhuma dylib empacotada: o `otool -L` da CLI lista só bibliotecas do sistema), `allow-dyld-environment-variables` (nada depende de `DYLD_*` chegar ao próprio Atlas; shells e agentes são processos separados, que o hardened runtime não restringe), Apple Events (nenhum no código), `get-task-allow` (recusado pela notarização). Screen Recording, Input Monitoring e Accessibility são permissões do TCC que o usuário concede, e escutar na rede não precisa de entitlement fora do App Sandbox, que o Atlas não usa. **Se um build assinado mostrar um crash do WebKit ou do Metal citando JIT ou memória executável, acrescente `allow-jit` ao `Entitlements.plist` e diga o motivo lá.**

**O que foi provado localmente e o que só a Apple pode provar.** Em um Mac, com uma identidade de assinatura de código autoassinada descartável em um keychain descartável (apagado depois), um `atlas` de debug assinado com `--options runtime` e `Entitlements-cli.plist` rodou `atlas --version` e `atlas hostd --dir <tmp>` (daemon de pé, `--status` respondido pelo socket, `--stop`) sem processo sobrando, então o hardened runtime não quebra a CLI nem o hostd; e a importação no keychain usada pelo workflow (criptografar de novo, `security import`, `set-key-partition-list`) funciona com o `openssl` desta máquina (LibreSSL, o do runner também). Não dá para provar sem o certificado da Apple: que a notarização é aceita, o `spctl` dizendo `Notarized Developer ID`, que o dmg e o app recebem o staple, que o Atlas.app com hardened runtime abre a sua janela e o WebKit funciona (a lista de entitlements acima foi deduzida do código, não de uma execução do app assinado) e que a chave do App Store Connect pode notarizar. A primeira execução de `build_only` da `main` com os secrets é essa prova.

### Versões de pré-lançamento no bundle do macOS

O Tauri escreve a versão do crate sem alterações em `CFBundleShortVersionString` e `CFBundleVersion`, e no nome do arquivo `.dmg` (`Atlas_<version>_aarch64.dmg`), então uma versão pré-lançamento como `1.4.0-alpha.1` aparece ali como está. O código-fonte do bundler (`tauri-bundler`, `macos/app.rs` e `dmg/mod.rs`) não faz nenhum parsing de SemVer para o plist. Em uma cópia do `Atlas.app` instalado com esse valor, `plutil -lint`, `codesign --verify --deep --strict` ad-hoc e `hdiutil create` têm todos êxito, e o único leitor neste repositório (`app_version` no `atlas-cli`, por `plutil -extract`) o trata como uma string. O glob `Atlas_*_aarch64.dmg` do job de release continua casando. Um `cargo tauri build` completo com uma versão pré-lançamento **não** foi rodado do início ao fim aqui: a primeira alpha é esse teste. Se ele se comportar mal, defina `bundle.macOS.bundleVersion` no `tauri.conf.json` como o `X.Y.Z` do núcleo e mantenha a versão completa nos nomes dos arquivos e no `atlas --version`.

## Se algo falhar

- **O plano ou a pré-verificação falha** (branch errada, soak, guarda, a tag existe, seção do changelog): nada mudou ainda; corrija e rode de novo.
- **As verificações falham**: nada mudou ainda.
- **A execução do Release deu certo, mas a versão ainda é um rascunho** (a assinatura foi pulada ou falhou, por exemplo: sem chave, uma chave que não está no `release-keys.json`, `ssh-keygen` ausente): nada é público e nenhum cliente a vê. Rode `scripts/release-sign.sh vX.Y.Z --key <sua chave>`; é seguro rodar de novo, e uma versão que já tem uma assinatura válida só é publicada. O `bun run release` não aumenta a versão de novo (a tag existe), então faça o deploy do site depois com `bun run site:deploy`.
- **O push deu certo, mas a execução do Release falhou**: corrija a causa (`gh run view <id> --log-failed`) e depois rode `bun run release:publish vX.Y.Z` de novo. Ele dispara o `release.yml` para a tag existente e o acompanha. Rodar de novo o mesmo `bun run release <request>` nesse commit faz o mesmo (a verificação de retomada vê a tag enviada sem um Release). Os jobs que falharam também podem ser rodados de novo pela página do Actions.
- **O push falhou no meio**: `--atomic` quer dizer que nem a branch nem a tag se moveram em `origin`. Ficam um commit e uma tag locais: apague a tag (`git tag -d vX.Y.Z`), desfaça o commit da versão e rode de novo.
- **A primeira beta falhou depois de a branch ser criada**: o `release/X.Y` existe localmente; volte para a `main`, rode `git branch -D release/X.Y` se nada foi enviado e rode de novo.
- **A tag existe**: escolha outra versão, ou apague a tag só se nenhum Release foi criado a partir dela.

## Ensaiar a versão

```bash
bun run release:dry alpha     # mostra a versão que criaria e cada passo; não muda nada
bun run release:test          # as próprias regras
```

### Somente build: testar a main do início ao fim antes de uma versão

```bash
gh workflow run release.yml --ref main -f build_only=true     # ou qualquer branch: --ref <branch>
gh run watch                                                  # depois baixe os artefatos na página da execução
```

O `build_only` roda os mesmos jobs no commit disparado (não em uma tag) e para antes do `publish`: ele compila os artefatos de macOS, Linux (x86_64, aarch64), Arch e Windows, roda `verify-linux`, `verify-windows` e `verify-macos` contra eles e envia tudo como artefatos do workflow (mantidos por 90 dias, o padrão do GitHub). Ele nunca cria uma tag nem uma release e nunca lê os secrets de publicação do repositório: o `publish` é pulado pelo seu `if`, o `gate` não espera o CI (não há tag para procurar) e os jobs da SignPath também são pulados, então os arquivos do Windows ficam sem assinatura. O macOS só é assinado com Developer ID e notarizado quando a execução é da `main` e os secrets existem (uma branch nunca recebe o certificado); senão ele fica ad-hoc e o job diz isso. Os arquivos recebem o nome de uma versão descartável, `0.0.0-alpha.<run number>`; a versão dentro dos binários é a do `Cargo.toml`, e o `verify-linux` pula a sua verificação "`atlas --version` é a tag". O `bun run release` não muda: ele envia uma tag, e a execução da tag é a de verdade. Disparar sem `build_only` exige `-f tag=vX.Y.Z`, como antes.

Rode antes de uma versão sempre que o Linux ou o empacotamento mudou, e leia os artefatos `verify-linux-*`: cada um tem `screenshot.png`, o stderr do app e o seu `app.log`; o `verify-windows` e o `verify-macos` enviam as suas capturas de tela e o `app.log` do app da mesma forma.

## AUR (`codeatlas-atlas-bin`)

Depois que o `publish` tem êxito, o job `publish-aur` envia o `PKGBUILD` renderizado da versão (o que o `build-arch` anexou; a sua fonte é o `.deb` da própria versão, então ele só existe depois que a versão existe) e um `.SRCINFO` gerado com `makepkg --printsrcinfo` para `ssh://aur@aur.archlinux.org/codeatlas-atlas-bin.git`, branch `master`. Rodar de novo uma versão sem mudanças não envia nada. O `build-arch` já falha quando o PKGBUILD não consegue se descrever (`makepkg --printsrcinfo`, com `ripgrep` entre as dependências). Alphas e betas também vão para o AUR: as versões do pacman tiram o `-` de um pré-lançamento (`1.5.0alpha.26`, veja `packaging/arch/render.sh`), e Alpha é o canal padrão.

O `publish` cria a versão como rascunho ([Assinatura de versões](#assinatura-de-versões)), então, nos minutos até o `scripts/release-sign.sh` publicá-la, o `.deb` para o qual o novo PKGBUILD aponta ainda não pode ser baixado: um helper do AUR que compila nessa janela recebe um 404 e tenta de novo depois. O `bun run release` assina logo depois da execução, então a janela é o passo de assinatura.

A conta do AUR é do mantenedor, então o job não faz nada até esta configuração única ser feita (sem ela, o job imprime um aviso e termina com sucesso; ele nunca bloqueia uma versão):

1. Registre uma conta em <https://aur.archlinux.org> (a do mantenedor: o AUR lista a conta como mantenedora do pacote).
2. Crie uma chave só para o CI, `ssh-keygen -t ed25519 -f aur-ci -C atlas-release-ci`, e cole o `aur-ci.pub` em **My Account › SSH Public Key** (várias chaves são permitidas, então ela pode ser revogada sozinha).
3. Acrescente a chave privada como o secret do repositório `AUR_SSH_PRIVATE_KEY` (Settings › Secrets and variables › Actions).
4. Nada mais: a primeira versão depois disso clona o repositório `codeatlas-atlas-bin` (que ainda não existe, então está vazio), acrescenta `PKGBUILD` e `.SRCINFO` e faz push, o que cria o pacote na conta da chave. Se já existe um pacote com esse nome e ele é de outra pessoa, o push é recusado e o job falha; fale primeiro com o AUR.

O workflow fixa a chave de host Ed25519 do aur.archlinux.org (`SHA256:RFzBCUItH9LZS0cKB5UE6ceAYhBD5C8GeOBip8Z11+4`, uma das três impressões digitais que a página inicial do AUR publica; o `scripts/linux-launch.test.ts` confere a linha fixada contra ela) e nunca usa `ssh-keyscan` nem `StrictHostKeyChecking=no`. Se o AUR um dia trocar as suas chaves, o job falha na verificação do host; atualize a linha no `release.yml` a partir de <https://aur.archlinux.org/>. As regras do AUR pedem que os mantenedores revisem atualizações automáticas (uma dependência nova, uma mudança de licença): leia o commit que ele envia depois de uma versão que mudou o PKGBUILD.

## Verificar uma versão

Nenhum destes usa a API do GitHub, que tem limite de requisições e que os clientes do Atlas também não chamam:

```bash
# O que o GitHub mostra como "Latest": o build mais novo do canal padrão (uma alpha antes do GA), NÃO o canal Stable
curl -sI https://github.com/codeatlasdev/atlas/releases/latest | grep -i '^location:'

# Stable, beta e alpha: o feed que os clientes leem, do mais novo ao mais antigo (Stable = a tag mais nova sem rótulo de prerelease;
# entradas mais antigas: releases.atom?after=<tag mais antiga mostrada>)
curl -fsSL https://github.com/codeatlasdev/atlas/releases.atom | grep -o '<title>[^<]*</title>' | head

# Os assets e o que cada canal oferece agora, como os clientes calculam
curl -fsSIL https://github.com/codeatlasdev/atlas/releases/download/vX.Y.Z/manifest.json | head -1
atlas update channel --json
atlas update --check
```

Confira os checksums de um download:

```bash
curl -fsSLO https://github.com/codeatlasdev/atlas/releases/download/vX.Y.Z/SHA256SUMS
shasum -a 256 -c SHA256SUMS --ignore-missing
```

Confira a assinatura (o que os clientes fazem), a atestação do build e a atestação da própria versão no GitHub:

```bash
curl -fsSLO https://github.com/codeatlasdev/atlas/releases/download/vX.Y.Z/SHA256SUMS.sig
bun scripts/release-keys.ts allowed-signers > allowed_signers          # de um checkout na tag
ssh-keygen -Y verify -f allowed_signers -I <key name> -n atlas-release -s SHA256SUMS.sig < SHA256SUMS
gh attestation verify atlas-X.Y.Z-aarch64-apple-darwin.tar.gz --repo codeatlasdev/atlas
gh release verify vX.Y.Z --repo codeatlasdev/atlas                       # só versões imutáveis
gh release verify-asset vX.Y.Z atlas-X.Y.Z-aarch64-apple-darwin.tar.gz --repo codeatlasdev/atlas
```

Os usuários recebem a nova versão com `atlas update`, que baixa `atlas-<version>-aarch64-apple-darwin.tar.gz` da versão e depois o `Atlas-<version>-…app.tar.gz` correspondente quando o Atlas Desktop está instalado. Ou clicam em **Atualizar CLI e app** no app. Confira o arquivo do app com `atlas desktop --install --latest --no-open && atlas desktop --status`.

## Símbolos

Toda versão publica as informações de depuração do que entrega, para que um relatório de crash de um usuário possa ser convertido em funções e linhas (#318). Os arquivos `*-symbols.tar.gz`, um por plataforma, ao lado do tarball da CLI:

| Plataforma | O que o arquivo contém | Casado por |
|---|---|---|
| Linux (x86_64, aarch64) | um `<build-id>.debug` por binário (`atlas`, e `atlas-desktop` no x86_64), gerado por `scripts/symbols.sh extract` | o build id do GNU (`atlas version` o imprime como `build:`; todo relatório de crash tem `build_id`) |
| macOS | os bundles `.dSYM` (`split-debuginfo=packed`) | o UUID do Mach-O |
| Windows | os arquivos `.pdb` | a assinatura do PDB (GUID + age) do exe, que é o `build_id`; `load_bias` em um relatório é a base da imagem |

Um relatório de crash (`crash-*.toml` ou `crash-native-*.toml`) nomeia `version`, `channel`, `commit`, `build_id`, `load_bias` e `frames` (endereços de retorno brutos). No Linux:

```bash
tar xzf atlas-X.Y.Z-x86_64-unknown-linux-gnu-symbols.tar.gz -C symbols/
scripts/symbols.sh resolve crash-native-<ms>-<pid>.toml symbols/   # uma linha por frame: endereço função arquivo:linha
```

Os binários entregues, o AppImage, o `.deb`, o `.rpm`, o `.dmg` e o instalador NSIS **não** carregam informações de depuração: o workflow as retira logo depois do build (`scripts/symbols.sh split` no Linux, `strip -S` depois do `.dSYM` no macOS, um PDB que já é separado no Windows) e as confere (`symbols.sh verify-stripped`) antes de qualquer pacote ser feito. Os nomes dos símbolos ficam no binário, então um backtrace simples ainda tem nomes de função; arquivo e linha vêm do arquivo de símbolos.

O `resolve` procura o frame como `frame - load_bias - 1` em `symbols/<build_id>.debug`, então um relatório de um build cujo arquivo não foi publicado diz isso em vez de adivinhar. Os relatórios ficam na máquina do usuário até que ele anexe um; nada é enviado.

### Fingerprint de um crash

Todo relatório de crash nomeia o bug com um `fingerprint` de 16 caracteres hexadecimais, para achar dois relatórios do mesmo bug buscando uma issue por ele (#320). Um relatório de panic tem a linha no arquivo, feita da mensagem e do local. Um relatório nativo (`crash-native-*.toml`) é escrito em contexto de sinal, onde nada pode ser calculado, então o fingerprint é calculado quando ele é lido, a partir dos frames:

- **Com os símbolos do build**: `atlas crash-fingerprint <crash.toml> --resolved <saída do symbols.sh resolve>` usa a primeira função resolvida dentro do Atlas e o arquivo dela (sem linha, sem diretório, sem hash no nome), então o mesmo crash em duas versões tem um fingerprint só.
- **Sem eles** (o que o app tem na máquina do usuário, que não leva debug info; também `atlas crash-fingerprint <crash.toml>`): `build_id` e o offset, o primeiro frame dentro do executável depois dos frames do próprio handler de crash, menos `load_bias`. O ASLR não o muda, então o mesmo crash do mesmo build em duas máquinas tem um fingerprint só; duas versões têm dois, porque um offset não significa nada fora do seu build.

Um relatório sem nenhum frame dentro do executável não tem fingerprint e nunca coincide com outro. Os mesmos 16 caracteres estão no corpo da issue que o "Reportar issue" abre (a linha `Fingerprint:`), e o "Reportar issue" do app procura issues abertas por eles antes de abrir uma nova.

## Retirar uma versão com defeito

Uma versão que não deve ser instalada é retirada às claras e nunca reescrita. A tag e os assets ficam (outros clones e a lógica de retomada dependem deles); o que muda é que todo cliente para de oferecê-la, **sem uma nova versão**:

1. Acrescente-a a [`data/yanked.json`](../../data/yanked.json) na `main` (um PR normal; um mantenedor pode fazer o merge como emergência):

   ```json
   { "yanked": [ { "version": "1.5.0-alpha.3", "reason": "Corrupts the workspace file on first launch.", "replacement": "1.5.0-alpha.4" } ] }
   ```

   `version` não tem `v` na frente; `reason` é uma linha que o usuário lê; `replacement` é opcional e não pode estar retirada também. `cargo test -p atlas-core yanked` confere o arquivo commitado.
2. A partir daí, `atlas update`, as Configurações → Geral → Atualizações do app e os cartões de canal, a checagem de celular do hostd e o `atlas desktop` pulam essa versão: um canal oferece a versão publicada mais nova que não foi retirada (a Stable volta para a Stable anterior, mesmo quando o `releases/latest` ainda nomeia a retirada). Os clientes leem o arquivo de `raw.githubusercontent.com/codeatlasdev/atlas/main/data/yanked.json` a cada verificação de atualização, então ele vale dentro do intervalo de verificação (6 h para o banner em cache) depois do merge. Um cliente que não consegue lê-lo (offline, GitHub fora do ar, um arquivo que não faz parse) registra isso no log e oferece todas as versões: a lista só remove ofertas, então não conhecê-la nunca bloqueia uma atualização.
3. Um usuário que **roda** a versão retirada é avisado do motivo (`atlas update`, `atlas update --check --json` como `yanked`, Configurações → Geral → Atualizações) e recebe a oferta da versão mais nova do seu canal que não foi retirada, mesmo quando ela é mais antiga do que a que ele roda (um rollback confirmado, como o `--downgrade`).
4. Marque também a seção no `CHANGELOG.md`: `## [1.4.0-beta.2] — 2026-10-20 [YANKED]` com uma linha dizendo o motivo e qual versão usar no lugar. Não a apague.
5. Entregue a correção como a próxima versão (`beta.3`, ou `patch` para uma stable); os clientes sobem para ela pela precedência do SemVer.
6. Só o mantenedor também pode rebaixar ou remover a release do GitHub: `gh release edit vX.Y.Z --prerelease` (uma release deixa de ser "Latest" quando algo mais novo é marcado como latest; rebaixar uma alpha do canal padrão devolve o "Latest" ao build anterior só quando você também roda `gh release edit <tag> --latest`), ou `gh release delete vX.Y.Z` para um build perigoso de manter disponível para download. Nunca apague nem mova a **tag**, e o número de uma versão apagada nunca é reutilizado (a v1.2.0 foi apagada e continua pulada).

O manifesto (`manifest.json`) não tem campos de kill switch: ele guarda `version`, `channel`, `prerelease`, `commit`, `date`, `assets`, `changelog` e, em uma promoção, `promoted_from`. O `scripts/test_generate_manifest.py` fixa esse conjunto, então uma chave que nada lê não pode voltar. A lista de versões retiradas ainda não é assinada; ela é tão confiável quanto a `main` (a assinatura espera o #279).

## Procedência e assinatura: o que é verdade hoje

| Propriedade | Situação |
|---|---|
| Os binários são assinados ad-hoc (`codesign --sign -`) | só quando os secrets do Developer ID estão ausentes (forks, antes de serem configurados); o Gatekeeper então avisa em um `.dmg` baixado |
| Assinatura Developer ID e notarização (CLI, `Atlas.app`, dmg) | **decidido e implementado**; ligado quando os seis secrets existem ([Assinatura e notarização no macOS](#assinatura-e-notarização-no-macos)). Até uma versão rodar com eles, trate as versões como ad-hoc |
| Assinatura Authenticode do Windows (SignPath Foundation) | **ainda não**: o workflow está pronto e desligado; ele liga com a variável de repositório `SIGNPATH_ENABLED` quando a Foundation aceitar o projeto ([Assinatura de código no Windows](#assinatura-de-código-no-windows-signpath-foundation)) |
| Checksums | `.sha256` por asset (mantido para clientes anteriores à primeira versão assinada), `SHA256SUMS` sobre tudo, `manifest.json` |
| `SHA256SUMS` assinado | **construído, ainda não obrigatório.** Assinatura OpenSSH (`SHA256SUMS.sig`, namespace `atlas-release`) feita com a chave ed25519 offline de um mantenedor; a CLI a verifica e toma os hashes dos assets do arquivo assinado. Ela passa a ser obrigatória a partir de `signed_since`, que continua sem valor (e a lista de chaves vazia) até a primeira versão assinada ([Transição](#transição)) |
| SBOM | JSON CycloneDX da árvore de dependências da CLI, quando o `cargo-cyclonedx` está disponível |
| Builds reproduzíveis | parcial: `--locked`, `SOURCE_DATE_EPOCH`; o `--remap-path-prefix` não está definido porque não foi verificado contra o build do Tauri |
| Compilado em runners hospedados pelo GitHub a partir da tag | sim (`release.yml`, disparado pelo push da tag `v*`) |
| Atestações de artefato do GitHub | **sim** (job `attest`, `actions/attest` sobre o `SHA256SUMS`): uma atestação Sigstore da execução do `release.yml` para cada asset. Ela é feita depois dos jobs de build, a partir dos arquivos que o `publish` reuniu, então diz "esta execução do workflow publicou estes bytes", nada além disso |
| Versões imutáveis | **desligadas** (configuração do repositório); ligue na primeira versão assinada ([abaixo](#ativar-versões-imutáveis)) |

**O que um build hospedado pode afirmar:** os artefatos foram compilados pelo GitHub Actions a partir do commit da tag, com dependências travadas, e o manifesto registra esse commit. A atestação é a declaração assinada do GitHub sobre essa execução; ela não afirma nenhum nível SLSA acima do nível 1 de build para o build em si. Não descreva os builds como de um nível mais alto que esse.

## Assinatura de versões

A ameaça: uma versão trocada (um token de upload roubado, uma conta de mantenedor ou um workflow comprometido) passava em todas as verificações, porque o checksum vinha da mesma versão que o arquivo. Agora a âncora de confiança é a **chave offline de um mantenedor**, não o GitHub: o CI compila um rascunho, um mantenedor assina o `SHA256SUMS` na própria máquina e só então a versão fica pública. Quem consegue publicar assets mas não tem uma chave não produz uma assinatura que os clientes aceitem; o pior caso é uma versão que fica como rascunho. Uma mudança maliciosa no workflow ainda compila uma versão envenenada que um mantenedor pode assinar às cegas, então o `release-sign.sh` mostra os hashes que assina; compare-os com a atestação da execução (`gh attestation verify`).

| Ameaça (S = barrada, N = não, P = em parte) | `.sha256` ao lado do asset (antes) | Chave offline do mantenedor + atestação (agora) |
|---|---|---|
| Um **token de upload** de assets de versão roubado troca assets depois do CI | N | S (não consegue assinar) |
| Uma **conta do GitHub** comprometida (mantenedor ou admin) | N | S (pode publicar assets, não consegue produzir uma assinatura válida; o pior caso é reter a versão, um DoS) |
| Um **workflow de CI** comprometido (mudança maliciosa que entrou por merge, action envenenada, runner) | N | P (o CI não assina; um mantenedor que assina um build envenenado o assina, então compare antes os hashes com a atestação) |
| Adulteração de **CDN / espelho** | N | S |
| Suprimir versões novas, ou servir uma antiga assinada (rollback) | N | N (o feed de atualização não é assinado; fora do escopo) |

Custódia: dois mantenedores, duas chaves, qualquer uma assina; rotação, expiração e o procedimento de chave perdida estão abaixo. O Sigstore keyless como *única* âncora de confiança foi rejeitado (ele não defende contra uma conta ou um workflow comprometido, custa de 1,7 a 5 MB e mais de 34 crates na CLI, e os instaladores de shell não conseguem verificá-lo); a atestação do GitHub fica como a segunda camada, aditiva.

- **Formato:** `sshsig` do OpenSSH sobre o `SHA256SUMS` inteiro, namespace `atlas-release`, tipo de chave `ssh-ed25519`, hash `sha512` (o padrão do `ssh-keygen -Y sign`). Só ed25519 comum: chaves FIDO `-sk` são rejeitadas pelo verificador da CLI. Uma chave no `gpg-agent` ou em um cartão OpenPGP serve quando é uma chave ed25519 comum.
- **Chaves:** uma por mantenedor (`mvilacad`, `luancoleto`), qualquer uma assina. A lista é `crates/atlas-core/data/release-keys.json` (`name`, `pubkey`, `valid_after`, `valid_before`, mais `signed_since`). A CLI a embute; `bun scripts/release-keys.ts allowed-signers` a converte em um arquivo `allowed_signers` do OpenSSH para o `ssh-keygen -Y verify`, e o `release-sign.sh` confere a assinatura contra ela antes de enviar, então uma assinatura que os clientes rejeitariam nunca é enviada.
- **Onde é verificada:** `atlas update`, `atlas desktop` e o download que o `atlas crew install` faz na sua própria máquina (o fallback que baixa aqui e envia para o servidor) passam todos por `download_verified` (`crates/atlas-cli/src/commands/release.rs`). Uma tag em `signed_since` ou depois exige `SHA256SUMS` e `SHA256SUMS.sig` da lista de chaves; o asset precisa bater com a sua linha no arquivo assinado. Uma assinatura ausente ou inválida, ou um asset que o arquivo não lista, é um erro e nada é instalado; a CLI não tem override por variável de ambiente. Uma tag anterior a `signed_since` mantém a verificação do `.sha256` e um aviso. Builds de release ignoram `ATLAS_RELEASE_POLICY`, o ponto de teste dos builds de debug. Os scripts de shell (`install.sh`, `install.ps1` e o script que o `atlas crew install` roda no servidor) ainda verificam só o `.sha256` até o [passo 3 da transição](#transição).

### Gerar uma chave (uma vez por mantenedor)

Na sua própria máquina, nunca no CI e nunca em uma pasta sincronizada por um gerenciador de senhas:

```bash
ssh-keygen -t ed25519 -C "atlas-release mvilacad" -f ~/.ssh/atlas-release   # defina uma passphrase
cat ~/.ssh/atlas-release.pub
```

Adicione a chave ao `release-keys.json` em um pull request, com `valid_before` de 12 a 24 meses à frente (`valid_after` = hoje), e peça que o **outro mantenedor confirme a impressão digital** (`ssh-keygen -lf ~/.ssh/atlas-release.pub`) por um canal que não seja o GitHub. Faça um backup offline (cifrado) da chave privada; ela não existe em nenhum outro lugar.

### Assinar uma versão

O `bun run release <channel>` termina com isso. À mão, para um rascunho que está esperando:

```bash
scripts/release-sign.sh vX.Y.Z --key ~/.ssh/atlas-release
```

Ele baixa o `SHA256SUMS`, mostra o arquivo, pergunta, roda `ssh-keygen -Y sign -n atlas-release`, verifica o resultado contra o `release-keys.json`, envia o `SHA256SUMS.sig` e roda `gh release edit --draft=false --latest=<flag>` (a flag que o `release-version.ts github-flags` dá: alphas são "Latest" enquanto alpha é o canal padrão). Rodar de novo é seguro.

### Rotacionar uma chave

1. Adicione a nova chave na versão N (um pull request que muda o `release-keys.json`) e assine a versão N com a chave **antiga**: os clientes que confiam só na antiga aceitam a N e, a partir da N, conhecem a nova.
2. A versão N+1 em diante pode ser assinada com a nova chave. Quando a N for a versão mais antiga que você ainda mantém, remova a chave antiga na N+k.
3. Toda chave tem `valid_before` (12 a 24 meses). Uma chave não rotacionada deixa de ser aceita pelos clientes novos nesse dia, sozinha, e esse é o limite real de quanto tempo uma chave vazada é perigosa: clientes que nunca atualizam continuam confiando em uma chave removida até lá.

### Chave perdida ou vazada

- **Perdida** (o outro mantenedor ainda tem a dele): o outro mantenedor assina uma versão que adiciona uma chave de reposição e remove a perdida. Nada quebra.
- **Vazada ou suspeita de vazamento**: remova a chave do `release-keys.json` e do `allowed_signers` dos instaladores imediatamente, publique uma versão assinada pelo outro mantenedor e diga isso nas notas da versão. As versões já assinadas com ela não ficam seguras retroativamente; os clientes confiam nela até atualizarem ou até o `valid_before` passar. Prefira prazos curtos.
- **As duas perdidas**: nenhuma versão que os clientes existentes aceitem pode ser assinada. Os clientes existentes recusam versões sem assinatura, então a recuperação é uma instalação nova a partir de uma fonte em que você confia, mais um cliente compilado com chaves novas; guarde as duas chaves em dois backups diferentes para evitar isso.

### Ativar versões imutáveis

Settings › General › Releases › **Enable release immutability** (`gh api repos/codeatlasdev/atlas/immutable-releases` mostra `{"enabled": ...}`). Assets podem ser enviados enquanto a versão é um rascunho e ficam congelados na publicação, que é a ordem que o `release-sign.sh` usa (envia a assinatura, depois publica). Uma versão publicada não pode mais ser editada, ter assets trocados nem ter a tag movida; o GitHub então também atesta a versão (`gh release verify`). Ligue quando a primeira versão assinada for gerada; uploads com `--clobber` contra uma versão publicada deixam de funcionar, por design.

### Transição

A assinatura chega em etapas para que ninguém fique para trás:

1. **Verificador, scripts e workflow, sem mudança de comportamento** (esta etapa): `signed_since` sem valor, lista de chaves vazia; a CLI mantém o caminho do `.sha256`; o `release.yml` compila rascunhos e o `release-sign.sh` os publica sem assinatura, com um aviso, enquanto nenhuma chave está configurada.
2. **O PR de versão da primeira versão assinada** (a primeira versão depois da que traz esta etapa): as duas chaves públicas no `release-keys.json`, `signed_since` = essa tag. A versão é publicada como rascunho e assinada. Os clientes antigos (alpha.26 e anteriores, o atualizador do app rodando a CLI antiga) leem o `.sha256` por asset nesse salto e a instalam; todo salto a partir daí é verificado por assinatura. Continue publicando o `.sha256` por asset enquanto puderem existir clientes mais antigos que essa versão.
3. **`install.sh`, `install.ps1` e o script de download do crew** passam a exigir assinatura (`allowed_signers` embutido, `ssh-keygen -Y verify`, hash tirado do arquivo assinado, falha fechada, `ATLAS_ALLOW_UNSIGNED=1` como a recusa explícita quando falta o `ssh-keygen`) no **mesmo PR que faz da primeira tag assinada o bootstrap permitido mais antigo**, ou seja, o PR de versão do passo 2. Até uma tag assinada ser a mais nova, eles mantêm o caminho legado.
4. As tags antigas (alpha.26 e anteriores) continuam sem assinatura: uma assinatura feita hoje garantiria quaisquer bytes que estejam na versão agora. Instalações explícitas delas mostram um aviso.
5. O `bun run release stable` passa pelo `release-sign.sh` como todo outro canal, então uma versão promovida é assinada; senão uma CLI que conhece assinaturas a recusaria.

### Lista de verificação da versão

1. `bun run release <channel>`; acompanhe a execução (`gate` → builds → `verify-linux` → `publish` → `attest`).
2. No prompt de assinatura, leia os hashes; confira um deles contra a atestação: baixe um asset e rode `gh attestation verify <asset> --repo codeatlasdev/atlas`.
3. Depois de pública: `gh release verify vX.Y.Z --repo codeatlasdev/atlas` (quando as versões imutáveis estiverem ligadas), `gh release verify-asset vX.Y.Z <asset> --repo codeatlasdev/atlas` e `ssh-keygen -Y verify` como em [Verificar uma versão](#verificar-uma-versão).
4. `atlas update --check` e um `atlas update` de verdade a partir da versão anterior.

## Auditoria de contraste (antes de cada versão)

O `bun run check` cobre o nível dos tokens (todo token de texto, sintaxe, indicador, anel de foco e borda de campo em toda superfície e tom dos seis temas incluídos: `apps/desktop/src/lib/theme/theme.test.ts`, `debug/contrast.test.ts`). A auditoria em tempo de execução precisa de um build de debug do app, então é um passo da versão, não parte do gate:

```bash
bun run desktop:build:app                 # só o .app de debug (nunca um DMG)
bun scripts/contrast-audit.ts --json /tmp/contrast.json --md /tmp/contrast.md
```

Ela inicia a sua própria instância de agente (nunca a sua tela), passa pelos seis temas e percorre cerca de 35 telas (barra lateral, painel, logs, editor com busca e autocompletar, toda aba das configurações, paleta, Novidades inclusive com uma atualização oferecida, diálogos de confirmar/prompt/descobrir/importar tema, toasts de toda gravidade, menus de linha/projeto/nova aba, tooltip, anel de foco, tour, checklist, folha de atalhos). Em cada tela, ela roda a auditoria de texto (4,5 para o corpo, 3 para grande/desabilitado/placeholder/gutter) e a auditoria de não texto (ícones, bordas de campo e o contorno em foco, 3:1). Saída 0 significa **0 falhas**; corrija tudo o que ela listar antes de criar a tag. Decisões: cores de sintaxe e inlay hints são texto (4,5); controles desabilitados estão isentos de AA pelo WCAG, mas são mantidos em 3 para continuarem visíveis; o limite de 3:1 de um campo de texto é a caixa que envolve o próprio campo (a sua borda ou preenchimento, ou um `label` ao redor); um container que também guarda outros controles (a paleta de comandos, um diálogo) é uma superfície, e um input solto ali é identificado pelo seu placeholder e pelo cursor; um anel/spinner é julgado pela sua parte mais forte (o arco, não a trilha fraca); ícones de tipo de arquivo repetem o nome de arquivo ao lado e são marcados com `data-contrast-decorative` (ignorados pela auditoria de não texto, nunca recoloridos: são cores da marca); o `terminal.minimum_contrast` do terminal por GPU assume 4,5 por padrão (a tabela ANSI não é tocada, as células são clareadas no momento do desenho).
