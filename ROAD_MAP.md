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
      `bigfloat` (decimal 30 digitos), `time`, `env`, `path`, `buffer`,
      `string`, `process`, `os`, `collections`, `hash`, `fmt`, `crypto`
      — 16 namespaces total.

---

## Fase 3 - Migracao gradual de melhorias de codegen — **em andamento**

### Concluido

Codegen:
- [x] Intrinsics inline (#87)
- [x] f64 modulo via fmod (#89)
- [x] Switch jump table (#91)
- [x] Tail call optimization (#93)
- [x] Imm forms (#94)
- [x] JIT mode (#95)
- [x] First-class function pointers (#97 fase 1)
- [x] MemFlags::trusted (#98)
- [x] stack_slot helper (#99)
- [x] Compound assign (#48)
- [x] Ternario (#49)
- [x] Bitwise ops (#47)
- [x] Exponentiation (#52)
- [x] `typeof`/`void`/`delete` (#51)
- [x] `??` e optional call `?.()` (#50)
- [x] Template literals (#46)
- [x] let/const scoping (#44)
- [x] Function/arrow expressions (#55, #56)

Namespaces:
- [x] math (#20), time (#14), env (#12), path (#13)
- [x] buffer (#22), string (#25), process (#15), os (#19)
- [x] collections (#26), bigfloat

### Pendente

Codegen:
- [ ] #96 DWARF debug info
- [ ] #97 fases 2/3 — arrow como valor + captura de escopo externo
- [ ] #90 loop block params — deferred; SSA atual ja funcional
- [ ] #92 autovec — fechada como inviavel sem loop vectorizer
- [ ] #53 object literals, #54 array literals — desbloqueia classes/HOF idiomatico
- [ ] #62 try/catch/throw — error handling
- [ ] #60/#61 for-of / for-in

Namespaces:
- [ ] #23 fmt, #21 hash, #24 crypto, #28 regex
- [ ] #16 net, #17 thread, #18 channel, #27 sync
- [ ] #29-#39 (ffi, atomic, mem, ptr, num, hint, alloc, task, mpmc, simd, backtrace)

Regra: nao juntar refactor grande e mudanca de comportamento no mesmo lote.

---

## Guardrails

- Sem dependencia de `xtask` para build padrao.
- Build reproduzivel via comandos `cargo` diretos.
- Distribuicao standalone sem exigir Rust/Cargo no ambiente de uso.
- Sem dependencia de download de runtime support library em tempo de execucao.
- Cada etapa com criterio de validacao objetivo antes de avancar.
