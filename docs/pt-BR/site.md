---
title: "O site"
source: site.md
source_sha: "ff021a471ba3f5ce52acb3e920874d006084039d"
translation: machine
---

# O site

O `site/` é um site estático em Astro 7 + Svelte 5, servido em https://atlas.codeatlas.com.br. Ele renderiza `docs/*.md` e o `CHANGELOG.md` diretamente. Veja o [site/README.md](https://github.com/codeatlasdev/atlas/blob/main/site/README.md) para a organização dos arquivos e as regras de movimento.

```bash
cd site
bun install
../scripts/fetch-media.sh               # uma vez só: o filme e os loops não estão no git (veja abaixo)
bun run dev                            # http://localhost:4321
bun run build                          # site estático em site/dist
```

## A mídia não está no git

O filme, os loops e as imagens estáticas maiores (`site/public/media/*.mp4`, `*.webm` e `demo.gif`) somam cerca de 28 MB e existem só no site publicado. O `scripts/fetch-media.sh` baixa do site no ar o que estiver faltando para `site/public/media/`; ele é idempotente e pula os arquivos que você já tem. Os pôsteres e as imagens estáticas ficam no git (todos com cerca de 330 KB ou menos). A mídia é gerada a partir de código pelo studio em `studio/` (`bun studio render` / `publish`; veja [Contribuir](contributing.md#fazer-vídeos)).

Rode o `scripts/fetch-media.sh` antes de `bun run build` ou `bun run site:deploy` em qualquer máquina que nunca tenha compilado o site, senão o site publicado iria sem o filme.

## Restrições de design

- A Content-Security-Policy em `site/public/_headers` só permite recursos de mesma origem: sem fontes, scripts, analytics ou embeds externos. Mantenha tudo hospedado por nós.
- O Cloudflare Pages concatena as regras `_headers` que casam, então um header definido em `/*` não pode ser sobrescrito em um subcaminho; defina `Cache-Control` só em caminhos específicos.
- `prefers-reduced-motion`: sem rolagem suave, sem ScrollTrigger, sem autoplay de vídeo. Sem JavaScript, a página é HTML estático completo.

## Deploy (mantenedores)

O site é um projeto do Cloudflare Pages. Os deploys rodam localmente, como as versões:

```bash
export CLOUDFLARE_API_TOKEN=…     # permissões de edição do Pages e do D1
export CLOUDFLARE_ACCOUNT_ID=…
bun run site:deploy                # build + wrangler pages deploy site/dist
scripts/deploy-site.sh --branch x  # deploy de preview
```

O `site:deploy` envia o `site/dist`, que é gerado a partir de `site/public/`, então a mídia precisa estar presente localmente (rode antes o `scripts/fetch-media.sh`).

### Cadastro para notas de versão (`/api/waitlist`)

O `site/functions/api/waitlist.ts` (`POST /api/waitlist`) armazena e-mails em um banco D1 ligado como `DB` (veja `site/wrangler.toml`; o schema está em `site/functions/schema.sql`). Ele aceita só requisições de mesma origem, valida o e-mail e o converte para minúsculas, ignora um campo honeypot escondido, limita a taxa por hash com salt do IP do cliente (IPs brutos nunca são armazenados nem registrados em log) e nunca registra dados da requisição em log. O site o apresenta como "Receber as notas de versão por e-mail"; o nome do endpoint e o contrato não mudam, e há um `topic` opcional (`windows`, `linux`, `macos-intel`, `mobile-link`; qualquer outro valor é descartado) que a página de download envia para que um cadastro registre a plataforma que a pessoa está esperando. Um novo cadastro com um topic o acrescenta à lista `topics` armazenada. Os bancos existentes precisam rodar `ALTER TABLE waitlist ADD COLUMN topics TEXT NOT NULL DEFAULT ''` uma vez (também em `site/functions/schema.sql`) **antes** de fazer o deploy desta função.

### Página de download

### O motor de versões (o site nunca fica atrás do app)

`/download/` e `/changelog/` não dependem do último deploy do site. As Cloudflare Pages Functions (`site/functions/`) resolvem a **última versão publicada na borda**:

| Endpoint | O que serve |
|---|---|
| `/release.json[?channel=stable\|beta\|alpha]` | tag, versão, data, assets do `.dmg` e da CLI com sha256 e tamanho (do `manifest.json` da release), mais `channel` e `prerelease`. Padrão: o canal padrão (`alpha` enquanto o Atlas está em alpha; `DEFAULT_CHANNEL` em `site/functions/_lib/channels.ts`, fixado ao `#[default]` do atlas-core por um teste); `?channel=stable` é a tag mais nova sem rótulo de pré-lançamento; qualquer outro valor devolve `400 {"error":"invalid_channel","valid":["stable","beta","alpha"]}`. O `/download/` hidrata o botão principal com ele, então o botão sempre aponta para o `.dmg` mais novo do canal padrão (o alpha mais novo, ou o do Stable quando nada é mais novo); Stable, Beta e os outros canais são cards abaixo dele |
| `/channels.json` | `{ stage, stage_note, channels: [{ id, label, promise, risk, risk_label, cadence, audience, switch_to, release }] }`: cada [canal de release](versioning.md) com o texto dele e o que oferece agora (`release` é um corpo de `/release.json` sem os campos do canal, ou `null`). O texto vem de `crates/atlas-core/data/channels.json`, o único arquivo que a CLI e o Atlas Desktop também usam. O `/download/` hidrata com ele os cards de "Outros canais" (todo canal, exceto o padrão, que é o download principal). Um canal diferente do Stable sem nada mais novo que o Stable devolve a própria versão do Stable, então a página mostra "Ainda não há versão Beta" quando `release.tag` é igual à tag do Stable |
| `/changelog/` | o shell estático da página com o corpo trocado pelo changelog **da versão mais nova do canal padrão** (`CHANGELOG.md` lido de `raw.githubusercontent.com` naquela tag), uma `<section id="v1-2-0">` por versão mais uma lista de saltos entre versões. O `[Unreleased]` nunca é renderizado. As versões de pré-lançamento ganham um selo Alpha / Beta / RC e um filtro (Todas / Só estáveis / Pré-lançamentos, `?show=stable\|pre`) que funciona no cliente; "Só estáveis" esconde um pré-lançamento cuja versão estável já está no changelog. `?channel=stable\|beta\|alpha` mostra o histórico na tag mais nova desse canal |
| `/changelog.json[?channel=…]` | as mesmas entradas como dados: `{ tag, entries: [{ version, date, id, html, kind, superseded }] }`; `kind` é `stable`, `alpha`, `beta`, `rc` ou `pre`, e `superseded` marca um pré-lançamento cuja versão estável está listada. As entradas seguem a precedência do SemVer (`alpha.10` é mais nova que `alpha.9`) |

Canais (`functions/_lib/channels.ts`, um port em TypeScript do `atlas_core::channel`; os dois lados são fixados pelos vetores compartilhados em `testdata/channels/resolve.json`): o Stable é a tag mais nova sem rótulo de pré-lançamento entre as de `releases.atom` (páginas mais antigas com `?after=<tag>`, no máximo 5, quando uma sequência de alphas preenche a janela de 10 entradas) e o redirect de `releases/latest`, que é só um candidato porque é o alpha mais novo enquanto o Alpha é o padrão; uma requisição sem `?channel=` resolve o canal padrão (`DEFAULT_CHANNEL`). O Beta e o Alpha leem `github.com/codeatlasdev/atlas/releases.atom` (as 10 versões publicadas mais novas, sem API, com cache de 60 segundos e o mesmo fallback para a última cópia boa), acrescentam a tag do Stable como candidata, pegam a maior precedência SemVer entre as tags que o canal aceita e só oferecem uma tag depois que o `manifest.json` dela existe (a tag é enviada antes de a versão ser compilada, então o resolvedor desce pulando as não publicadas). Se o feed não puder ser lido, o Beta e o Alpha caem para o Stable em vez de falhar. A última cópia boa é mantida por canal.

Como ele se mantém atual e no ar (`functions/_lib/edge.ts`): o redirect de `github.com/codeatlasdev/atlas/releases/latest` (sem cota de API, sem token; o build mais novo do canal padrão, só como dica) e o feed ficam em cache por 60 segundos, então uma versão nova aparece em cerca de um minuto; tudo o que depende de uma tag fica em cache por tag. As respostas levam `Cache-Control: public, max-age=60, s-maxage=60, stale-while-revalidate=86400, stale-if-error=604800`. Se o GitHub estiver fora do ar, a última cópia boa é servida (header `x-atlas-source: stale-last-good`, e o changelog avisa); sem nada em cache, a página volta ao corpo gerado no último deploy. Acrescente `?fresh=1` a qualquer endpoint para pular o cache de 60 segundos das tags (limpeza manual do cache). As partes puras (parser e renderizador do changelog, mapeamento do manifest, regras de indisponibilidade e de atualidade) têm testes unitários: `bun test functions` em `site/`, parte do `bun run check`.

O `scripts/release.sh` também roda `bun run site:deploy` depois de publicar (pulado com um aviso quando as variáveis do Cloudflare faltam; um deploy que falha nunca faz a versão falhar), o que atualiza o fallback gerado e as docs. Veja o [PRIVACY.md](https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md).
