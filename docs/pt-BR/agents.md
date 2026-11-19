---
title: "Configuração dos agentes"
source: agents.md
source_sha: "a198547c73ecbf760c3a58e66e52f86b819544e6"
translation: machine
---

# Configuração dos agentes

O `atlas setup` liga o Atlas ao seu agente de código com IA em um comando. Quatro agentes são compatíveis: [OMP](https://github.com/can1357/oh-my-pi), [Claude Code](https://code.claude.com), [Codex](https://developers.openai.com/codex) e [OpenCode](https://opencode.ai) (`atlas setup opencode`: um plugin global para eventos de status e aprovação remota, só no escopo de usuário; as tabelas abaixo cobrem os três primeiros). Todos usam a mesma mecânica de plano e aplicação, as mesmas flags e as mesmas regras de propriedade.

```bash
atlas setup                          # detecta omp / claude / codex no PATH, mostra o status e os comandos para rodar
atlas setup claude-code              # SIMULAÇÃO: imprime o plano, não muda nada
atlas setup claude-code --apply      # aplica
atlas setup claude-code --apply      # de novo: tudo aparece como "em dia"
```

Troque `claude-code` por `omp` ou `codex`. O padrão é sempre a simulação (dry run). Cada linha do plano mostra o artefato, o caminho, a ação (`criar`, `atualizar`, `em dia`, `CONFLITO`, `remover`, `ignorado`) e um resumo curto da mudança.

| Artefato | OMP | Claude Code | Codex |
|----------|-----|-------------|-------|
| Servidor MCP `atlas` | plugin `~/.atlas/omp-plugin/.mcp.json` | `~/.claude.json` / `.mcp.json` | `config.toml` `[mcp_servers.atlas]` |
| Skills | plugin `skills/` (+ `~/.agents/skills` para os outros agentes) | `~/.claude/skills` (o Claude Code não lê `~/.agents/skills`) | `~/.agents/skills` **compartilhado** (o Codex lê; é a mesma cópia do OMP) |
| Hooks | plugin `hooks/pre/atlas.ts` | `settings.json` → `atlas hook claude-code …` | `hooks.json` → `atlas hook codex …` (confie uma vez em `/hooks`) |
| `--scope project` | nenhum: plugins do omp são do usuário | `<project>/.mcp.json` + `<project>/.claude/settings.json` | `<project>/.codex/` (só projetos confiáveis) |

Flags comuns (nos três): `--scope user|project`, `--apply`, `--force`, `--uninstall`.

| Flag | Significado |
|------|---------|
| `--scope user` (padrão) | Grava a entrada MCP e os hooks para todos os projetos (a configuração de usuário do agente). |
| `--scope project` | Grava no projeto. A raiz do projeto é o ancestral mais próximo do diretório atual que tem um `atlas.yaml`; sem ele, dá erro. As skills sempre ficam no escopo de usuário. |
| `--apply` | Executa o plano. Sem ela, nada é gravado. |
| `--force` | Substitui uma entrada MCP `atlas` ou um arquivo de hook que o Atlas não criou. |
| `--uninstall` | Remove só o que é do Atlas (simulação, a menos que use `--apply`). |

## O que os hooks fazem

O OMP roda uma pequena extensão (dentro do plugin do Atlas); o Claude Code e o Codex rodam o subcomando embutido `atlas hook <agent> <event>` (JSON do evento no stdin, resposta JSON do agente no stdout). Os três se comportam do mesmo jeito e só leem arquivos locais:

1. **Início de sessão** dentro de um projeto Atlas (um `atlas.yaml` no diretório de trabalho ou em um pai): injeta a saída de `atlas dev --status`. Se a stack não está rodando, diz ao agente para pedir que você a inicie e nunca rodar `atlas dev` por conta própria.
2. **Antes de um comando de shell**: nega comandos que iniciariam a TUI interativa. São eles `atlas dev` sem `--status`, `--ci` ou `--headless`, e `bun run dev` / `npm run dev` / `pnpm dev` / `yarn dev` em um projeto cujo script `dev` do `package.json` é `atlas dev`. O motivo aponta o agente para as ferramentas MCP e para `atlas dev --status`.

3. **Estado do agente para o Atlas Desktop**: quando o agente roda em um terminal do Atlas Desktop (`ATLAS_HOOK_ENDPOINT` definido), os eventos de prompt, ferramenta, parada e aprovação são enviados ao servidor de loopback do `atlas hostd` (ele continua rodando com o app fechado; só em 127.0.0.1) para mostrar trabalhando / precisa de você / concluído. O omp usa a extensão `atlas-status`; o Claude Code e o Codex recebem hooks extras para `UserPromptSubmit`, `PostToolUse`, `Stop`, `PermissionRequest` (nos dois; ele pode esperar até 55 s por uma resposta do Atlas Mobile e não responde nada quando nenhum celular está conectado), além de `Notification`, `StopFailure`, `SessionEnd` (Claude). Fora do Atlas Desktop, eles não fazem nada. O Codex roda hooks no daemon compartilhado dele, em segundo plano e sem o ambiente do terminal; para esses eventos, o `atlas hook` usa `~/.atlas/desktop/hook-endpoint.env` e envia o `cwd`, e o hostd o associa à aba do Codex que ele abriu naquela worktree (eventos de outros diretórios são ignorados).

O `atlas hook` fica escondido no `--help` e quem o roda é o agente, não você. Qualquer problema (entrada ruim, não é um projeto Atlas, ferramenta desconhecida) não produz saída e termina com código 0, ou seja, "sem opinião". O comando de hook registrado no Claude Code e no Codex é `atlas hook <agent> <event> 2>/dev/null || true`: os dois agentes tratam o código de saída 2 como um erro *bloqueante*, e o `|| true` garante que um `atlas` mais antigo no `PATH`, sem o subcomando `hook`, ou nenhum `atlas`, nunca bloqueie as suas chamadas de ferramenta.

Teste na mão, em simulação:

```bash
echo '{"tool_name":"Bash","tool_input":{"command":"atlas dev"},"cwd":"'"$PWD"'"}' | atlas hook claude-code pre-tool-use
echo '{"cwd":"'"$PWD"'"}' | atlas hook codex session-start
```

## `atlas setup omp`

```text
atlas setup omp [--scope user|project] [--apply] [--force] [--uninstall]
```

Tudo para o omp é um plugin só, `~/.atlas/omp-plugin/`, ligado ao omp com `omp plugin link`. Plugins do omp são do usuário, então `--scope project` não grava nada para o omp: só remove arquivos antigos de projeto (a entrada `<project>/.omp/mcp.json` e o `<project>/.omp/hooks/pre/atlas.ts`) deixados por versões antigas do Atlas, e imprime um aviso.

### O que ele instala

| Artefato | Caminho |
|----------|------|
| Manifesto do plugin | `~/.atlas/omp-plugin/package.json` (`name: atlas-omp`) |
| Servidor MCP `atlas` | `~/.atlas/omp-plugin/.mcp.json` |
| Hook | `~/.atlas/omp-plugin/hooks/pre/atlas.ts` |
| Regra (TTSR) | `~/.atlas/omp-plugin/rules/atlas-no-dev-tui.md` |
| Subagente `atlas-ops` | `~/.atlas/omp-plugin/agents/atlas-ops.md` |
| Skills | `~/.atlas/omp-plugin/skills/<name>/` e `~/.agents/skills/<name>/` |
| Link | `omp plugin link ~/.atlas/omp-plugin` (estado em `omp plugin list`) |

**Plugin.** O Atlas é dono do diretório do plugin e o reescreve a cada `--apply` e `atlas update`; nunca edite arquivos dentro dele. A etapa `Link do plugin` roda `omp plugin link` quando o plugin não está registrado, `omp plugin enable atlas-omp` quando está registrado mas desativado, e nada quando já está ativado. Se o `omp` não está no `PATH`, a etapa de link fica `ignorado`, os arquivos são gravados do mesmo jeito, e o plano imprime o comando para você rodar.

**Servidor MCP.** O `.mcp.json` declara `{"type":"stdio","command":"atlas","args":["mcp"]}` em `mcpServers.atlas`. Se o `atlas` não está no `PATH`, o caminho absoluto do binário em execução é gravado no lugar, e o plano avisa. Veja [MCP](mcp.md) para as ferramentas, os recursos e os prompts (`/atlas:status`, `/atlas:diagnose <service>`, `/atlas:add-service`).

**Skills.** As skills `atlas`, `atlas-desktop` e `atlas-install` vêm compiladas no binário, então sempre combinam com a versão que você roda; nada é baixado do GitHub. O omp as carrega do plugin (e prefere essa cópia); o mesmo conjunto fica em `~/.agents/skills/` para o Claude Code e o Codex. A propriedade da cópia compartilhada fica registrada em `~/.atlas/.atlas-skills.json`. `atlas skills list|status|install|update` trabalham sobre a mesma cópia embutida.

**Regra.** `atlas-no-dev-tui` é uma regra TTSR: quando o modelo começa a gerar um comando bash que roda `atlas dev` (exceto `--status`, `--ci`, `--headless`, `--help`), o omp interrompe a geração e mostra o lembrete para usar as ferramentas MCP.

**Subagente.** O `atlas-ops` diagnostica e corrige um serviço com problema pelas ferramentas MCP (status, logs, correção, reinício, espera até ficar pronto) e nunca inicia nem para a stack inteira.

**Hook.** Uma pequena extensão do OMP que nunca lança exceção, nunca toca a rede e dá 2 segundos a cada comando:

1. No primeiro turno do agente em uma sessão dentro de um projeto Atlas (um `atlas.yaml` no diretório de trabalho ou em um pai), ela injeta a saída de `atlas dev --status`. Se a stack não está rodando, diz ao agente para pedir que você a inicie e nunca rodar `atlas dev` por conta própria. Isso acontece uma vez por sessão.
2. Ela bloqueia chamadas de ferramenta que iniciariam a TUI interativa: comandos bash que rodam `atlas dev` sem `--status`, `--ci` ou `--headless`, `bun run dev` / `npm run dev` / `pnpm dev` / `yarn dev` em um projeto cujo script `dev` do `package.json` é `atlas dev`, e os mesmos comandos iniciados pela ferramenta `hub` (`op: start`, `application` + `args`). O motivo do bloqueio aponta o agente para as ferramentas MCP e para `atlas dev --status`.

A primeira linha do hook é um comentário com `managed by atlas setup` e a versão do Atlas. Não edite o arquivo: o `atlas setup omp --apply` o reescreve quando o conteúdo difere, e o `atlas update` o atualiza para você. Reinicie as sessões do omp para carregar um hook novo (`/reload-plugins` atualiza só skills e MCP).

### Propriedade e conflitos

O Atlas só mexe no que ele criou:

- Uma entrada MCP `atlas` é "própria" quando é um servidor stdio que roda `atlas mcp` (`atlas` simples ou qualquer caminho terminado em `/atlas`). O diretório do plugin é inteiramente do Atlas e nunca gera conflito.
- Migração de versões antigas: uma entrada própria `mcpServers.atlas` em `~/.omp/agent/mcp.json` e um hook próprio `~/.omp/agent/hooks/pre/atlas.ts` (cabeçalho `managed by atlas setup`) são removidos no `--apply`, para que o hook e o servidor não carreguem duas vezes. Entradas ou arquivos de hook `atlas` de terceiros ficam como estão.
- O `--uninstall` ignora entradas e arquivos de terceiros e nunca apaga skills que não estão no manifesto do Atlas nem os seus outros servidores MCP. Uma entrada antiga de terceiros é relatada como conflito (`--force` a remove).
- JSON inválido em um `mcp.json` antigo existente é erro para o `--uninstall`; o Atlas nunca sobrescreve um arquivo que não consegue interpretar.

### Depois de aplicar

1. Reinicie o omp: o hook carrega no início da sessão. Em uma sessão em andamento, `/reload-plugins` atualiza só skills e MCP.
2. Rode `/mcp list`: o `atlas` deve aparecer na lista (vindo do plugin). O `omp plugin list` mostra `atlas-omp`.
3. Abra o omp dentro de um projeto Atlas e pergunte: *"por que o api está falhando?"*, ou rode `/atlas:diagnose api`.

### Desinstalar

```bash
atlas setup omp --uninstall           # prévia
atlas setup omp --uninstall --apply   # desvincula o plugin, apaga ~/.atlas/omp-plugin, remove as skills embutidas e qualquer arquivo atlas antigo
atlas setup omp --scope project --uninstall --apply   # o mesmo, mais os arquivos antigos em <project>/.omp/
```

### Ambiente

| Variável | Efeito |
|----------|--------|
| `PI_CODING_AGENT_DIR` | Diretório do agente do OMP; usado só para achar e migrar o `mcp.json` e o hook antigos (a mesma variável que o OMP usa). |
| `CLAUDE_CONFIG_DIR` | Diretório de configuração do Claude Code; substitui `~/.claude` (e guarda o `.claude.json`). |
| `CODEX_HOME` | Home do Codex; substitui `~/.codex`. |
| `HOME` | Base de `~/.atlas/omp-plugin`, `~/.agents/skills`, `~/.claude.json` e `~/.atlas/.atlas-skills.json` (o omp guarda o registro de plugins em `~/.omp/plugins`). |

Para testar qualquer um deles sem tocar na sua configuração real, aponte as variáveis para um diretório temporário:

```bash
T=$(mktemp -d)
HOME=$T/home PI_CODING_AGENT_DIR=$T/omp CLAUDE_CONFIG_DIR=$T/claude CODEX_HOME=$T/codex atlas setup claude-code --apply
```

O próprio `omp` lê credenciais do seu `HOME` e do diretório do agente reais; para rodar um `omp` de verdade contra um sandbox assim, mantenha o `HOME` real e sobrescreva só o `PI_CODING_AGENT_DIR` (copie `agent.db` e `config.yml` para ele, para a autenticação).

## `atlas setup claude-code`

```text
atlas setup claude-code [--scope user|project] [--apply] [--force] [--uninstall]
```

| Artefato | Escopo de usuário | Escopo de projeto |
|----------|-----------|---------------|
| Servidor MCP `atlas` | `~/.claude.json` | `<project>/.mcp.json` |
| Hooks | `~/.claude/settings.json` | `<project>/.claude/settings.json` |
| Skills | `~/.claude/skills/<name>/` | o mesmo (sempre escopo de usuário) |

**Servidor MCP.** Adiciona `{"type":"stdio","command":"atlas","args":["mcp"],"env":{}}` em `mcpServers.atlas`, a mesma entrada que `claude mcp add --scope user atlas -- atlas mcp` grava, então entradas criadas por qualquer uma das duas ferramentas são intercambiáveis. O `~/.claude.json` é o arquivo de estado do próprio Claude Code (login, estado por projeto): o Atlas mexe só em `mcpServers.atlas`, mantém todas as outras chaves e a ordem delas, e grava de forma atômica. O Claude Code reescreve esse arquivo sozinho enquanto roda, então aplique com o Claude Code fechado se quiser ter certeza de que nenhuma gravação se perde. Servidores do `.mcp.json` no escopo de projeto pedem a sua aprovação na primeira vez que o Claude Code os carrega (`claude mcp reset-project-choices` redefine isso).

**Skills.** O Claude Code lê `~/.claude/skills` e não lê `~/.agents/skills`, então as skills embutidas são gravadas lá (a propriedade fica em `~/.claude/.atlas-skills.json`). As suas próprias skills nesse diretório nunca são tocadas.

**Hooks.** Duas entradas no `settings.json`: `hooks.SessionStart` (injeção de contexto) e `hooks.PreToolUse` com o matcher `Bash` (a proteção). Os seus outros hooks e configurações ficam, na ordem original. Um handler é do Atlas quando o comando dele é exatamente `<atlas> hook claude-code <event> 2>/dev/null || true`; se o `atlas` não está no `PATH`, o caminho absoluto é usado (entre aspas), e o `--apply` o reescreve quando o caminho muda. Fazer commit de `.claude/settings.json` e `.mcp.json` (escopo de projeto) compartilha a configuração com a equipe; quem usa precisa ter o `atlas` instalado.

Ambiente: `CLAUDE_CONFIG_DIR` muda o diretório de configuração (`settings.json`, `skills/`) e, como o próprio Claude Code faz, o `.claude.json` vai para dentro dele.

Depois de aplicar: reinicie o Claude Code e rode `claude mcp get atlas` (deve mostrar `✔ Connected`) ou `/mcp`.

Documentação usada: [escopos do MCP](https://code.claude.com/docs/en/mcp#mcp-installation-scopes), [skills](https://code.claude.com/docs/en/skills), [hooks](https://code.claude.com/docs/en/hooks).

## `atlas setup codex`

```text
atlas setup codex [--scope user|project] [--apply] [--force] [--uninstall]
```

| Artefato | Escopo de usuário | Escopo de projeto |
|----------|-----------|---------------|
| Servidor MCP `atlas` | `~/.codex/config.toml` | `<project>/.codex/config.toml` |
| Hooks | `~/.codex/hooks.json` | `<project>/.codex/hooks.json` |
| Skills | `~/.agents/skills/<name>/` **compartilhado** | o mesmo (sempre escopo de usuário) |

**Servidor MCP.** Adiciona `[mcp_servers.atlas]` com `command = "atlas"` e `args = ["mcp"]` (o que `codex mcp add atlas -- atlas mcp` grava). O `config.toml` é a sua configuração principal do Codex, então ele é editado preservando o formato: comentários, ordem e espaços ficam intactos, a nova tabela entra no fim, e o uninstall restaura o texto original. Uma tabela atlas existente que você ajustou (`startup_timeout_sec`, `enabled`, filtros de ferramentas, …) mantém essas chaves quando só o caminho do comando muda. Outras grafias do mesmo servidor (tabela inline, chaves com ponto, chaves entre aspas) também são reconhecidas. O Atlas usa o crate `toml_edit` para isso: um leitor de linhas escrito à mão não consegue distinguir com segurança o cabeçalho de uma tabela da continuação de uma lista ou string de várias linhas, e esse arquivo guarda configurações próximas das suas credenciais. As camadas `.codex/` no escopo de projeto só carregam para projetos que você marcou como confiáveis no Codex.

**Skills (compartilhadas).** O Codex lê `~/.agents/skills`, o diretório que o OMP usa. O setup garante que as skills embutidas estejam lá (nada acontece se o OMP ou o `atlas skills install` já as colocou), e o `--uninstall` as deixa no lugar enquanto o outro agente (OMP) ainda está configurado, então remover um agente nunca quebra o outro.

**Hooks.** O `hooks.json`, ao lado da configuração, recebe entradas `SessionStart` e `PreToolUse` (matcher `Bash`) que rodam `atlas hook codex <event>`. **O Codex só roda um hook depois que você o revisa e confia nele**: depois do `--apply`, inicie o Codex e rode `/hooks` uma vez para confiar nos dois hooks do atlas. Quando um `atlas update` posterior muda um hook, o Codex o marca para revisão de novo. Hooks não revisados são ignorados em silêncio, exceto por um aviso na inicialização. Não mantenha hooks do atlas ao mesmo tempo no `hooks.json` e em `[hooks]` inline na mesma camada (o Codex avisa); o Atlas sempre usa o `hooks.json`.

Ambiente: `CODEX_HOME` substitui `~/.codex`.

Depois de aplicar: reinicie o Codex, confie nos hooks em `/hooks` e rode `codex mcp get atlas`.

Documentação usada: [MCP](https://developers.openai.com/codex/mcp), [skills](https://developers.openai.com/codex/skills), [hooks](https://developers.openai.com/codex/hooks), [configuração avançada](https://developers.openai.com/codex/config-advanced).

## Solução de problemas

| Sintoma | Correção |
|---------|-----|
| `/mcp list` não mostra o `atlas` | Veja se o `omp plugin list` mostra o `atlas-omp` ativado; se não, rode `atlas setup omp --apply` (ou `omp plugin link ~/.atlas/omp-plugin`) e depois `/reload-plugins` ou reinicie o omp. |
| ``CONFLITO … existe uma entrada `atlas` que não foi criada pelo atlas`` | Claude Code / Codex: você tem uma entrada de servidor `atlas` própria. Renomeie, ou rode de novo com `--force` para substituí-la. |
| O servidor MCP não inicia | O `atlas` não está no `PATH` do omp. Rode `atlas setup omp --apply` de novo em um shell onde `which atlas` funcione (o `.mcp.json` do plugin passa a receber o caminho absoluto) ou corrija o `PATH` (`~/.atlas/bin`). |
| O agente ainda roda `atlas dev` | O hook carrega no início da sessão: confira `ls ~/.atlas/omp-plugin/hooks/pre/atlas.ts` e `omp plugin list`, e reinicie o omp. |
| `ignorado  Link do plugin  … omp não encontrado no PATH` | Instale o omp e rode o `omp plugin link <dir>` impresso (ou rode `atlas setup omp --apply` de novo). |
| Nenhuma mensagem de status no início da sessão | O hook procura o `atlas.yaml` a partir do diretório de trabalho da sessão, subindo. |
| `erro: --scope project precisa de um atlas.yaml em <dir> ou em uma pasta acima` | Rode de dentro do projeto. |
| Faltam skills no omp / Codex | `atlas skills status` mostra o que está instalado em `~/.agents/skills`. |
| Faltam skills no Claude Code | Elas ficam em `~/.claude/skills` (ou `$CLAUDE_CONFIG_DIR/skills`), não em `~/.agents/skills`. |
| `claude mcp get atlas` não mostra Connected | O `atlas` não está no `PATH` do Claude Code: rode `atlas setup claude-code --apply` de novo em um shell onde `which atlas` funcione. |
| O Claude Code ignora a entrada do `.mcp.json` do projeto | Aprove no prompt interativo, ou rode `claude mcp reset-project-choices`. |
| O Codex não roda os hooks do atlas | Confie neles uma vez: `/hooks` no Codex. Eles são ignorados até serem revisados. |
| O Codex ignora `<project>/.codex/` | O projeto precisa ser confiável no Codex (`projects.<path>.trust_level = "trusted"`). |

## Outros agentes

O Cursor não é configurado pelo `atlas setup`. A página [MCP](mcp.md) documenta a entrada manual. Agentes futuros chegam como novas variantes de `atlas setup <agent>` que reaproveitam as mesmas regras de simulação, mesclagem e propriedade; nenhuma data é prometida.

## Agentes personalizados no Atlas Desktop

Configurações → Agentes → **Agentes personalizados** roda qualquer CLI de agente (OpenCode, Gemini CLI, Amp, Aider…) como os embutidos: ele aparece no menu + de uma barra de abas, pode ser o agente padrão (⌘⇧A) e mostra o estado na aba e na barra lateral. O formulário pede um nome, o comando (programa mais argumentos fixos, com aspas como em um shell), argumentos extras, argumentos de retomada (`{session}` é trocado pelo id da sessão) e ambiente (`KEY=value` por linha; `ATLAS_*` é reservado). As entradas ficam em `~/.atlas/desktop/settings.json`, em `customAgents`:

```json
{ "customAgents": [
  { "id": "opencode", "label": "OpenCode", "command": "opencode",
    "args": ["--model", "gpt-5"], "resume": ["--session", "{session}"], "env": { "FOO": "1" } }
] }
```

Uma entrada cujo `id` não é um slug (`a-z`, `0-9`, `-`, no máximo 32), que colide com um agente embutido ou com uma entrada anterior, ou que não tem `label` ou `command`, é ignorada. Um agente personalizado nunca recebe as flags de aprovação automática dos agentes embutidos; coloque essas flags nos argumentos dele.

**Estado sem hooks.** A aba de um agente personalizado mostra *trabalhando* enquanto o terminal imprime (o `pty_activity` do hostd, no máximo um por segundo) e *ocioso* depois de 3 segundos de silêncio. O que você digita e o programa ecoa conta como saída. Estados reais (precisa de você, concluído) exigem o contrato de hooks abaixo.

### Contrato de eventos dos hooks

Todo terminal de agente que o Atlas Desktop inicia tem estas variáveis:

| Variável | Valor |
|---|---|
| `ATLAS_HOOK_ENDPOINT` | Caminho de um arquivo com as linhas `ATLAS_HOOK_PORT=<port>` e `ATLAS_HOOK_TOKEN=<token>` (`~/.atlas/desktop/hook-endpoint.env`, modo 0600). |
| `ATLAS_PANE_KEY`, `ATLAS_TAB_ID` | A aba em que o agente roda. |
| `ATLAS_WORKTREE_ID` | A worktree. |
| `ATLAS_AGENT` | O id do agente (`omp`, `claude`, `codex` ou o id personalizado). |

Para informar um estado, faça POST em `http://127.0.0.1:<port>/hook/<agent id>` com o cabeçalho `X-Atlas-Hook-Token: <token>` e um corpo JSON de até 1 MB:

```json
{ "pane_key": "<ATLAS_PANE_KEY>", "tab_id": "<ATLAS_TAB_ID>", "worktree_id": "<ATLAS_WORKTREE_ID>",
  "payload": { "event": "user_prompt_submit", "session_id": "…", "prompt": "…" } }
```

A resposta é `204` (aceito), `401` (token errado), `413` (corpo grande demais), `400` (não é JSON), `404` (caminho) ou `405` (não é POST). Sem `pane_key`, o app associa a aba pelo agente e por `payload.cwd`. Os eventos e o estado que cada um define:

| `payload.event` | Estado |
|---|---|
| `user_prompt_submit`, `pre_tool_use`, `post_tool_use`, `agent_start`, `message_end` | trabalhando |
| `permission_request`, `tool_approval_requested` | precisa de você (bloqueado) |
| `stop`, `agent_end` | concluído |
| `session_end`, `session_shutdown` | ocioso |
| `session_start` | sem mudança (registra `session_id` / `session_file`) |

Campos opcionais do payload: `session_id`, `session_file`, `prompt`, `tool_name`, `text` (última mensagem), `cwd`. Eventos de hook têm prioridade sobre a atividade do terminal por 5 segundos depois de cada um. Mantenha a chamada curta (os hooks do próprio Atlas usam timeout de 1 s) e ignore falhas: fora do Atlas Desktop, `ATLAS_HOOK_ENDPOINT` não está definido.

```sh
. "$ATLAS_HOOK_ENDPOINT"
curl -s -m 1 -X POST "http://127.0.0.1:$ATLAS_HOOK_PORT/hook/$ATLAS_AGENT" \
  -H "X-Atlas-Hook-Token: $ATLAS_HOOK_TOKEN" -H 'Content-Type: application/json' \
  -d "{\"pane_key\":\"$ATLAS_PANE_KEY\",\"payload\":{\"event\":\"stop\"}}" || true
```
