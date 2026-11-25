---
title: "Cadeia de suprimentos e proveniência"
source: supply-chain.md
source_sha: "fe74337efbbf2caf46125d3d30e9da6c80b2838a"
translation: machine
---

# Cadeia de suprimentos e proveniência

Esta página é a versão honesta de "quanto você pode confiar em um download do Atlas". Ela lista o que está feito hoje, o que **deliberadamente não é afirmado** e os passos que o mantenedor pode dar a seguir. Como reportar uma vulnerabilidade e como verificar um download estão em [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md); como as versões são geradas está em [Publicar uma versão](release.md).

## O que o Atlas faz hoje

| Área | O que existe | Onde |
|---|---|---|
| Política de dependências | `cargo deny check` (avisos do banco RustSec, licenças em uma lista de permitidas, crates duplicados e proibidos) roda no `bun run check`, no CI (job `cargo-deny`) e antes de toda versão. O único aviso ignorado tem um motivo escrito. | [`deny.toml`](https://github.com/codeatlasdev/atlas/blob/main/deny.toml) |
| Entradas fixadas | `Cargo.lock` e `bun.lock` estão no repositório. O CI, o gate que toda versão roda primeiro (`bun run check`) e o build da versão usam `--locked`, o build do desktop usa `bun install --frozen-lockfile`, e a toolchain do Rust é fixada. | `Cargo.lock`, `rust-toolchain.toml` |
| PRs de atualização | O Dependabot abre atualizações semanais para todo manifesto do repositório: Cargo (o workspace e o crate nativo do app de celular), os cinco projetos bun, Dart e Ruby (Atlas Mobile), o módulo Go e a imagem de build do push relay, e GitHub Actions (workflows e actions compostas). Atualizações minor e patch vêm agrupadas, um pull request por ecossistema, e cada major ganha o seu; os pins das actions vêm num único pull request. Uma versão nova espera 7 dias (`cooldown`) antes do pull request dela, então uma versão comprometida ou retirada costuma ser notada antes. O `bun run deps:outdated` mostra o mesmo conjunto localmente. | [`.github/dependabot.yml`](https://github.com/codeatlasdev/atlas/blob/main/.github/dependabot.yml) |
| Actions fixadas | Todo `uses:` dos workflows é fixado em um SHA de commit completo, com a versão exata em um comentário (`# v7.0.1`). | `.github/workflows/*.yml`, `.github/actions/*` |
| Auditoria de workflows | actionlint (sintaxe, expressões, shellcheck dos blocos `run:`) e [zizmor](https://docs.zizmor.sh) (injeção de template, persistência de credenciais, envenenamento de cache, permissões excessivas, actions não fixadas) rodam no `bun run check` e no job de CI `Workflow lint`. Toda supressão está em `.github/zizmor.yml` com o motivo. | `scripts/lint-workflows.sh` |
| CI com privilégio mínimo | Todo workflow define `permissions: contents: read`; o job `Plan` soma `actions: read` e `pull-requests: read`, o job agregador `CI` não tem nenhuma, e só o job `publish` do `release.yml` sobe o nível (`contents: write`). Todo checkout usa `persist-credentials: false`. Pull requests de forks rodam com um token somente leitura e sem secrets; texto não confiável (o título do pull request) chega aos scripts só por `env`. | `.github/workflows/*.yml` |
| Proveniência das contribuições | Todo commit precisa de um `Signed-off-by` ([DCO](https://developercertificate.org/)), conferido pelo job `Signed-off-by`. | `.github/workflows/pr.yml`, `scripts/check-dco.sh` |
| Integridade das versões | Cada arquivo de uma versão tem um `.sha256`, e o `manifest.json` lista o checksum de todos os arquivos. O `atlas update`, o `atlas desktop` e o instalador verificam o checksum antes de instalar qualquer coisa. | [Instalação](installation.md) |
| SBOM | O job da versão anexa um SBOM CycloneDX da árvore de dependências da CLI (`atlas-<version>-sbom.cdx.json`) quando o `cargo-cyclonedx` está disponível, e avisa em voz alta quando não está, então uma versão pode sair sem um. | `release.yml` |
| Assinatura de código | macOS: a CLI, o `Atlas.app` e o dmg são assinados com **Developer ID** (hardened runtime, timestamp seguro) e notarizados quando os secrets de assinatura estão configurados, ou ad-hoc (`codesign --sign -`) caso contrário. | `release.yml` |
| Build hospedado | As versões são geradas em runners hospedados do GitHub a partir da tag; sem secret de longa duração: o job de publicação usa o `GITHUB_TOKEN` da execução. | [Publicar uma versão](release.md) |
| Superfície de rede | O Atlas não faz nenhuma requisição de rede além de verificações e downloads de atualização no GitHub; não há telemetria. O **Atlas Crew** (quando instalado) também chama `push.codeatlas.com.br` para notificações push (só o tipo do evento) e, quando configurado, `api.telegram.org` para a ponte do Telegram. Os dois podem ser desligados: `ATLAS_PUSH_RELAY=off` e deixar o Telegram sem configuração (Configurações → Atlas Crew; `crew_telegram_set`). | [PRIVACY.md](https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md) |

## O que deliberadamente não é afirmado

- **Nenhum nível SLSA.** O Atlas compila em runners hospedados do GitHub a partir da tag e o job `attest` registra uma attestation do GitHub para os assets publicados, mas não afirma nenhum nível SLSA acima do nível 1 de build.
- **A attestation é do que o `publish` reuniu, não do que cada job de build produziu.** O job `attest` roda depois dos jobs de build, sobre o `SHA256SUMS` que o `publish` calculou a partir dos artefatos deles, então prova "esta execução do workflow publicou estes bytes", não "estes bytes foram compilados a partir deste commit". Atestar a saída de cada job de build provaria mais; isso não é feito.
- **A assinatura Developer ID precisa dos secrets.** Um build sem eles (um fork, ou antes de serem configurados) recebe assinatura ad-hoc, e o Gatekeeper pode pedir confirmação na primeira abertura de um app baixado; um build assinado ainda precisa de uma verificação online para um binário de CLI solto, que não pode receber o ticket de notarização anexado (stapling). O instalador e o `atlas update` baixam com `curl`, que é o caminho que funciona sem aviso.
- **Não é reproduzível bit a bit.** A assinatura de código e o bundle do macOS embutem timestamps; nada aqui promete que dois builds sejam idênticos byte a byte.
- **O arquivo de checksums assinado é obrigatório a partir da primeira versão assinada, não antes.** Até lá, `signed_since` fica sem valor e os clientes verificam só o `.sha256` ([docs/release.md](release.md#assinatura-de-versões)). O SBOM é feito no melhor esforço, veja a tabela acima.
- **Sem auditoria de terceiros.** As dependências passam pelo `cargo deny`, não são revisadas linha a linha.

Proveniência em uma frase: *compilado em runners hospedados do GitHub a partir da tag, assinado com Developer ID e notarizado no macOS (ad-hoc sem os secrets), com checksum, atestado pelo GitHub e (a partir da primeira versão assinada) com o `SHA256SUMS` assinado offline por um mantenedor.*

## Acréscimo à lista de verificação da versão

O `CITATION.cff` traz a versão e a data de lançamento da última versão. A ferramenta de release deveria atualizar `version` e `date-released` ali junto com as versões do Cargo e o `CHANGELOG.md`, e o `scripts/release.sh` é o lugar para fazer isso. Até que faça, atualize os dois à mão no commit da versão.

## Recomendações para o mantenedor

Nada disso está aplicado; cada item exige permissões de dono do repositório ou uma compra, então a decisão é do mantenedor. Estão ordenados por valor em relação ao esforço.

### 1. Ruleset de branch para `main` e `release/*`

**Settings → Rules → Rulesets → New branch ruleset.**

1. Nome `protect main and release`, enforcement **Evaluate** primeiro (ele registra o que seria bloqueado) e depois **Active**.
2. Branches alvo: a branch padrão e o padrão `release/*`.
3. Regras:
   - Restringir exclusões; bloquear force push.
   - Exigir histórico linear.
   - Exigir commits assinados.
   - Exigir um pull request antes do merge, com **0** aprovações exigidas (há um mantenedor só).
   - Exigir que os status checks passem, com os nomes reais dos checks de `.github/workflows`:
     `CI` (o job agregador do `ci.yml`: só passa quando todos os outros jobs passaram, então os jobs podem mudar sem mexer no ruleset), e `Signed-off-by` e `Conventional title` (do `pr.yml`).
4. Lista de bypass: adicione o papel do mantenedor com **For pull requests only**, para que um push direto seja impossível, mas o mantenedor ainda possa fazer merge de um pull request quando um check hospedado estiver indisponível.

Um check obrigatório precisa ser reportado por alguma coisa. O CI roda em runners hospedados do GitHub para pull requests; as versões não precisam deles. Se o mantenedor preferir não gastar esses minutos, tire os checks hospedados da lista e mantenha só o `Signed-off-by`.

### 2. Ruleset de tag para `v*`

Novo ruleset de tag, alvo `v*`, regras: restringir exclusões e restringir atualizações (uma tag publicada nunca deve se mover; o fluxo de release em [Publicar uma versão](release.md) cria tags e nunca as reaponta). Adicione o mantenedor como a única pessoa que pode criá-las.

### 3. Commits e tags assinados

```bash
git config --global gpg.format ssh
git config --global user.signingkey ~/.ssh/id_ed25519.pub
git config --global commit.gpgsign true
git config --global tag.gpgsign true
```

Adicione a mesma chave pública ao GitHub como **signing key** (**Settings → SSH and GPG keys → New SSH key**, tipo *Signing Key*). Ligue o **Vigilant mode** (**Settings → SSH and GPG keys**) para que commits e tags sem assinatura atribuídos ao mantenedor apareçam como *Unverified*. Verifique uma tag com `git tag -v vX.Y.Z`.

### 4. Secret scanning, push protection, private vulnerability reporting

**Settings → Advanced Security (Code security)**: ative **Secret scanning**, **Push protection** e **Private vulnerability reporting**. O `SECURITY.md` já diz a quem reporta para usar o private vulnerability reporting, que só funciona depois de ativado; confira se `https://github.com/codeatlasdev/atlas/security/advisories/new` abre um formulário.

### 5. Versões imutáveis

**Settings → General → Releases → Enable release immutability.** Uma versão publicada então não pode ter arquivos adicionados, substituídos ou removidos, e a tag dela não pode se mover. Duas consequências para decidir antes:

- O fluxo de release precisa criar a versão como **draft**, enviar todos os arquivos e só então publicar. Hoje o job `publish` do `release.yml` a cria já publicada (`draft: false`) com os arquivos em uma etapa só, então precisa de `draft: true` e depois uma etapa de publicação antes de isso ser ligado.
- Uma futura versão-ponteiro móvel de "canais" (uma opção de design para descoberta de canais que não foi construída) entraria em conflito com isso: uma versão imutável não pode ser reenviada com `--clobber`. A lógica de canais do Atlas lê o `releases.atom` (e o redirecionamento `releases/latest` como mais um candidato), não uma versão móvel, então não há conflito hoje.

### 6. Cooldown do Dependabot

O Dependabot já espera 3 dias antes de abrir uma atualização de versão quando nada está configurado. Deixe isso explícito e mais longo para os ecossistemas em que uma versão comprometida tem mais chance de ser retirada em até uma semana. Em cada entrada de `updates:` do `.github/dependabot.yml`:

```yaml
    cooldown:
      default-days: 7
      semver-major-days: 14
```

Atualizações de segurança não são atrasadas por um cooldown.

### 7. OpenSSF Scorecard sem minutos de Actions

Não adicione a Scorecard Action: ela roda em runners hospedados. Em vez disso:

```bash
brew install scorecard
GITHUB_AUTH_TOKEN="$(gh auth token)" scorecard --repo=github.com/codeatlasdev/atlas
```

ou leia a varredura pública semanal em `https://api.scorecard.dev/projects/github.com/codeatlasdev/atlas` (sem configuração). Notas baixas esperadas por construção em um projeto de uma pessoa só: Code-Review, Contributors, Signed-Releases (até o item 9), SAST, Fuzzing. Um *badge* do Scorecard exige a Action com `publish_results: true`, então o Atlas não mostra um.

### 8. Badge OpenSSF Best Practices ("passing")

Inscreva-se em <https://www.bestpractices.dev> com a URL do repositório. A tabela mapeia as categorias de critérios do nível *passing* para o que o Atlas já cumpre. **Met** significa que um arquivo no repositório prova; **Unmet** significa uma lacuna a fechar antes de se inscrever; **N/A** é permitido pelos critérios.

| Categoria | Critérios (ids) | Status | Evidência ou lacuna |
|---|---|---|---|
| Basics | `description_good`, `interact`, `contribution`, `contribution_requirements` | Met | [README.md](https://github.com/codeatlasdev/atlas/blob/main/README.md), [CONTRIBUTING.md](https://github.com/codeatlasdev/atlas/blob/main/CONTRIBUTING.md) |
| Basics | `floss_license`, `floss_license_osi`, `license_location` | Met | `LICENSE-MIT`, `LICENSE-APACHE` na raiz |
| Basics | `documentation_basics`, `documentation_interface` | Met | [`docs/`](https://github.com/codeatlasdev/atlas/tree/main/docs), [referência da CLI](cli.md) |
| Basics | `sites_https`, `english`, `maintained` | Met | `https://atlas.codeatlas.com.br`; documentação em inglês; versões e commits continuam |
| Basics | `discussion` | Met | issues e pull requests do GitHub são pesquisáveis, endereçáveis por URL e abertos a qualquer pessoa; Discussions não são necessárias (veja [labels e triagem](labels.md)) [INFERÊNCIA: confirme com o texto do critério ao se inscrever] |
| Change control | `repo_public`, `repo_track`, `repo_interim`, `repo_distributed` | Met | repositório Git público no GitHub |
| Change control | `version_unique`, `version_semver`, `version_tags` | Met | [Versionamento](versioning.md); tags `vX.Y.Z` |
| Change control | `release_notes`, `release_notes_vulns` | Met | [CHANGELOG.md](https://github.com/codeatlasdev/atlas/blob/main/CHANGELOG.md); o `SECURITY.md` promete crédito e uma entrada **Security** para toda vulnerabilidade corrigida |
| Reporting | `report_process`, `report_tracker`, `report_responses`, `enhancement_responses`, `report_archive` | Met | formulários de issue em `.github/ISSUE_TEMPLATE/`, [metas de triagem](labels.md#política-de-triagem) |
| Reporting | `vulnerability_report_process`, `vulnerability_report_private`, `vulnerability_report_response` | Met quando o item 4 estiver feito | [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md) (reporte privado, confirmação em 7 dias, avaliação em 14) |
| Quality | `build`, `build_common_tools`, `build_floss_tools` | Met | `cargo build`, `rust-toolchain.toml` |
| Quality | `test`, `test_invocation`, `test_most`, `test_continuous_integration`, `test_policy`, `tests_are_added`, `tests_documented_added` | Met | `cargo test --workspace`, `.github/workflows/ci.yml`, regra de testes primeiro no [AGENTS.md](https://github.com/codeatlasdev/atlas/blob/main/AGENTS.md) e no template de PR |
| Quality | `warnings`, `warnings_fixed`, `warnings_strict` | Met | `clippy -D warnings`, `[workspace.lints]` no `Cargo.toml` |
| Security | `know_secure_design`, `know_common_errors` | Met (autodeclarado) | modelo de ameaças no `SECURITY.md`, regras de segurança em tempo de execução no `AGENTS.md` |
| Security | `crypto_*` | N/A | O Atlas não implementa criptografia própria; o TLS vem do `rustls` pelo `reqwest` |
| Security | `delivery_mitm`, `delivery_unsigned` | Met para HTTPS; **Unmet para assinaturas** até o item 9 | os downloads são HTTPS e verificados contra um `.sha256` publicado ao lado, mas esse arquivo vem do mesmo lugar que o arquivo da versão |
| Security | `vulnerabilities_fixed_60_days`, `vulnerabilities_critical_fixed` | Met por política | meta de 30 dias para correção ou mitigação no `SECURITY.md` |
| Security | `no_leaked_credentials` | Met | nenhum secret no repositório; o secret scanning (item 4) garante isso |
| Analysis | `static_analysis`, `static_analysis_fixed` | Met se o clippy for aceito como analisador | `clippy -D warnings` no CI; CodeQL ou Semgrep tirariam a dúvida, mas rodam em minutos hospedados |
| Analysis | `dynamic_analysis*` | N/A ou sugerido | os critérios são SUGERIDOS, não exigidos, para o nível passing |

O texto dos critérios está no site do badge e muda; releia cada resposta quando se inscrever.

### 9. Um `SHA256SUMS` assinado (construído; obrigatório a partir da primeira versão assinada)

Um `.sha256` ao lado de um arquivo protege contra um download corrompido, não contra uma versão adulterada, porque os dois vêm do mesmo lugar. O `SHA256SUMS` de cada versão agora é assinado offline por um mantenedor (`sshsig` do OpenSSH, namespace `atlas-release`; `scripts/release-sign.sh`) e a CLI o verifica com as chaves de `crates/atlas-core/data/release-keys.json`, que é o que um token de upload roubado, uma conta comprometida ou um workflow envenenado não conseguem forjar. Por que uma chave SSH offline e não o Sigstore keyless ou o minisign como âncora de confiança, a tabela de ameaças, a rotação de chaves e a transição: [docs/release.md](release.md#assinatura-de-versões) e `SECURITY.md#verifying-downloads`. O check *Signed-Releases* do Scorecard procura `*.sig` entre os arquivos da versão, então a primeira versão assinada também melhora essa nota. Falta: ligar as versões imutáveis (abaixo) nessa versão.

### 10. Assinatura Developer ID e notarização

Feito no `release.yml` (a participação no Apple Developer Program e o certificado Developer ID Application são da CODEATLAS LTDA, Team ID `58UYCA3ZYA`): a CLI, o `Atlas.app` e o dmg são assinados com o hardened runtime e notarizados, com o certificado em um keychain temporário no runner. A assinatura liga quando os seis secrets do repositório existem; como ela funciona, os entitlements, como definir os secrets e como rotacioná-los estão em [Publicar uma versão](release.md#assinatura-e-notarização-no-macos). Um binário de CLI solto não pode receber o ticket anexado (o Gatekeeper confere online uma vez); o app e o dmg recebem o ticket. É um dos critérios de [disponibilidade geral](versioning.md#disponibilidade-geral).

### 11. SBOM

A versão já anexa um SBOM CycloneDX quando a ferramenta instala. Para torná-lo uma garantia, faça a ausência do `cargo-cyclonedx` falhar a versão em vez de apenas avisar. O `cargo-auditable` é uma alternativa que embute a lista de dependências no binário para que o `cargo audit bin` possa varrer um `atlas` instalado; ele não está ligado.

### 12. Arquivos de comunidade no nível da organização

Se a CodeAtlas tiver mais repositórios, um repositório `codeatlasdev/.github` (que não existe hoje) pode hospedar `CODE_OF_CONDUCT.md`, `SECURITY.md` e `SUPPORT.md` compartilhados, que todo repositório sem cópia própria herda.
