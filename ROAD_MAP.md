# RTS Road Map

## Objetivo

Convergir para o modelo da `main` (pipeline completo + cache), mantendo a organizacao da API nova (`src/abi` + `SPECS` + namespaces modulares), porem com runtime support interno ao `rts` via objetos `.o/.obj` precompilados.

---

## Fase 1 - Alinhamento de arquitetura (paridade com main)

- [ ] Pipeline por grafo de modulos (`compile_graph`).
- [ ] Cache de objetos e metadados por modulo.
- [ ] Runtime support resolvido por payload interno do proprio `rts`.
- [ ] Remover caminho de download de runtime support library.
- [ ] Remover fallback para `cargo build --lib` no fluxo de uso.
- [ ] Fluxo `node_modules/.rts` para artefatos auxiliares.

Resultado esperado: base de compilacao equivalente a `main`, sem perder a estrutura nova da ABI e sem dependencia de runtime externo.

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
- Distribuicao standalone sem exigir Rust/Cargo no ambiente de uso.
- Sem dependencia de download de runtime support library em tempo de execucao.
- Cada etapa com criterio de validacao objetivo antes de avancar.
