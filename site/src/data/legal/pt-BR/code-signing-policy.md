Assinatura de código gratuita fornecida pela [SignPath.io](https://about.signpath.io), certificado da [SignPath Foundation](https://signpath.org).

<h2 id="scope">Escopo</h2>

Os arquivos do Windows de uma versão: `atlas.exe`, `atlas-desktop.exe`, o instalador NSIS (`Atlas-<version>-x86_64-pc-windows-msvc-setup.exe`) e o `Atlas-Setup.exe`. Eles são gerados pelo GitHub Actions a partir da tag da versão ([`.github/workflows/release.yml`](https://github.com/codeatlasdev/atlas/blob/main/.github/workflows/release.yml)) e assinados pela SignPath depois que um mantenedor aprova o pedido de assinatura. Nada é assinado a partir da máquina de uma pessoa desenvolvedora. Os arquivos de macOS e Linux não fazem parte disso.

<h2 id="status">Situação</h2>

A assinatura está **aguardando a aprovação da Foundation**. Até a primeira versão assinada, os arquivos do Windows não têm assinatura, então o SmartScreen pode avisar "O Windows protegeu seu computador": clique em **Mais informações** e depois em **Executar assim mesmo**. Veja o [guia para Windows](/pt-br/docs/installation/#windows).

<h2 id="roles">Papéis da equipe</h2>

- **Quem faz commits e revisa:** os mantenedores do repositório, veja o [GOVERNANCE.md](https://github.com/codeatlasdev/atlas/blob/main/GOVERNANCE.md).
- **Aprovadores** (liberam um pedido de assinatura): os membros do time do GitHub [`atlas-maintainers`](https://github.com/orgs/codeatlasdev/teams/atlas-maintainers), hoje [@mvilacad](https://github.com/mvilacad) e [@luancoleto](https://github.com/luancoleto).

<h2 id="privacy">Privacidade</h2>

O Atlas não transfere dados para outros sistemas a menos que você peça. Ele não tem telemetria, analytics, relatórios de falhas nem contas. A única requisição que ele faz por conta própria é uma verificação de atualização que lê as versões públicas de `codeatlasdev/atlas` no GitHub (o Atlas Desktop tem a chave *Verificar atualizações automaticamente* em Configurações); instalar ou atualizar baixa a versão do GitHub. Nada é enviado à CodeAtlas nem à SignPath. Toda requisição está listada no [PRIVACY.md](https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md).
