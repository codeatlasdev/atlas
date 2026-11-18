---
title: "Labels e triagem"
source: labels.md
source_sha: "111e3a6f9571af827cd1d83a1cc8607db2279cab"
translation: machine
---

# Labels e triagem

O Atlas tem um conjunto pequeno de labels, mantido em [`.github/labels.yml`](https://github.com/codeatlasdev/atlas/blob/main/.github/labels.yml). Cada label é `família: valor`, então as famílias aparecem juntas no seletor de labels. Esta página diz o que cada família significa, quem a aplica e o que esperar depois que você abre uma issue.

## Famílias de labels

| Família | Valores | Aplicada por | Regra |
|---|---|---|---|
| `type:` | `bug`, `feature`, `docs`, `chore`, `security`, `question` | o formulário da issue define; um mantenedor corrige | exatamente uma |
| `area:` | `cli`, `runtime`, `tui`, `desktop`, `terminal`, `editor`, `mcp`, `deploy`, `installer`, `site`, `ci` | um mantenedor, na triagem | uma ou mais |
| `priority:` | `0 data loss or security`, `1 high`, `2 normal`, `3 low` | um mantenedor, na triagem | no máximo uma; quem contribui não define |
| `status:` | `needs-triage`, `needs-repro`, `confirmed`, `blocked`, `upstream`, `stale`, `wontfix`, `duplicate` | os formulários de issue definem `needs-triage`; um mantenedor muda | uma de cada vez |
| `channel:` | `stable`, `beta`, `alpha` | um mantenedor, a partir da resposta em **Release channel** no formulário de bug | o canal em que o problema apareceu; veja [Versionamento](versioning.md) |

Labels sem família:

| Label | Significado |
|---|---|
| `good first issue` | Pequena, bem descrita, sem contexto escondido. O GitHub lista essas issues para quem está começando a contribuir, então a issue diz onde olhar e o que significa estar pronto. |
| `help wanted` | Um mantenedor aceitaria um pull request; pode exigir mais experiência que uma primeira issue. |
| `breaking change` | A mudança afeta o `atlas.yaml`, a CLI, as ferramentas MCP ou os arquivos de configurações. Precisa de uma entrada **Changed** ou **Removed** no changelog dizendo o que fazer no lugar, e segue a [política de depreciação](deprecation.md). |
| `skip-changelog` | Um pull request sem mudança visível para o usuário, então nenhuma linha no `CHANGELOG.md` é esperada. |
| `dependencies` | Atualizações de dependências, aplicada pelo Dependabot. |

### Prioridades

| Label | Significa | Exemplo |
|---|---|---|
| `priority: 0 data loss or security` | Perda de dados, crash ao iniciar ou impacto de segurança. Tratada primeiro; para vulnerabilidades, veja o [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md) | Uma atualização que apaga as configurações em `~/.atlas` |
| `priority: 1 high` | Um fluxo principal está quebrado e não há solução alternativa razoável | O `atlas dev` nunca chega a um serviço pronto |
| `priority: 2 normal` | Um problema real com solução alternativa, ou uma melhoria clara | Uma flag se comporta de forma diferente da documentação |
| `priority: 3 low` | Cosmético, raro ou desejável | Uma coluna desalinhada em um tema |

## Política de triagem

O Atlas é mantido por uma pessoa, no tempo livre ([GOVERNANCE.md](https://github.com/codeatlasdev/atlas/blob/main/GOVERNANCE.md)). Estas são metas, não garantias; a [página de suporte](https://github.com/codeatlasdev/atlas/blob/main/SUPPORT.md) diz que não há SLA de suporte. As vulnerabilidades têm metas próprias, mais firmes, no [SECURITY.md](https://github.com/codeatlasdev/atlas/blob/main/SECURITY.md).

| Etapa | Meta | O que acontece |
|---|---|---|
| Primeira resposta | em até 7 dias | Uma pessoa lê a issue, define `type:` e `area:` e pede o que falta ou passa a issue para `status: confirmed`. |
| Reprodução | na mesma passada | Uma issue que não dá para tratar sem mais detalhes recebe `status: needs-repro` e um comentário listando exatamente o que falta (versão, canal, um `atlas.yaml` mínimo, logs). |
| Prioridade | na confirmação | Um mantenedor define `priority:`. |
| Pull requests | primeira revisão em até 7 dias | Um mantenedor revisa ou diz quando vai revisar. |

### Fechar issues paradas

Não há bots: os minutos do Actions ficam para o CI. As issues paradas são tratadas à mão, com o `gh`, então todo fechamento tem uma pessoa por trás.

- Uma issue em `status: needs-repro` sem resposta há **30 dias** recebe `status: stale` e um comentário dizendo que será fechada em 14 dias.
- Sem resposta depois desses **14 dias**, ela é fechada como *not planned*, com um comentário dizendo que pode ser reaberta com as informações que faltam.
- Bugs confirmados, features aceitas e issues `help wanted` **nunca** são fechadas por inatividade.
- Issues `status: wontfix` e `status: duplicate` são fechadas na hora, com o motivo ou o link para a outra issue.

Um mantenedor pode listar as candidatas com:

```bash
gh issue list --label "status: needs-repro" --state open --json number,title,updatedAt \
  --jq '.[] | select(.updatedAt < (now - 30*86400 | todate)) | "#\(.number) \(.title)"'
```

## Manter as labels em sincronia

O `.github/labels.yml` é a fonte da verdade; as labels do repositório são ajustadas para bater com ele a partir da máquina de um mantenedor, sem gastar minutos do Actions:

```bash
scripts/sync-labels.sh --dry-run   # imprime os comandos `gh label create`, não executa nada
scripts/sync-labels.sh             # cria as labels que faltam, atualiza cor e descrição (gh label create --force)
scripts/sync-labels.sh --prune     # também lista as labels do GitHub que não estão no arquivo; nunca apaga
scripts/test-labels.sh             # verifica o arquivo: nomes únicos, cores de 6 dígitos, descrições de no máximo 100 caracteres
```

Mudar uma label significa editar o `.github/labels.yml` em um pull request; um mantenedor roda a sincronização depois do merge. Renomear uma label no arquivo cria uma nova: renomeie primeiro no GitHub (Issues → Labels → Edit) e depois mude o arquivo, para que as issues existentes a mantenham. As notas de release são escritas à mão a partir do [`CHANGELOG.md`](https://github.com/codeatlasdev/atlas/blob/main/CHANGELOG.md), então as labels não as geram.
