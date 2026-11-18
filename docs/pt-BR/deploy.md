---
title: "Deploy"
source: deploy.md
source_sha: "5425c0806524d3602f6f4e0cb193a1af4fad2856"
translation: machine
---

# Deploy: `atlas deploy`

O `atlas deploy` publica uma release em um único host e só troca para ela depois que todos os serviços estão saudáveis. Configure na seção `deploy:` do `atlas.yaml`.

Você faz o build uma vez, na sua máquina ou no CI. O Atlas empacota o artefato, envia por ssh e inicia um **driver** no host. O driver roda por conta própria, separado do ssh, e é dono de toda a transação: lock, selo, instalação, migração, troca, reinício, gate de saúde e rollback. O processo local do `atlas` só acompanha o log do driver. Ctrl-C, um job de CI cancelado ou uma conexão que cai não deixam o host com o deploy pela metade.

## Garantias

| # | Garantia | Como |
|---|---|---|
| 1 | O host termina o que começou | O driver roda sob `systemd-run` quando o systemd é o PID 1 e o atlas roda como root; senão, sob `setsid nohup`. Matar o ssh não o mata. |
| 2 | Um deploy por vez, e o lock nunca fica velho | O driver segura um `flock` do kernel em `.atlas/lock` durante toda a vida dele. O lock morre com o processo, então nunca precisa de TTL e ninguém o rouba. |
| 3 | As releases são imutáveis | Os ids de release são `YYYYMMDDHHMMSS-<sha7>[-dirty]`. O upload cai em `releases/.<id>.partial`, o sha256 é conferido e só então ele é renomeado. A ordenação sempre usa o id, nunca o mtime, então o timestamp é o maior entre o relógio e um segundo depois da release mais nova do host (lida pela proteção do deploy): dois deploys do mesmo commit no mesmo segundo, ou um relógio do cliente atrás do relógio do host, nunca reutilizam um id nem ficam antes de uma release existente. |
| 4 | A troca é atômica | `ln -sfn releases/<id> current.tmp.<id> && mv -Tf current.tmp.<id> current && sync -f`. |
| 5 | O rollback vai para a última release *saudável* | Cada release registra `sealed`, `healthy` ou `failed` em `releases/<id>/.atlas-release`. Releases com falha são puladas. |
| 6 | Saudável significa que continua de pé | Um serviço precisa de `health_passes` verificações seguidas aprovadas e depois de uma janela `soak` sem reinício. Os reinícios são detectados por `NRestarts`/`ActiveEnterTimestampMonotonic` do systemd ou `RestartCount`/`StartedAt` do docker. |
| 7 | Todo deploy fica registrado | O `.atlas/state.json` guarda a fase atual e é gravado de forma atômica. O `.atlas/deploys.jsonl` é um log de auditoria só de acréscimo. |

Se um deploy é interrompido depois da troca (o host reiniciou, ou um `kill -9` atingiu o driver), o próximo deploy ou rollback vê a fase não terminal no `state.json`. Ele registra `recovering interrupted deploy <id>`, marca essa release como falha e restaura a última release saudável antes de fazer qualquer outra coisa.

## Configuração

```yaml
deploy:
  app: shop                 # [a-z0-9-]+, padrão: nome do projeto
  build: bun run build      # `sh -c` local na raiz do projeto (pule com --skip-build)
  build_heavy: true         # opcional: roda o build pela fila de jobs pesados da máquina (`atlas run --heavy`)
  artifact: [dist, package.json, bun.lock]   # omita para enviar `git archive HEAD`
  targets:
    prod:
      host: prod            # host ssh ou alias do ~/.ssh/config; `local` roda nesta máquina
      dir: /opt/shop        # padrão /opt/<app>
      keep: 5               # releases mantidas (>= 2)
      link:                 # caminho relativo à release -> arquivo compartilhado absoluto (os secrets ficam aqui)
        .env: /opt/shop/shared/.env.production
      install: bun install --frozen-lockfile   # na nova release, antes da troca
      migrate: ./scripts/migrate.sh            # na nova release, antes da troca (só expand!)
      hooks:
        pre_swap: ./scripts/smoke.sh           # cwd = nova release; código diferente de zero aborta
        post_swap: ./scripts/sync-infra.sh     # cwd = current; roda de novo no rollback
        post_deploy: ./scripts/notify.sh       # só aviso
        on_failure: ./scripts/page.sh          # só aviso; recebe ATLAS_DEPLOY_REASON
      pause_marker: /opt/shop/shared/.deploy-lock  # tocado a cada 30 s enquanto o driver roda
      health_timeout: 60    # segundos por serviço
      health_passes: 3      # verificações seguidas aprovadas
      soak: 15              # segundos sem reinício
      min_free_mb: 1024
      services:             # reiniciados estritamente nesta ordem
        - { name: shop-api, kind: systemd, health: "http://127.0.0.1:3000/health" }
        - { name: worker, kind: compose, compose_file: compose.yml }
        - { name: cron, kind: command, restart: "systemctl reload cron" }
```

