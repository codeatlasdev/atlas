# Prompt: Update Docs

Atualizar a documentação do Atlas (`apps/docs/content/docs/`) para refletir o estado atual do código-fonte.

## Procedimento

### 1. Understand

Leia o código-fonte completo do projeto:

- `apps/cli/src/` — todos os comandos, flags, comportamentos
- `apps/panel/src/` — router, routes, services, lib
- `packages/*/src/` — cada package, exports, interfaces

Para cada arquivo, extraia:
- Funcionalidades expostas
- Flags e opções disponíveis
- Comportamentos por runtime (K3s, Swarm, Firecracker)
- Fluxos de dados (CLI → Panel → Server)

### 2. Diff contra docs

Compare o que o código faz vs o que a documentação diz. Identifique:

- Comandos ou flags não documentados
- Comportamentos desatualizados
- Runtimes faltando (ex: Firecracker)
- Fluxos incorretos ou simplificados demais
- Informações técnicas erradas (ex: REST vs oRPC)

### 3. Reescrever

Para cada arquivo MDX que precisa de atualização:

**Estilo obrigatório:**
- Voz direta, técnica, segunda pessoa (you)
- Frases curtas e declarativas (1-3 frases por parágrafo)
- Tabelas para comparações e referências
- Code blocks para comandos e exemplos
- Sem travessões (—), sem "Here's what happens", sem "That's it"
- Sem exclamações desnecessárias, sem emojis no corpo
- Sem bold excessivo — apenas para termos-chave na primeira menção
- Callouts (`<Callout type="warn">`) apenas para avisos críticos
- Componentes Fumadocs: `<Cards>`, `<Card>`, `<Callout>`

**Estrutura por tipo de página:**
- Conceito: título, descrição curta, diagrama se aplicável, explicação, tabela de detalhes
- Comando CLI: Usage, Description (1-2 frases), Options (tabela), Examples
- Referência: tabelas com campos, tipos, descrições

### 4. Verificar

```bash
bun run --filter docs build
```

O build deve passar sem erros. Se falhar, corrija antes de prosseguir.

### 5. Commit

```bash
git add apps/docs/content/
git commit -m "docs: sync documentation with current source code"
```

### 6. Deploy

O docs é hospedado em `atlas.codeatlas.com.br` via GitHub Pages (static export).

```bash
git push origin main
```

O deploy acontece automaticamente via GitHub Actions após push na main.
Se não houver CI configurado, faça o build e deploy manual:

```bash
cd apps/docs && bun run build
# O output em apps/docs/out/ é o site estático
```

## Checklist

- [ ] Todos os 17 comandos CLI documentados
- [ ] 3 runtimes (K3s, Swarm, Firecracker) mencionados onde relevante
- [ ] API Reference usa oRPC (não REST)
- [ ] atlas.yaml reference inclui campo `platform`
- [ ] Encryption details corretos (AES-256-GCM, Web Crypto, SHA-256 key derivation)
- [ ] Build passa sem erros
- [ ] Commit feito
- [ ] Push para main (deploy automático)
