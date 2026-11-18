---
title: "Política de depreciação"
source: deprecation.md
source_sha: "ccd85849d7e351d6449b2352726277c81ca9c522"
translation: machine
---

# Política de depreciação

Esta página diz como o Atlas remove ou muda coisas de que você pode depender. Ela adapta a [política de depreciação do Kubernetes](https://kubernetes.io/docs/reference/using-api/deprecation-policy/) a um projeto de um mantenedor só. As promessas de cada fase estão na tabela de [Versionamento e canais de versão](versioning.md#o-que-o-atlas-promete-e-quando); esta página não as muda, ela diz como são cumpridas.

> **O Atlas está em alpha.** Até a primeira release de disponibilidade geral, toda superfície abaixo pode mudar entre quaisquer duas releases. O que não muda é que a mudança fica registrada por escrito: veja [Durante o alpha](#durante-o-alpha).

## A superfície pública

Estas são as coisas que a política cobre. Todo o resto (crates internos, a API Rust dos crates `atlas-*`, formatos de linha de log além do que a documentação descreve, a aparência do TUI e do app) pode mudar em qualquer release.

| Superfície | Inclui |
|---|---|
| `atlas.yaml` (e `atlas.local.yaml`) | chaves, o significado delas, padrões e valores aceitos; veja [Configuração](configuration.md) |
| CLI | comandos, subcomandos, flags, os padrões delas e **códigos de saída**; veja a [referência da CLI](cli.md) |
| MCP | nomes das ferramentas, argumentos e campos de resultado delas, recursos e prompts; veja [MCP](mcp.md) |
| Arquivos de configurações | os arquivos em `~/.atlas` e em `.atlas/` de um projeto (chaves e significado); veja [Configurações do desktop](desktop-settings.md) |

As variáveis de ambiente documentadas nestas páginas (por exemplo `ATLAS_UPDATE_CHANNEL`) contam como parte da CLI.

## Durante o alpha

- Uma superfície pode mudar ou ser removida em qualquer release, inclusive em uma release de patch.
- Toda mudança desse tipo aparece no [`CHANGELOG.md`](https://github.com/codeatlasdev/atlas/blob/main/CHANGELOG.md) em **Changed** ou **Removed**, com o que fazer no lugar. Essa é a regra que as notas de release de todas as fases seguem.
- Quando sai barato, a forma antiga continua funcionando por um tempo e imprime um aviso. Isso não é garantido.
- No canal **Alpha**, uma mudança também pode entrar sem nenhum aviso e ser corrigida no build seguinte; a entrada no changelog continua aparecendo.

## Depois da disponibilidade geral

Quando o Atlas estiver em disponibilidade geral (os critérios estão em [Versionamento](versioning.md#disponibilidade-geral)):

1. **Nenhuma breaking change dentro de uma versão major.** Remover ou mudar o significado de algo da superfície pública espera a próxima versão major.
2. **Depreciar primeiro.** Antes de ser removido, algo fica depreciado por **pelo menos uma release minor**. Depreciado significa:
   - continua funcionando exatamente como antes;
   - o changelog o lista em **Deprecated**, com o que usar no lugar e a release que vai removê-lo;
   - usá-lo imprime um **aviso em tempo de execução** (um aviso da CLI no stderr, uma linha no status do `atlas dev`, um toast no app ou um diagnóstico do `atlas.yaml`), uma vez por execução, citando o substituto. Uma depreciação sem substituto diz isso e explica por quê.
3. **Remover em uma release major.** A remoção aparece em **Removed**, com as mesmas instruções, e o aviso some junto com o recurso.
4. **Uma faixa não é depreciada em favor de outra menos estável.** Algo estável não é trocado por algo que ainda está em alpha ou beta.
5. **Códigos de saída e saídas legíveis por máquina** (`--json`, resultados do MCP) só ganham valores ou campos novos em uma release minor; os existentes mantêm o significado. Quem chama deve ignorar os campos que não conhece.

Uma superfície nova começa marcada como **experimental** na documentação e no changelog. A superfície experimental segue a regra de "Durante o alpha" mesmo depois da disponibilidade geral, até a documentação dizer que ela é estável.

## Breaking changes e pull requests

Um pull request que muda a superfície pública leva a [label](labels.md) `breaking change`, uma entrada no changelog em **Changed** ou **Removed** (ou **Deprecated**) dizendo o que fazer no lugar e um teste que falha se o comportamento mudar por acidente. O [template de pull request](https://github.com/codeatlasdev/atlas/blob/main/.github/pull_request_template.md) pede os três.

## Releases retiradas (yanked)

Se uma release é retirada por ser perigosa (perda de dados, um build que não inicia, um problema de segurança) e existe uma versão corrigida, o título dela no changelog recebe a marca `[YANKED]` e uma linha dizendo o motivo e qual versão a substitui, como faz o formato [Keep a Changelog](https://keepachangelog.com/en/1.1.0/#yanked). Uma release retirada continua publicada, então as instalações não quebram e as tags nunca se movem; as notas da release no GitHub apontam para a correção. Uma release que foi **apagada** antes de alguém poder depender dela não recebe a marca.

## Plataformas e toolchain

Deixar de oferecer uma plataforma aparece em **Removed** no changelog. Aumentar a versão mínima do Rust para compilar o Atlas é uma mudança normal, destacada em **Changed** (veja [Contribuir](contributing.md)); ela não faz parte da superfície pública acima.