| Tipo | Reinício | Estado ativo, quando `health` não está definido | Impressão digital do reinício |
|---|---|---|---|
| `systemd` | `systemctl reset-failed` + `systemctl restart <name>` | `systemctl is-active` | `NRestarts`, `ActiveEnterTimestampMonotonic` |
| `compose` | `docker compose -p <app> -f current/<compose_file> up -d --no-deps --build --remove-orphans <name>` | um contêiner em execução | `RestartCount`, `StartedAt` |
| `command` | `restart` (obrigatório), cwd `current` | sempre ativo | nenhuma |

`health` é uma URL http(s) consultada com curl **no host**. Uma verificação passa com uma resposta 2xx/3xx em até 5 s.

Todo hook e comando recebe `ATLAS_RELEASE`, `ATLAS_RELEASE_DIR`, `ATLAS_PREVIOUS`, `ATLAS_SHA`, `ATLAS_TARGET`, `ATLAS_APP` e `ATLAS_DIR`.

O `atlas config check` valida a seção. Ele rejeita:
- uma lista `targets` ou `services` vazia
- `keep < 2`
- `command` sem `restart`
- `restart` ou `compose_file` no tipo errado
- nomes de serviço duplicados
- uma chave de `link` absoluta ou que contém `..`
- um valor de `link`, `dir` ou `pause_marker` relativo

A seção `deploy:` não pode ser substituída pelo `atlas.local.yaml`.

**Secrets nunca viajam com o atlas.** Guarde-os em arquivos compartilhados no host e referencie por `link`. O Atlas nunca envia, lê nem imprime arquivos de ambiente.

## Comandos

```text
atlas deploy [TARGET] [--dry-run] [--json] [--allow-dirty] [--skip-build] [--force]
                      [--allow-downgrade] [--allow-diverged] [--allow-unknown-base]
                      [--lock-wait SECS] [--no-follow] [-d DIR]
atlas deploy rollback [TARGET] [--to RELEASE_ID] [--json]
atlas deploy status   [TARGET] [--json]
atlas deploy logs     [TARGET] [RELEASE] [-f]
atlas deploy history  [TARGET] [-n 20] [--json]
```

Você pode omitir `TARGET` quando há exatamente um alvo.

