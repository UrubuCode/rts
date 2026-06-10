# WORKING.md — memória de trabalho da migração rts-engine

> **Propósito:** rastreador operacional vivo. Ler PRIMEIRO a cada sessão;
> **atualizar a cada commit** (status + log + checar etapa). O design canônico
> está em `RTS_ENGINE.md` (§0.1 = roadmap); este arquivo é a camada de execução:
> onde estamos, qual o próximo passo, comandos, e as armadilhas já descobertas.
> Vivo até **todas** as etapas abaixo fecharem.

- **Issue:** #1536 · **Branch:** `feat/engine-method-dispatch-1536`
- **Objetivo final:** `rts-engine` = núcleo cru (abi + registry + builder + gc +
  globals). `rts-macro` e `rts-abi` **deletados**. Camadas (`rts-std`/`rts-node`/
  `rts-browser`) registram a superfície via builder. Codegen = motor genérico que
  lê o `Registry`, nada hardcoded.

---

## 🎯 PRÓXIMO PASSO (sempre no topo)

**F3 — codegen lê o `Registry`** (a ponte). Decisão de escopo (descoberta):
`rts_engine::Registry` guarda `Member` (owned, novo); codegen consome
`&NamespaceMember`/`&GlobalClassSpec` (const, antigo) — **tipos diferentes**.
Dois sub-passos:
- **F3a (mínimo, seguro):** `lookup`/`global_class_lookup` viram índice
  `OnceLock<HashMap>` sobre os const arrays (O(1) + extensível p/ registro
  runtime), **mantendo `&'static NamespaceMember`** — call-sites do codegen
  intocados. Gate: parity-count + `rts.d.ts` byte-idêntico + suíte 1710.
- **F3b (grande, = parte da Fase 2):** codegen migra a consumir
  `rts_engine::Member`; `register_builtins()` converte os const arrays → `Member`;
  runtime registra via builder. Toca todos os call-sites (`member.args` →
  `member.sig.args`, `symbol:&str` → `String`). Fazer junto da Fase 2.

Começar por **F3a**.

---

## ✅ FEITO (log de commits — mais recente embaixo)

| Commit | Etapa | Resumo |
|--------|-------|--------|
| `122e1392` | F1 | `NamespaceMember` += aliases/variadic/default_args + DefaultArg |
| `1af5bea0` | A1a+A2 | MemberFlags + InstanceSetter/VarGetter/VarSetter; macro `#[rts_module]`/`#[rts_var]`/`#[rts_setter]` + readonly/static_field (6/6) |
| `68ba0bba` | doc §10 | novo modo (motor genérico + módulos externos) |
| `c2e1757f` | F2a | `GlobalClassSpec::resolve_instance_method` arity-keyed (abi 17/17) |
| `fe894ab9` | F2b | rotear 3 call-sites de dispatch; suíte TS **1710/1710** |
| `ffc66f20` | doc | §0.1 roadmap canônico + §9.7 `#[rts_global]` |
| `da01b50c` | doc | §11 gates CI + §12 glossário + §13 fora-de-escopo |
| `fae05975` | ENG0 | **crate `rts-engine`** criado (registry + builder + sig!); 5+1 verde |
| `94c5501d` | doc | pivot pro modelo builder (supersede §9.1) |
| `10eea9a2` | Fase 1a/b | **dobra `rts-abi` em `rts-engine/src/abi/`**; rts-abi vira shim; workspace verde, engine 17+5+1 |
| `30d2fc2c` | doc | **WORKING.md** (este arquivo) |
| _(HEAD)_ | Fase 1c | flip codegen/mir/cli `rts_abi::`→`rts_engine::abi::` + Cargo deps; hir/linker droparam dep stale. Shim `rts-abi` agora só via runtime/macro. `cargo check --workspace` verde |

---

## ⬜ FALTA (etapas até terminar)

### Fase 1c — flipar consumers off o shim `rts-abi` (mecânico) ✅ (não-runtime)
- [x] `rts-codegen` (4 arquivos): `rts_abi::`→`rts_engine::abi::`; Cargo `rts-abi`→`rts-engine`. (`pub mod signature` sombreia o glob, sem conflito.)
- [x] `rts-mir` (1), `rts-cli` (1): idem.
- [x] `rts-hir` / `rts-linker`: dep stale removida do Cargo.
- [x] `cargo check --workspace` verde.
- [ ] `rts-runtime` fica pro fim — morre junto com a macro na Fase 2.

