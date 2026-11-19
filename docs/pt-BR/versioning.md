---
title: "Versionamento e canais de versão"
source: versioning.md
source_sha: "dc3dd911095b1f9f69edb32d748862b056bfe3c0"
translation: machine
---

# Versionamento e canais de versão

> **O Atlas está em alpha.** Todo canal, inclusive o Stable, pode ter arestas e mudanças incompatíveis até a primeira versão que cumprir os [critérios de disponibilidade geral](#disponibilidade-geral) abaixo. Os canais descrevem o quanto um build foi testado, não o quanto o Atlas está pronto.

O Atlas lança a CLI e o app desktop juntos, com a mesma versão. Toda versão é construída e publicada pelo GitHub Actions a partir da tag da versão (veja [Publicar uma versão](release.md)).

## Canais

Você escolhe o que recebe. **Até o primeiro Stable que cumpra os [critérios de disponibilidade geral](#disponibilidade-geral), o padrão é o Alpha**: uma instalação ou atualização que nunca escolheu um canal segue o build mais novo do `main`. Quem escolheu um canal (o instalador, as Configurações, `atlas update channel`, `ATLAS_UPDATE_CHANNEL`) o mantém, e o Stable fica a um comando de distância. Quando existir um Stable de verdade, o padrão volta a ser o **Stable**.

| Canal | Como a versão aparece | Gerado a partir de | Risco |
|---|---|---|---|
| **Stable** | `1.4.0` | uma branch `release/X.Y`, depois de um beta | o mais baixo |
| **Beta** | `1.4.0-beta.2`, `1.4.0-rc.1` | uma branch `release/X.Y` | médio |
| **Alpha** | `1.5.0-alpha.3` | `main` | o mais alto |

### Stable

Só builds que já rodaram como beta, sem nada alterado desde então além do número da versão.

- **Risco:** o mais baixo dos três.
- **Frequência:** quando um beta já testado é promovido, em geral a cada poucas semanas.
- **Para quem:** quem usa o Atlas em trabalho de verdade e quer só builds já testados.
- **Ao trocar para ele:** você vai para a próxima versão Stable que seja mais nova que a versão em uso. O Atlas nunca instala um build mais antigo por conta própria.
- Enquanto o Atlas está em alpha, Stable significa testado em alpha e beta, não terminado.

### Beta

Builds com todos os recursos, em teste antes de virarem Stable. Entre betas, só entram correções.

- **Risco:** quase tudo funciona; um bug pode escapar.
- **Frequência:** um novo beta a cada uma ou duas semanas enquanto uma versão está em teste.
- **Para quem:** quem quer os recursos da próxima versão com algumas semanas de antecedência e vai reportar o que quebrar.
- **Ao trocar para ele:** você recebe o beta ou release candidate mais novo, e o Stable assim que ele for mais novo. Voltar ao Stable depois espera por um Stable mais novo que o seu build, ou você pode reinstalar o Stable explicitamente.

### Alpha

O padrão enquanto o Atlas está em alpha. Builds novos, gerados direto do main, com pouco teste. Recursos podem estar incompletos e o comportamento pode mudar entre builds.

- **Risco:** espere arestas e mudanças incompatíveis.
- **Frequência:** sempre que o mantenedor gera um, muitas vezes várias vezes por semana.
- **Para quem:** contribuidores e testadores que aceitam quebras em troca do trabalho mais novo.
- **Ao trocar para ele:** você recebe o build mais novo de qualquer canal, então uma configuração ou um arquivo de config pode não ser legível por um Atlas mais antigo. Voltar ao Beta ou ao Stable depois espera por um build mais novo ali, ou você pode reinstalá-lo explicitamente.

### Trocar de canal

| Onde | Como |
|---|---|
| Desktop | Configurações → Geral → Atualizações → **Canal de atualização** |
| CLI | `atlas update channel` (mostra cada canal e o que ele oferece agora), `atlas update channel beta` (define o canal), `atlas update --channel beta` (define e atualiza) |
| Scripts / CI | `ATLAS_UPDATE_CHANNEL=beta` substitui a escolha salva |
| Instalador | `curl -fsSL https://atlas.codeatlas.com.br/install.sh \| ATLAS_CHANNEL=stable bash` (ou `beta`, `alpha`); sem a variável, o instalador segue o canal padrão |

A escolha é uma palavra em `~/.atlas/update-channel`, compartilhada pela CLI e pelo app.

**O Atlas nunca faz downgrade por conta própria.** Se você muda para um canal mais conservador (Alpha → Stable) enquanto roda um build mais novo do que esse canal oferece, nada é instalado. O Atlas avisa, e você espera por uma versão mais nova que o seu build ou escolhe **Instalar a versão X agora** (CLI: `atlas update --downgrade`). Um downgrade pode deixar configurações ou dados gravados pelo build mais novo; tenha isso em mente antes de usá-lo.

## Números de versão

O Atlas usa o [Versionamento Semântico 2.0.0](https://semver.org/spec/v2.0.0.html).

- **Versões finais:** `MAJOR.MINOR.PATCH`, tag `vMAJOR.MINOR.PATCH`.
- **Pré-lançamentos:** `-alpha.N`, `-beta.N`, `-rc.N`, com um **ponto antes do contador** (`1.4.0-alpha.10` é mais novo que `1.4.0-alpha.9`; `alpha10` seria comparado como texto). A precedência é `alpha < beta < rc < versão final`, e um pré-lançamento do próximo minor é mais novo que a versão final anterior (`1.5.0-alpha.1 > 1.4.0`).
- Metadados de build (`+…`) nunca são usados.
- Qualquer outro rótulo (`nightly`, `dev`, …) não é oferecido em nenhum canal.
- **O que o "Latest" do GitHub significa.** Enquanto o canal padrão é o Alpha (antes da disponibilidade geral), uma versão do canal padrão é publicada como versão normal e marcada como **Latest**, então o selo, a página de versões e o `releases/latest` mostram o build mais novo que recebe quem nunca escolheu: o alpha mais novo. Builds beta e rc continuam marcados como **pré-lançamento**. Isto *não* é o canal Stable: o Atlas, o instalador e o site nunca leem o Stable em `releases/latest`; o Stable é a tag mais nova sem rótulo de pré-lançamento no `releases.atom`. Na disponibilidade geral, o padrão volta a ser o Stable, toda tag com `-` volta a ser um pré-lançamento e o "Latest" é o Stable mais novo. As flags são decididas pelo `scripts/release-version.ts github-flags` a partir de `default` em `crates/atlas-core/data/channels.json`.

Um canal oferece a versão mais nova, pela precedência SemVer, entre as versões que aceita: o Stable aceita só versões finais; o Beta aceita versões finais, `beta` e `rc`; o Alpha aceita tudo isso mais `alpha`. Um canal, portanto, nunca é pior que o mais conservador que ele.

## Como uma versão passa de um canal a outro

```text
main        ── 1.5.0-alpha.1 ── 1.5.0-alpha.2 ──▶            (Alpha)
   \
    release/1.4 ── 1.4.0-beta.1 ── fix ── 1.4.0-beta.2 ── 1.4.0 ── fix ── 1.4.1    (Beta, then Stable)
```

1. O trabalho entra no `main`, que está sempre pronto para sair como o próximo alpha.
2. Quando uma versão tem todos os recursos, o primeiro beta cria a `release/X.Y` a partir do `main`. Daí em diante, só correções chegam a ela: uma correção entra primeiro no `main` e é levada com cherry-pick.
3. **A promoção a Stable** parte de um beta (ou release candidate) que já foi testado, e o commit Stable só pode diferir dele **nos arquivos de versão e no `CHANGELOG.md`**. A ferramenta de release recusa o contrário, então o Stable é um beta testado mais um número de versão. Na era alpha, o tempo mínimo de teste é de 48 horas em beta; o mantenedor pode abrir mão disso, e as notas da versão dizem quando isso acontece.
4. Hotfixes do Stable mais novo são versões `X.Y.(Z+1)` da `release/X.Y`.

As notas de versão ficam no [`CHANGELOG.md`](https://github.com/codeatlasdev/atlas/blob/main/CHANGELOG.md): a seção de um pré-lançamento lista o que mudou desde o pré-lançamento anterior, e a seção Stable é o resumo completo desde o Stable anterior. *Novidades* esconde a seção de um pré-lançamento quando o Stable dele está ao alcance.

## Suporte

Enquanto o Atlas está em alpha, **só o Stable mais novo recebe correções**, e as correções chegam ao Beta e ao Alpha pelo fluxo normal. Não existe linha de suporte de longo prazo. As correções de segurança seguem o [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md).

| | Correções | Correções de segurança |
|---|---|---|
| Stable mais novo | sim | sim |
| Beta atual | sim, no próximo beta | sim |
| Alpha atual | próximo alpha | sim |
| Qualquer coisa mais antiga | não | não |

## O que o Atlas promete, e quando

| Superfície | Durante o alpha | Depois da disponibilidade geral |
|---|---|---|
| Chaves e significado do `atlas.yaml` | podem mudar entre quaisquer duas versões, avisado no changelog | nenhuma mudança incompatível dentro de uma versão major; uma chave fica obsoleta por pelo menos uma versão minor (com aviso) antes de ser removida |
| Comandos, flags e códigos de saída da CLI | podem mudar, avisado no changelog | a mesma regra |
| Nomes e argumentos das ferramentas MCP | podem mudar, avisado no changelog | a mesma regra |
| Arquivos de configuração (`~/.atlas`) | só para frente; um build mais antigo pode não ler um arquivo mais novo | migrações só para frente; uma atualização a partir do Stable anterior nunca perde dados |
| Caminho de atualização | `atlas update` a partir de qualquer versão anterior | a partir do Stable anterior e dos betas dele |

Toda mudança incompatível, em qualquer fase, aparece em **Changed** ou **Removed** no changelog, com o que fazer no lugar.

## Disponibilidade geral

O Atlas sai do alpha com uma versão deliberada, não com um evento de número de versão. A primeira versão Stable que cumprir **todos** estes critérios é declarada em disponibilidade geral, e os avisos de alpha saem na mesma versão:

1. O `atlas.yaml`, a superfície da CLI (comandos, flags, códigos de saída) e os nomes das ferramentas MCP estão documentados e cobertos por testes que falham diante de uma quebra não intencional.
2. A regra de obsolescência da tabela acima está em vigor, e a ferramenta de release exige uma entrada no changelog para toda mudança incompatível.
3. Uma atualização a partir do Stable anterior foi testada em uma instalação real (config, configurações do desktop, terminal host), incluindo a mensagem de downgrade.
4. A versão passou pelo menos 30 dias como beta sem nenhum problema aberto de perda de dados, falha ao iniciar ou segurança.
5. A janela de suporte e as metas de resposta a problemas de segurança do `SECURITY.md` estão publicadas e foram cumpridas nas duas últimas versões.
6. Os builds de macOS são assinados com Developer ID e notarizados, ou a documentação de instalação diz com honestidade por que não.

O número de versão dessa versão é o que o próximo minor seria; o Atlas não pula para o 2.0 para marcá-la.

Nessa mesma versão, o canal padrão volta a ser o Stable: `#[default]` em `Channel` em `crates/atlas-core/src/channel.rs`, `"default"` em `crates/atlas-core/data/channels.json` (o que o `release.yml` lê: daí em diante, tags com `-` voltam a ser pré-lançamentos e só um Stable é "Latest"; um teste falha se ele diferir do atributo), `DEFAULT_CHANNEL` em `site/functions/_lib/channels.ts` (um teste falha se os dois diferirem), `CHANNEL=${ATLAS_CHANNEL:-…}` em `scripts/bootstrap.sh`, a frase `The default while Atlas is in alpha.` na promessa do Alpha (`crates/atlas-core/data/channels.json`, repetida acima) e `site/public/install.sh`. Instalações que escolheram um canal não são tocadas; uma instalação que só seguiu o padrão passa para o Stable assim que o Stable for mais novo que o build dela, e nunca sofre downgrade.

## Verificar um download

Cada versão lista um `.sha256` ao lado de cada arquivo e um `manifest.json` com a versão, a data, o tamanho e o checksum de cada arquivo. O `atlas update` e o instalador conferem o checksum antes de instalar.
