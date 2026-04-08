# Filosofia — Atlas

Qualidade é a única restrição. Tempo, prazo, complexidade — irrelevantes.

## Mentalidade

Atlas é infraestrutura. Infraestrutura quebrada destrói confiança. Infraestrutura que funciona é invisível. O padrão é excelência silenciosa.

Nunca cortamos caminho. Nunca entregamos "bom o suficiente". Nunca ignoramos edge cases porque "é raro" — em infra, o raro acontece às 3h da manhã num servidor de produção.

## Hierarquia de prioridades

1. **Confiabilidade** — se o Atlas falha, o deploy do cliente falha. Inaceitável.
2. **Segurança** — secrets, tokens, kubeconfigs. Tudo encriptado. Sem exceção.
3. **DX (Developer Experience)** — o dev roda `atlas deploy` e esquece. Zero fricção.
4. **Performance** — provisioning rápido, deploys rápidos, CLI responsiva.
5. **Manutenibilidade** — código que qualquer dev entende em 5 minutos.

## O que NÃO fazemos

- Não salvamos secrets em plain text — tudo passa por `@atlas/crypto`
- Não duplicamos código entre CLI e Panel — extraímos para packages
- Não ignoramos erros silenciosamente — `catch {}` é proibido
- Não hardcodamos valores — env vars validadas com zod
- Não assumimos que SSH vai funcionar — sempre tratamos falha
- Não deixamos TODOs sem prazo
- Não desabilitamos regras de lint
