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

> **Fase 2 NAMESPACES ✅ (todas exceto gc+collections)** — a macro
> `#[rts_namespace]` passou a **auto-emitir `register(e: &mut Engine)`** com a
> MESMA metadata do const SPEC; `register_builtins` folda ~36 ns macro'd via
> `register()` + as 9 hand-migradas. **Só `gc` (GC sensível) e `collections`
> (owner hand-written com `concat_members`, sem `#[rts_namespace]` próprio)
> seguem no const `SPECS`.** `rts_engine::Member += intrinsic` (math inline
> preservado). Suíte **1710/1710**, workspace lib verde, macro 6/6.
>
> **Próximos passos (na ordem):**
> 1. **Classes globais (27)** — `register_builtins` precisa foldar também
>    `engine.registry().classes()` via um `leak_class` análogo ao `leak_namespace`
>    (+ `classes`/`classes_ordered` no Registry do codegen). O builder de classe
>    (`engine.class(...)`) precisa existir/ligar no `GLOBAL_CLASS_SPECS`. Migrar
>    `#[rts_class]` → builder, ns-a-ns, suíte verde entre cada.
> 2. **Deletar `rts-macro`** quando zero `#[rts_*]` (hoje a macro ainda é a fonte
>    do `register()` das ~36 ns + dos `#[rts_class]`; some quando classes migrarem
>    e as ns macro'd virarem hand-register OU a macro virar parte do rts-engine).
> 3. **Deletar shim `rts-abi`** quando zero `rts_abi::`.
> 4. **Mover `gc` + `globals` pra dentro do `rts-engine`** (núcleo cru completo);
>    migrar gc+collections off o const SPECS (esvaziar o const).
> 5. **E2-E4 / X1-X5** (dispatch engine + plugins externos `.dll`/`.so`).
>
> **⚠️ Armadilha resolvida (não repetir):** membro `alias`/`external` tem
> `fn_ptr` null e REUSA o `symbol` do membro dono (real). `leak_namespace` NÃO
> pode gravar `(symbol, 0)` no jit_symbols — sobrescreve o endereço real com NULL
> → call p/ 0x0 (ACCESS_VIOLATION, não-determinístico por ordem de HashMap). Skip
> fn_ptr null. Mesma regra no `Registry::insert_module`/`insert_class` do engine.
> **⚠️ Consumidores do const `SPECS`:** ao migrar ns p/ fora do const, TODO
> `for s in SPECS`/`SPECS.iter()` que decide membership/listagem quebra (vira
> só gc+collections). Já trocados p/ `registry_specs_ordered()`/`registry_namespace()`:
> apis, init, jit-diag, `is_global` (mod.rs), `builtin_module_keys`, `build_pure_ns_set`.

### (próximo passo original abaixo)


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

**F3 ✅ + F4 ✅** — bridge builder→codegen COMPLETO: resolução (`lookup` acha) +
execução (símbolo injetado no JIT). A arquitetura builder→registry→codegen→JIT
está provada ponta-a-ponta no nível de API (testes); falta exercê-la com uma ns
real (piloto).

**Bridge COMPLETO:** registry é lido por `lookup` (F3a), JIT (`F4`) e import
resolver (`F4b`). Um módulo do builder registrado é importável + resolvível +
executável. Falta só o LUGAR que registra (camadas) + integrar o gerador d.ts.

**Agora Fase 2 — PILOTO (1 namespace real via builder, ex.: `hint`):**
hint = 5 membros: `spin_loop()→void`, `black_box_i64(I64)→I64`,
`black_box_f64(F64)→F64`, `unreachable()→void` (ts "never"), `assert_unchecked(Bool)→void`.
Symbols `__RTS_FN_NS_HINT_*` (#[no_mangle], existem).
1. Onde registrar: **dentro de `register_builtins()`** (o seed do registry em
   `abi/mod.rs`) — construir um `Engine`, registrar as camadas, e foldar os
   Modules no `Registry` local ANTES de retornar. **NÃO** chamar
   `register_namespace`/`leak_namespace` (que usam o `registry()` global) de
   dentro do seed → reentrância no `OnceLock` = deadlock. Fazer um
   `leak_into(&mut maps, module)` que insere nos maps locais.
2. `hint::register(&mut Engine)` apontando pros externs
   (`rts_runtime::namespaces::hint::__RTS_FN_NS_HINT_* as *const u8`), com sigs +
   `ts_signature` EXATOS (p/ não mudar o d.ts).
3. **Remover** `&...hint::SPEC` do const `SPECS` (senão dup). `pub const SPEC`
   fica unused mas é `pub` → sem dead_code. add_fn! do hint pode ficar (dup
   overwrite no JIT, mesmo ptr).
4. **✅ d.ts (F3d feito):** `emit_types` já itera `registry_specs_ordered()`.
   Migrar hint = ele sai do const SPECS, entra no registry via builder (append) →
   d.ts regenera com o bloco `rts:hint` no FIM. Rodar `rts emit-types` + commitar
   o `rts.d.ts` (diff = só hint movendo). **`apis.rs`/`init.rs` ainda iteram
   SPECS** — rotear pra `registry_specs_ordered()` também (ou hint some do `rts
   apis`; não-lintado, baixa prio).
5. **Gate:** TS `import { spin_loop } from "rts:hint"` roda sob `rts run`;
   `rts emit-types` regenerado+commitado; suíte 1710.
- Piloto OK → repetir mecânico p/ as 50 ns + 27 classes (o grosso, 933 membros).
  Aí a macro e o shim rts-abi morrem.

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
| `214fc402` | Fase 1c | flip codegen/mir/cli `rts_abi::`→`rts_engine::abi::` + Cargo deps; hir/linker droparam dep stale. Shim só via runtime/macro |
| `877b2ebd` | F3a | `lookup`/`global_class_lookup` viram índice `OnceLock<Registry>` (`register_builtins()` semeia dos const arrays). Suíte **1710/1710** |
| `7962a782` | F3b | registry vira `RwLock` + `register_namespace`/`register_class` + `leak_namespace` (Module do builder → `&'static NamespaceSpec`). Teste-ponte: ns do builder achada pelo `lookup`. Suíte **1710/1710** |
| `c0d75f54` | F4 | `leak_namespace` grava `(symbol, fn_ptr)` em `JIT_SYMBOLS`; `runtime_jit_symbols()`; `jit.rs` injeta no `JITBuilder` após o `add_fn!`. Habilita EXECUÇÃO de fn do builder. Suíte **1710/1710** |
| `1b8559a6` | F4b | `builtin_module` (import resolver) lê `registry_namespace()` em vez do const `SPECS` → módulos do builder ficam importáveis. Registry é a fonte de leitura dos 3 consumidores (lookup, JIT, import). Suíte **1710/1710** |
| `33801868` | Q1 | fix tabela `ty.rs` Bool `i8`→`i64` (doc-only) |
| `4f261284` | doc | WORKING.md — pega do Q2 (1ª tentativa revertida) |
| `4609b7c6` | Q2 | symbol-switches de `ns_call.rs` → `MemberFlags` (RAW_BITS_ARG/AMBIGUOUS_RET/UNDEF_RET). 13 membros flagados; entradas mortas `reduce_right*` dropadas. Suíte **1710/1710** |
| `f7500ebb` | F3d | gerador d.ts itera `registry_specs_ordered()` (Vec ordenado) em vez do const `SPECS`. Byte-idêntico. Desbloqueia Fase 2 |
| `ca2e73e7` | **Fase 2 piloto** | **`hint` migrado da macro → builder** (1ª ns). hand-externs + `hint::register(Engine)`; `register_builtins` folda os modules; hint fora do const SPECS. `rts run` ok; suíte 1710/1710. Prova Fase 2 |
| `163c508f` | Fase 2 #2 | **`hash`** migrado + `rts_engine::Member += pure`. Suíte 1710/1710 |
| `b46de0e5` | Fase 2 #3 | batch **`alloc`+`time`+`trace`** (3 ns). 1710/1710 |
| `e754f92b` | Fase 2 #4 | batch **`env`+`path`** (2 ns). 1710/1710 |
| `93ccbc0a` | Fase 2 #5 | **`fmt`** (10 membros pure: parse/fmt, on_null sentinels i64::MIN/NaN/-1, I32; + extern parse_int_radix não-membro + tests). `rts run` "42 0xff 1 3.14"; **1710/1710**. 8 ns migradas |
| `090df81a` | Fase 2 #6 | **`ptr`** (14 membros, hand-externs). 9 ns migradas à mão. 1710/1710 |
| _(HEAD)_ | **Fase 2 BULK** | **macro `#[rts_namespace]` auto-emite `register(e: &mut Engine)`** (mesma metadata do const SPEC → `NamespaceMember` byte-equiv via leak). `register_builtins` folda **~36 ns** via register() (io/json/date/fs/math/net/num/mem/bigfloat/buffer/ffi/atomic/sync/string/process/promise/os/http_server/crypto/regex/audio/runtime/test/thread/parallel/tls/globals*+events). **Só `gc`+`collections` restam no const `SPECS`.** `rts_engine::Member += intrinsic` (math sqrt/abs/min/max inline preservados). **Bug crítico achado+corrigido:** `leak_namespace` gravava `(symbol, 0)` p/ membros alias (fn_ptr null) → sobrescrevia o símbolo real com NULL → call p/ 0x0 (ACCESS_VIOLATION não-determinístico, ordem HashMap). Fix: pular fn_ptr null no jit_symbols. 6 consumidores do const SPECS → registry (apis/init/jit-diag/is_global/builtin_module_keys/pure-ns-set). Teste stale `rts:ui`→`rts:gc`. Workspace lib + macro 6/6 + **suíte 1710/1710** |

---

## ⬜ FALTA (etapas até terminar)

### Fase 1c — flipar consumers off o shim `rts-abi` (mecânico) ✅ (não-runtime)
- [x] `rts-codegen` (4 arquivos): `rts_abi::`→`rts_engine::abi::`; Cargo `rts-abi`→`rts-engine`. (`pub mod signature` sombreia o glob, sem conflito.)
- [x] `rts-mir` (1), `rts-cli` (1): idem.
- [x] `rts-hir` / `rts-linker`: dep stale removida do Cargo.
- [x] `cargo check --workspace` verde.
- [ ] `rts-runtime` fica pro fim — morre junto com a macro na Fase 2.

### F3 — codegen lê o `Registry` (a ponte, Track A, SEM linkme)
- [x] **F3a:** índice `OnceLock<Registry>`; `register_builtins()` semeia. 1710/1710. (`877b2ebd`)
- [x] **F3b:** `RwLock` + `register_namespace`/`register_class` + `leak_namespace` (Member→`&'static NamespaceMember`); teste-ponte. 1710/1710. (HEAD)
- [x] **F4:** `JIT_SYMBOLS` gravado em `leak_namespace`; `runtime_jit_symbols()`; `jit.rs` injeta após `add_fn!`. EXECUÇÃO habilitada. 1710/1710. (HEAD)
- [ ] Rotear os `for spec in GLOBAL_CLASS_SPECS` (vários sites) → iterar o registry (p/ módulos do builder/externos aparecerem nas iterações). _(adiar até precisar)_
- [ ] `rts.d.ts` byte-idêntico (gerador itera `SPECS` direto — ok por ora).
- [ ] Encolher os 1104 `add_fn!` via `GetProcAddress`/`dlsym` — depois do piloto, opcional.

### Fase 2 — runtime → rts-std via builder (remove `rts-macro`; 933 membros / 73 arquivos)
- [x] Decidir: migrar **in-place em `rts-runtime`** (não criar rts-std agora).
- [x] Migrar namespace piloto (`hint`). Provado end-to-end (codegen resolve via Registry). (`ca2e73e7`)
- [x] **Todas as namespaces** migradas: 9 à mão + ~36 via macro-`register()` auto-emitido. **Só gc+collections no const SPECS.** Suíte 1710/1710 entre cada. (HEAD)
- [ ] **Classes globais (27)** via builder — falta `leak_class` + fold de `engine.registry().classes()` no `register_builtins` + builder de classe ligado no `GLOBAL_CLASS_SPECS`.
- [ ] `#[rts_var]`/`#[rts_global]`/setters consumidos no codegen (A3: VarGetter read `members.rs:1006` + write-path `x.v=5` em `lower_assign_expr` + readonly hard-error).
- [ ] Quando zero uso de `#[rts_*]`: **deletar `crates/rts-macro`** + remover dos members do workspace. _(macro ainda gera `register()` das ~36 ns + os `#[rts_class]`.)_

### Limpeza final
- [ ] Quando zero uso de `rts_abi::`: **deletar shim `crates/rts-abi`** + remover do workspace.
- [ ] Mover **gc** + **globals** pra dentro do `rts-engine` (núcleo cru completo).
- [ ] Atualizar `RTS_ENGINE.md` §0.1 + este arquivo a cada marco.

### Pendências paralelas (do RTS_ENGINE.md, não bloqueiam o acima)
- [x] **Q1** pin Bool=i64 — corrigida a tabela de `ty.rs` (dizia `i8`; `type Bool = i64` + doc 32-35 já corretos). Doc-only. (HEAD)
- [x] **Q2** ✅ symbol-switches `ns_call.rs` → `MemberFlags`. Tracing resolveu a pega: `reduce_right*` é só builtins.rs (sem membro) → entrada morta dropada; `JSON_PARSE5` tem membro em globals/json5 (symbol-override) → flagado. 13 membros, suíte 1710/1710. (HEAD)
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
- **d.ts gerador itera `SPECS` const** (`emit_types.rs`/`apis.rs`/`init.rs`): remover uma ns do const SPECS a faz sumir do `rts.d.ts` → lint byte-idêntico quebra. Migrar uma ns pro builder exige o gerador ler o `registry`.
- **Reentrância OnceLock:** não chamar `register_namespace`/`leak_namespace` (usam `registry()` global) de dentro de `register_builtins()` (o init do mesmo OnceLock) → deadlock. Foldar nos maps locais.
- **`rts_engine::sig!`** chamável por path: `rts_engine::sig!(StrPtr, I64 => Handle)`. Tipos via `$crate::AbiType::X`.
- **Nem todo símbolo em `ns_call.rs` tem membro de macro:** vários externs são `pub extern "C"` hand-escritos (ex.: `COLLECTIONS_VEC_REDUCE_RIGHT*`, `JSON_PARSE5`) alcançados por `builtins.rs`/symbol-override, não por `#[rts_fn]`. Mover comportamento por-símbolo pra `MemberFlags` exige achar o MEMBRO certo (pode ser via `symbol=` em outra ns) + verificar o caminho de chamada. Q2 não é find-replace cego.

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
