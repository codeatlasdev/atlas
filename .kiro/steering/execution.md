# Execução — Como Agir Neste Projeto

Protocolo de execução para qualquer agente trabalhando no Atlas. Não é sugestão — é obrigação.

## Antes de Agir: Pesquisar

Nenhuma ação sem pesquisa prévia. Nenhuma implementação sem entender o contexto.

1. **Ler o contexto** — README.md, steering files relevantes
2. **Pesquisar no codebase** — como isso já é feito? Existe padrão? Existe package reutilizável?
3. **Pesquisar na documentação** — a lib suporta isso? Qual a API correta? Mudou na versão atual?
4. **Pesquisar na internet** — qual a abordagem moderna? Existem alternativas melhores?
5. **Verificar** — o que você "sabe" ainda é verdade? Confirme com fonte.

Achismo é proibido. "Eu acho que funciona assim" não é aceitável.

## Durante a Ação: Rastrear

### Para tarefas com plano (`__plans/`)

- Ler `progress.md` ANTES de começar
- Atualizar "Last updated" e mover tarefa para "In Progress"
- Executar UMA tarefa por vez, completar antes de avançar
- Atualizar `progress.md` DEPOIS de cada tarefa
- Rodar `bun run check` após cada tarefa

### Para tarefas sem plano

- Se a tarefa tem mais de 3 passos, considerar criar um plano
- Se a tarefa toca mais de 5 arquivos, criar um plano
- Se a tarefa envolve decisão arquitetural, criar um plano

## Qualidade de Código

### Antes de escrever

- Existe package reutilizável? Usar.
- Existe padrão estabelecido? Seguir.
- Existe steering file? Ler.

### Ao escrever

- Um arquivo, uma responsabilidade
- Nomes que comunicam intenção
- Early returns sobre nesting
- Tipos explícitos, sem `any`
- Secrets sempre encriptados via `@atlas/crypto`
- SSH sempre com tratamento de falha
- Env vars sempre via `@atlas/env`

### Depois de escrever

- `bun run check` passa?
- Imports não usados removidos?
- Código morto removido?
- Funciona em modo interativo E `--yes` (CI)?

## Comunicação

- Responder no idioma do usuário
- Ser direto e conciso
- Explicar apenas decisões não óbvias
- Admitir incerteza quando existir
- Nunca inventar informação — se não sabe, pesquisa