### F3 — codegen lê o `Registry` (a ponte, Track A, SEM linkme)
- [ ] `OnceLock<RwLock<Registry>>` (ou `OnceLock<Registry>`) acessível ao codegen.
- [ ] `register_builtins()`: drena `SPECS`/`GLOBAL_CLASS_SPECS` (const) → entradas do `Registry` (modules/classes), montando no startup do `rts.exe`.
- [ ] `abi::lookup` / `global_class_lookup` passam a ler o `Registry` (mantêm a assinatura — consumers não mudam).
- [ ] Rotear os `for spec in GLOBAL_CLASS_SPECS` (vários sites) → iterar o registry.
- [ ] **Gate:** parity-count (mesmas N entradas dos arrays); `rts.d.ts` byte-idêntico; `rts.exe test` 1710/1710.

### Fase 2 — runtime → rts-std via builder (remove `rts-macro`; 933 membros / 73 arquivos)
- [ ] Decidir: migrar in-place em `rts-runtime` OU criar crate `rts-std`.
- [ ] Migrar **uma namespace piloto** (ex.: `hint` ou `math`) do `#[rts_namespace]` pra registro de builder. Provar end-to-end (codegen resolve via Registry).
- [ ] Em lote, namespace-por-namespace + classes globais; **suíte verde entre cada**. Cada migrada → seu `rts_abi`+macro morrem.
- [ ] `#[rts_var]`/`#[rts_global]`/setters consumidos no codegen (A3: VarGetter read `members.rs:1006` + write-path `x.v=5` em `lower_assign_expr` + readonly hard-error).
- [ ] Quando zero uso de `#[rts_*]`: **deletar `crates/rts-macro`** + remover dos members do workspace.

### Limpeza final
- [ ] Quando zero uso de `rts_abi::`: **deletar shim `crates/rts-abi`** + remover do workspace.
- [ ] Mover **gc** + **globals** pra dentro do `rts-engine` (núcleo cru completo).
- [ ] Atualizar `RTS_ENGINE.md` §0.1 + este arquivo a cada marco.

### Pendências paralelas (do RTS_ENGINE.md, não bloqueiam o acima)
- [ ] **Q1** pin Bool=i64 (corrigir doc `ty.rs`/`types.rs`; `signature.rs:9` lowra Bool→I64).
- [ ] **Q2** mover symbol-switches `ns_call.rs:272`/`:314` → `MemberFlags` (RAW_BITS_ARG/AMBIGUOUS_RET).
- [ ] **E2-E4** drenar `builtins.rs` (~182 braços) pras rows — pré-req da tese "registry é portão único".
- [ ] **X1-X5** módulos externos `.dll`/`.so` (§10): libloading, `c_plugin` repr(C), loader JIT, AOT gated.

---

## ⚠️ ARMADILHAS / INVARIANTES (já descobertas)

- **Sem cycle:** `rts-engine` depende de **nada**; `rts-abi` (shim) → `rts-engine`. Nunca fazer engine depender de abi/runtime.
- **`mod tests` aninhado:** dentro de `#[cfg(test)] mod tests`, `super::X` ≠ módulo-irmão. Usar caminho absoluto `crate::abi::X`. (Pegou em `global_class.rs` — `cargo check` passa mas `cargo test` quebra.)
- **`dead_code` = erro** (CLAUDE.md). Código morto deletado no mesmo commit, nunca comentado.
- **Macro gera `::rts_abi::...`:** enquanto a macro existir, o shim `rts-abi` deve resolver. Ao deletar o shim, a macro já não pode existir (Fase 2 antes da limpeza).
- **Stray file:** `docs/package-name-request.md` é untracked e NÃO meu — nunca commitar.
- **CRLF warnings** no commit são inócuos (autocrlf).
- **fn-ptr ABI by-honor:** sig declarada ≠ extern real = corrupção de stack, verifier não pega. Builder só macro-autorado na v1 (ou validar no register).
- **Bool=i64** (não i8) no boundary — `signature.rs:9`.

---

## 🔧 COMANDOS

```bash
cargo check --workspace                    # rápido, valida fold/flip
cargo test -p rts-engine                   # builder + abi movidos (17+5+1)
cargo test -p rts-abi                       # (shim — herda do engine)
cargo build --release                       # ~100s
$env:RUST_BACKTRACE="full"; target/release/rts.exe test   # suíte TS (1710), só se mexeu codegen/runtime
cargo build -p rts-runtime                  # antes de rts compile (AOT staticlib)
```

> `cargo check` NÃO compila `#[cfg(test)]` — rodar `cargo test -p <crate>` quando
> mover/renomear módulos com test mods.

---

## 🧭 COMO RETOMAR (cada sessão)

1. Ler este arquivo (topo: PRÓXIMO PASSO) + `RTS_ENGINE.md` §0.1.
2. `git log --oneline -8` + `git status` (confirmar branch + tree limpo).
3. Executar o PRÓXIMO PASSO; build/test conforme COMANDOS.
4. **Commitar** + **atualizar este arquivo** (mover etapa pra FEITO, novo PRÓXIMO PASSO, log).
