# RTS Road Map

## Objetivo

Convergir para o modelo da `main` (pipeline completo + cache + runtime lib externo), mantendo a organizacao da API nova (`src/abi` + `SPECS` + namespaces modulares).

---

## Fase 1 - Alinhamento de arquitetura (paridade com main)

- [ ] Pipeline por grafo de modulos (`compile_graph`).
- [ ] Cache de objetos e metadados por modulo.
- [ ] Runtime support library resolvida por `runtime_lib`.
- [ ] Fluxo `node_modules/.rts` para artefatos auxiliares.

Resultado esperado: base de compilacao equivalente a `main`, sem perder a estrutura nova da ABI.

---

## Fase 2 - Consolidacao da API nova organizada

- [ ] `abi::SPECS` como fonte unica para codegen/runtime/types.
- [ ] Namespaces `io`, `fs`, `gc` completos no fluxo novo.
- [ ] Declaracoes TypeScript sincronizadas a partir da ABI nova.

Resultado esperado: arquitetura da `main` + superficie de API nova limpa e extensivel.

---

## Fase 3 - Migracao gradual da nova bench

- [ ] Integrar melhorias de codegen em lotes pequenos.
- [ ] Validar benchmark e regressao a cada lote.
- [ ] Medir compile time, tamanho final e estabilidade.

Regra: nao juntar refactor grande e mudanca de comportamento no mesmo lote.

---

## Guardrails

- Sem dependencia de `xtask` para build padrao.
- Build reproduzivel via comandos `cargo` diretos.
- Cada etapa com criterio de validacao objetivo antes de avancar.
