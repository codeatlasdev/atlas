O Atlas é feito pela CodeAtlas LTDA. Ele é local primeiro: a CLI, o Atlas Desktop e o app de celular **Atlas: Dev Remote** não têm contas, telemetria, analytics, anúncios nem relatórios de falhas. Não vendemos nem compartilhamos dados, e não rastreamos você entre apps ou sites. Toda requisição de rede que o software faz está listada no [PRIVACY.md](https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md), ao lado do código que a faz.

<h2 id="mobile">O app de celular (Atlas: Dev Remote)</h2>

O app mostra e controla o Atlas que roda no seu próprio computador. Ele não tem conta e conversa só com os computadores que você pareia com ele.

- **No celular:** uma chave gerada no celular (ela nunca sai dele) e a lista de computadores pareados (nome, chave pública, endereços de rede), no armazenamento seguro do sistema. "Esquecer este computador" apaga um; desinstalar o app apaga o resto (o iOS pode manter itens do Keychain).
- **Entre o celular e o seu computador:** estado de projetos e serviços, logs, conversas com agentes, saída do terminal e o que você digita. Criptografado de ponta a ponta; só os dois dispositivos têm as chaves. A CodeAtlas nunca recebe nada disso.
- **Relays de conexão:** para encontrar o seu computador, e para repassar o tráfego quando uma conexão direta está bloqueada, o app usa os relays públicos da number 0 (os criadores do iroh). Um relay vê os endereços IP e as chaves públicas dos dois dispositivos e quantos dados passam, nunca o conteúdo.
- **Notificações:** para avisar que uma tarefa terminou ou precisa de você, o app recebe um token do dispositivo do Firebase Cloud Messaging (Google). O seu computador envia esse token e o tipo do evento (`task_done` ou `needs_you`) pelo `push.codeatlas.com.br`, operado pela CodeAtlas, até o Google. Sem texto de mensagem, nomes de arquivos nem prompts. O relay registra só o tipo do evento e o código do resultado. Desative as notificações nas configurações do sistema, ou defina `ATLAS_PUSH_RELAY=off` no computador.
- **Câmera:** usada só para ler o código de pareamento; nada é gravado nem enviado.
- **Face ID / biometria:** verificada pelo sistema antes de você digitar em um terminal; o app só fica sabendo sim ou não.

<h2 id="desktop">O Atlas e o Atlas Desktop</h2>

Eles consultam o GitHub para ver se há atualizações (uma requisição simples pela última versão pública; o Atlas Desktop tem uma chave para desativar a verificação automática) e baixam as versões do GitHub. Todo o resto fica no seu computador. Os comandos, terminais, páginas web e agentes de IA que você executa pelo Atlas fazem as próprias requisições de rede; elas são deles, não do Atlas.

<h2 id="site">Este site</h2>

Um site estático no Cloudflare Pages, sem scripts, fontes, anúncios nem conteúdo incorporado de terceiros. A Cloudflare mantém os logs de servidor padrão. Se você se inscrever para receber as notas de versão, guardamos o seu endereço de e-mail, o idioma da página e a data para avisar você por e-mail sobre as versões, e apagamos tudo isso quando você pedir.

<h2 id="rights">Seus dados, seus pedidos</h2>

Quase tudo fica nos seus próprios dispositivos, onde você pode apagar. Para qualquer coisa que guardamos (um endereço de e-mail para as notas de versão, as contagens do relay), ou para qualquer dúvida, escreva para [luan@codeatlas.com.br](mailto:luan@codeatlas.com.br). Crianças: o Atlas é uma ferramenta para desenvolvedores e não é voltado a crianças menores de 13 anos.

<h2 id="changes">Alterações</h2>

As alterações desta política são feitas às claras, no [histórico desta página](https://github.com/codeatlasdev/atlas/commits/main/site/src/data/legal/pt-BR/privacy.md). Última atualização: 12 de novembro de 2026.