- `--dry-run` faz o build e o empacotamento e depois imprime o id da release, o sha256 e o `plan.sh` renderizado. Não muda nada no host (só lê qual release está publicada).
- Uma árvore de trabalho suja é recusada, a menos que você passe `--allow-dirty`; nesse caso o id termina em `-dirty`.
- Se o sha do HEAD já é o atual e está saudável, o deploy é um `noop`. Use `--force` para publicar de novo mesmo assim.
- Um `HEAD` mais antigo que o commit publicado, ou divergente dele, é recusado, a menos que você passe `--allow-downgrade` ou `--allow-diverged`; um commit publicado que este repositório não conhece é recusado, a menos que você passe `--allow-unknown-base`. Veja [Releases nunca voltam por acidente](#releases-nunca-voltam-por-acidente).
- `--no-follow` imprime o id da release assim que o driver inicia e sai com 0.
- Ctrl-C durante o acompanhamento imprime `detached; deploy continues on host` e sai com 130. O host segue em frente.
- `rollback` troca para a release saudável mais nova antes de `current`, ou para `--to <id>`, e depois reinicia e passa todos os serviços pelo gate. Uma release marcada como `failed` é recusada.
- `status` sai com 0 só se a última fase é terminal, o lock está livre e todos os serviços estão saudáveis. Uma fase não terminal com o lock livre mostra `INTERRUPTED`.

### Códigos de saída

| Código | Significado |
|---|---|
| 0 | publicado, noop ou rollback concluído |
| 1 | falhou, e o `current` foi restaurado ou nunca mudou (`rolled_back: true` quando foi restaurado) |
| 2 | **crítico**: o próprio rollback ou a recuperação não está saudável e precisa de uma pessoa |
| 3 | bloqueado por outro deploy |
| 4 | não foi possível alcançar o host, ou o contato se perdeu durante o acompanhamento (o deploy continua no host) |
| 130 | desanexado com Ctrl-C (o deploy continua no host) |

`--json` imprime um objeto no stdout, e o log do host vai para o stderr:

```json
{"ok":true,"target":"prod","host":"prod","release":"20250101120000-abcdef0","sha":"abcdef0…","previous":"20241231090000-1234567","phase":"healthy","rolled_back":false,"reason":null,"duration_s":74}
```

### Releases nunca voltam por acidente

Antes do build, o `atlas deploy` lê o commit que o host está servindo (`current` → `releases/<id>/.atlas-release`) e o compara com o `HEAD` local. Isso pega o erro clássico de rodar o `atlas deploy` de um checkout velho, que publicaria um commit mais antigo sobre a produção (as migrações continuam aplicadas, então recursos cujo schema já rodou sumiriam).

| `HEAD` em relação ao commit publicado | Comportamento |
|---|---|
| mesmo commit | como antes: `noop`, ou uma nova publicação com `--force` |
| descende dele | segue, imprimindo `deploying <id> (<sha>) over <deployed-id> (<deployed-sha>): +N commits` |
| mais antigo (um downgrade) | recusado, com os dois shas e `git log --oneline HEAD..deployed`. `--allow-downgrade` o publica. Para voltar de propósito, use `atlas deploy rollback` |
| divergente | recusado, com o merge-base e a contagem de commits de cada lado. `--allow-diverged` o publica |
| commit publicado que não está neste repositório | o atlas tenta `git fetch origin <sha>`; se ainda faltar, o deploy é recusado. `--allow-unknown-base` o publica |
| nada publicado ainda | segue (primeiro deploy) |

`--dry-run` faz a mesma verificação (ele só lê do host). O `atlas deploy rollback` nunca passa por ela. Uma release marcada como `-dirty` é comparada pelo commit em que foi construída.

### Artefatos

Com caminhos em `artifact:`, o atlas monta o tarball com o `tar` local e deixa os metadados do macOS de fora: sem atributos estendidos (cabeçalhos pax `LIBARCHIVE.xattr.*`, que fazem o GNU tar do host imprimir um aviso por arquivo) e sem arquivos AppleDouble `._*`. Ele detecta o bsdtar (macOS) e o GNU tar e passa a cada um as flags próprias; qualquer outro `tar` não recebe nenhuma. O dono é fixado em `0/0`, o cabeçalho do gzip não leva nome nem timestamp e, com o GNU tar, os nomes são ordenados e os mtimes fixados no horário do commit, então o mesmo conteúdo gera o mesmo hash onde quer que seja construído (o bsdtar não consegue fixar mtimes). No host, o driver extrai com `--warning=no-unknown-keyword` quando o `tar` do host é GNU, o que silencia os avisos de artefatos construídos por versões antigas do atlas.

## Estrutura remota

```text
/opt/<app>/
  current -> releases/<id>
  releases/<id>/                 # imutável; .atlas-release = {"id","sha","ts","user","status"}
  releases/.<id>.partial/        # upload em andamento
  .atlas/lock                    # flock, segurado pelo driver em execução
  .atlas/lock.owner              # user@host pid run ts
  .atlas/state.json              # {"release","previous","phase","pid","ts","reason","run"}
  .atlas/deploys.jsonl           # trilha de auditoria, uma linha por deploy/rollback concluído
  .atlas/run/<id>/{driver.sh,plan.sh,log,exit}
```

Fases, em ordem: `preflight`, `uploaded`, `sealed`, `installing`, `migrating`, `pre_swap`, `swapped`, `post_swap`, `restarting`, `healthy`.

Uma falha até `pre_swap` inclusive apaga a nova release e deixa o `current` intocado. Uma falha de `swapped` em diante passa por `rolling_back` e termina em `rolled_back` (saída 1) ou `failed` (saída 2).

## Requisitos do host

O host precisa de Linux com bash, GNU coreutils, `flock` do util-linux, `tar`, `curl` e `sha256sum`. Dependendo dos tipos de serviço, você também precisa de `systemctl` (com o usuário ssh autorizado a reiniciar as units) ou `docker compose`.

O usuário ssh precisa de acesso de escrita em `dir`. O Atlas usa o `ssh` do sistema com `BatchMode=yes` e um socket `ControlPersist` de 120 s em `~/.ssh/atlas-%C`.

## Migrações

O `migrate` roda antes da troca, enquanto a release antiga ainda atende o tráfego, e um rollback **nunca** reverte o schema. Por isso toda migração deve ser só expand: adicione colunas ou tabelas, preencha os dados e mantenha as duas releases funcionando. Remova colunas em um deploy posterior, quando nenhuma release que as lê restar em `keep`. É o padrão expand/contract de Fowler.

## GitHub Actions

```yaml
name: deploy
on: { push: { branches: [main] } }
concurrency: { group: deploy-prod, cancel-in-progress: false }   # nunca cancele um deploy em andamento
jobs:
  deploy:
    runs-on: ubuntu-latest
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v4
      - name: ssh
        run: |
          mkdir -p ~/.ssh && chmod 700 ~/.ssh
          printf '%s\n' "${{ secrets.DEPLOY_SSH_KEY }}" > ~/.ssh/id_ed25519 && chmod 600 ~/.ssh/id_ed25519
          printf '%s\n' "${{ secrets.SSH_KNOWN_HOSTS }}" > ~/.ssh/known_hosts   # fixado; nunca use ssh-keyscan
          printf 'Host prod\n  HostName %s\n  User deploy\n' "${{ secrets.DEPLOY_HOST }}" > ~/.ssh/config
      - run: curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh   # as versões prontas são para macOS; em um runner Linux, compile o atlas a partir do código-fonte
      - run: ~/.atlas/bin/atlas deploy prod --json
```

Mesmo que o job seja cancelado ou estoure o tempo, o deploy termina no host. Confira com `atlas deploy status prod`.

## Exemplo: app web, worker e proxy reverso

Uma configuração realista de um único host: uma etapa de build, secrets compartilhados ligados a partir do host, uma migração só expand, uma configuração de proxy validada e recarregada depois da troca e três serviços systemd reiniciados em ordem.

```yaml
deploy:
  app: shop
  build: bun run build
  targets:
    prod:
      host: prod
      dir: /opt/shop
      link:
        .env: /opt/shop/shared/.env.production
      install: bun install --frozen-lockfile
      migrate: ./scripts/migrate.sh        # só expand; saída diferente de zero aborta antes da troca
      hooks:
        post_swap: >-
          caddy validate --config deploy/Caddyfile &&
          sudo cp deploy/Caddyfile /etc/caddy/Caddyfile &&
          sudo systemctl reload caddy
      pause_marker: /opt/shop/shared/.deploy-lock
      services:
        - { name: shop-api,    kind: systemd, health: "http://127.0.0.1:3000/health" }
        - { name: shop-web,    kind: systemd, health: "http://127.0.0.1:3001/" }
        - { name: shop-worker, kind: systemd }
```

## Testes

`cargo test -p atlas-cli --test deploy` exercita o ciclo de vida inteiro contra `host: local`: deploy, noop, force, rollback por falha, rollback manual, lock, recuperação de interrupção, árvore suja, `--no-follow` e as recusas de downgrade, divergência e base desconhecida. Ele precisa de Linux. No macOS, rode no docker:

```sh
docker run --rm -v "$PWD":/w -v atlas-target:/w/target -w /w rust:1 \
  bash -c 'git config --global --add safe.directory "*"; cargo test -p atlas-cli --test deploy'
```
