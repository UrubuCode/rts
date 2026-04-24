# RTS Road Map

## Objetivo

Convergir para o modelo da `main` (pipeline completo + cache), mantendo a organizacao da API nova (`src/abi` + `SPECS` + namespaces modulares), porem com runtime support interno ao `rts` via objetos `.o/.obj` precompilados.

---

## Fase 1 - Alinhamento de arquitetura (paridade com main) — **CONCLUIDA**

- [x] Pipeline por grafo de modulos (`compile_graph`).
- [x] Cache de objetos e metadados por modulo.
- [x] Runtime support resolvido por payload interno do proprio `rts`
      (runtime_support.a embutido via build.rs).
- [x] Remover caminho de download de runtime support library.
- [x] Remover fallback para `cargo build --lib` no fluxo de uso.
- [x] Fluxo `node_modules/.rts` para artefatos auxiliares.

---

## Fase 2 - Consolidacao da API nova organizada — **CONCLUIDA**

- [x] `abi::SPECS` como fonte unica para codegen/runtime/types.
- [x] Namespaces `io`, `fs`, `gc` completos no fluxo novo.
- [x] Declaracoes TypeScript sincronizadas a partir da ABI nova.
- [x] Namespaces adicionais consolidados: `math` (27 membros + 4 constantes),
      `bigfloat` (decimal 30 digitos).

---

## Fase 3 - Migracao gradual de melhorias de codegen — **em andamento**

### Concluido

- [x] Intrinsics inline (#87)
- [x] f64 modulo via fmod (#89)
- [x] Switch jump table (#91)
- [x] Tail call optimization (#93)
- [x] Imm forms (#94)
- [x] JIT mode (#95)
- [x] First-class function pointers (#97 fase 1)
- [x] MemFlags::trusted (#98)
- [x] stack_slot helper (#99)

### Pendente

- [ ] #96 DWARF debug info
- [ ] #97 fases 2/3 — arrow como valor + captura de escopo externo
- [ ] #90 loop block params — deferred; SSA atual ja funcional
- [ ] Novos namespaces sobre o contrato atual (#12-#39: env, path, time, process,
      net, thread, etc)

Regra: nao juntar refactor grande e mudanca de comportamento no mesmo lote.

---

## Guardrails

- Sem dependencia de `xtask` para build padrao.
- Build reproduzivel via comandos `cargo` diretos.
- Distribuicao standalone sem exigir Rust/Cargo no ambiente de uso.
- Sem dependencia de download de runtime support library em tempo de execucao.
- Cada etapa com criterio de validacao objetivo antes de avancar.
