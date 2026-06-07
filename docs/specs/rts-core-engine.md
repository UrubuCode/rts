# RTS Core Engine — declaração única + camada de conversão + tier dinâmico

Branch: `feat/rts-core-engine`. Estado: **em implementação por estágios**.

Documento canônico do refator que (1) move `rts-runtime/namespaces` para
`rts-core/{namespaces,nodespaces}`, (2) substitui a quádrupla-escrita por uma
**declaração única** via proc-macro + registry, (3) torna builtins classes de
verdade (prototype, `instanceof`, `extends`, `typeof`), (4) introduz um **tier
dinâmico** (valor uniforme + object model) sem perder o tier nativo atual.

## Por que

Hoje registrar uma função de namespace exige **4 lugares concordando**:

1. `namespaces/<ns>/abi.rs` — `NamespaceMember { name, symbol, args, returns,
   doc, ts_signature, ... }` + `NamespaceSpec`.
2. `namespaces/<ns>/ops.rs` — `#[no_mangle] extern "C" fn __RTS_FN_NS_<NS>_<NAME>`.
3. `rts-codegen/src/abi/mod.rs` — entrada no array `SPECS` / `GLOBAL_CLASS_SPECS`.
4. `rts-codegen/src/codegen/jit.rs` — `add_fn!("__RTS_...", path)` (1180 entradas).

A symbol string é a cola, repetida 3×. Frágil, verboso, e expõe nomes diretos
(`__RTS_FN_NS_FS_REMOVE`) — superfície de ataque por adivinhação/forja.

Classes builtin (`GlobalClassSpec`) são escritas à mão; `typeof`/`instanceof`
são tratados espalhados no lowering; dispatch virtual usa **switch sobre string**
do tag `__rts_class` (O(n) `string_eq` por chamada). Não é 100% JS: monkey-patch
de primitivo, proxy, proto mutável, `extends` nativo completo não funcionam.

## Estrutura nova de diretórios

```
crates/rts-core/                 (renomeia rts-runtime)
  src/
    namespaces/<ns>/             — modelo RTS, imports `rts` e `rts:*`
      mod.rs                     — só `#[rts_namespace]` impl + submódulos de impl
    nodespaces/<ns>/             — API Node.js (futuro: fs, path, os, process, ...)
    object_model/                — proto runtime, shapes, dispatch (tier B)
    value/                       — valor uniforme NaN-box (tier A, fase tardia)
crates/rts-macro/                — proc-macro: #[rts_namespace] #[rts_fn] #[rts_class]
crates/rts-abi/                  — tipos do registry (linkme slices) + ty aliases + hash
```

`namespace` = modelo RTS (`import { x } from "rts"` / `"rts:<ns>"`).
`nodespace` = API Node (`import { x } from "node:<ns>"`), camada futura por cima
dos mesmos primitivos.

## Declaração única — proc-macro

### Função de namespace

```rust
use rts_macro::rts_namespace;
use rts_abi::ty::{Handle, U64, I64};

/// Paralelismo de dados via Rayon (map/for_each/reduce sobre Vec<i64>).
#[rts_namespace(parallel)]
impl Parallel {
    /// Aplica `fn_ptr(x)` em paralelo sobre cada elemento de um `Vec<i64>`.
    /// @param vec_handle - handle do Vec<i64>
    /// @param fn_ptr - extern "C" fn(i64)->i64
    /// @returns novo Vec<i64>
    /// @example const out = parallel.map(vec, double);
    #[rts_fn]
    pub fn map(vec_handle: Handle, fn_ptr: U64) -> Handle { /* corpo */ }
}
```

A macro deriva, de UMA função:

- **extern typed**: `#[no_mangle] extern "C" fn <symbol>(...)` com `<symbol>`
  hasheado (opaco) — ver "Symbol hashing".
- **registry entry**: `NamespaceMember` num `linkme` distributed slice
  (`MEMBER_REGISTRY`), substituindo a tabela `MEMBERS` + array `SPECS`.
- **args/returns**: derivados do **token** do tipo (tabela abaixo).
- **doc**: do `///` (TSDoc completo, com `@param/@returns/@example`).
- **ts_signature**: derivada de `nome(arg: tipo,...) : ret`; override por
  `#[rts_fn(ts = "...")]`.

### Mapa token → ABI/TS

| Token Rust | AbiType | TS | Repr |
|---|---|---|---|
| `Handle` | Handle | number | u64 |
| `U64` | U64 | number | u64 |
| `I64`/`i64` | I64 | number | i64 |
| `F64`/`f64` | F64 | number | f64 |
| `Bool`/`bool` | Bool | boolean | i8/i64 |
| `Str` | StrPtr | string | (ptr,len) |
| `()`/ausente | Void | void | — |

`Handle` vs `U64` (ambos `u64`): distinguidos pelo **token escrito**. Por isso
aliases em `rts_abi::ty` que carregam a intenção ABI.

### Classe global

```rust
#[rts_class(EventEmitter)]
impl EventEmitter {
    #[ctor]      pub fn new() -> Handle { ... }                  // new EventEmitter()
    #[method]    pub fn on(self: Handle, name: Str, l: U64) -> Handle { ... } // proto
    #[getter]    pub fn listenerCount(self: Handle, n: Str) -> I64 { ... }
    #[static_fn] pub fn from(pairs: Handle) -> Handle { ... }     // EventEmitter.from
    #[constant]  pub const defaultMaxListeners: I64 = 10;
}
```

Mapa termo → MemberKind:

| Termo | JS | MemberKind | attr |
|---|---|---|---|
| fn | `ns.f()` | Function | `#[rts_fn]` |
| fn_prototype | `C.prototype.m` → `inst.m()` | InstanceMethod | `#[method]` (self: Handle) |
| fn_class_prototype | `C.m()` | StaticMethod | `#[static_fn]` |
| ctor | `new C()` | Constructor | `#[ctor]` |
| getter/setter | `inst.p` | Instance{Getter,Setter} | `#[getter]`/`#[setter]` |
| const | `C.K` | Constant | `#[constant]` |

`self: Handle` no 1º param ⇒ instance method (recebe `this` boxed).

## Symbol hashing (segurança por opacidade — honesto: não é sandbox)

`<symbol>` = `__RTS_` + hex curto de `fnv1a(ns + "." + name [+ build_salt])`.
Ambos os lados usam o mesmo hash (codegen lê `registry.symbol`, nunca recomputa).
Combina com `strip = "symbols"` (já ativo) — tira nomes do exe shipado.

Protege contra inspeção/forja do **binário**. **Não** protege contra quem
controla o TS fonte (codegen chama o que o registry expõe de qualquer jeito). A
proteção real é o **registry ser o único gate de dispatch** — eval/FFI também
resolvem só via registry (allowlist). Hash fecha o vazamento por adivinhação.

## Mata as 4 escritas

| Antes | Depois |
|---|---|
| `abi.rs` MEMBERS+SPEC | `#[rts_fn]`/`#[rts_class]` derivam → linkme |
| `ops.rs` extern fn | corpo dentro do `#[rts_fn]`; extern gerado |
| array `SPECS`/`GLOBAL_CLASS_SPECS` | derivado iterando `MEMBER_REGISTRY`/`CLASS_REGISTRY` |
| `jit.rs` 1180 `add_fn!` | `JITBuilder::symbol_lookup_fn` → GetProcAddress/dlsym no próprio binário |

JIT: todo `__RTS_*` é `#[no_mangle]` e linkado estático no `rts.exe` → já está
na tabela de símbolos do processo. `symbol_lookup_fn` resolve por nome:
Win `GetProcAddress(GetModuleHandle(NULL), name)`, Unix `dlsym(RTLD_DEFAULT, name)`.
Caveat AOT: garantir que o linker não pode os `#[no_mangle]` não-referenciados
(usar `#[used]` / lista de export) — testar no MSVC.

## Camada de conversão — class de verdade

O spec plano vira class real:

- **object model** (`object_model/`): cada class registra um **prototype object
  real** (GC Map com method handles). `instanceof`, `Object.getPrototypeOf`,
  `C.prototype.m`, passar método como valor — tudo bate em estrutura real.
- **dispatch**: troca o switch-string do tag `__rts_class` por **shape/vtable**
  (offset/índice fixo) → O(1) + inline cache. Devirtualização quando o tipo é
  provado.
- **typeof**: do tag do valor (unificado, não espalhado no lowering).
- **extends nativo + super**: liga a proto chain ao `prototype` gerado; override
  de subclasse roteia virtual. (Pedaço mais pesado.)

## Tier dinâmico (fase tardia) — 100% JS sem perder o nativo

Modelo de **duas faces** + router por tipo:

- **fast tier** (default): symbol typed, valor nativo unboxed, `call` direto —
  o que existe hoje. Mantido para código TS bem-tipado e não-escapante.
- **dyn tier**: valor uniforme NaN-box (`{tag,payload}`; f64 nos próprios bits,
  sem heap; objeto/string já são handles). A macro emite, junto do fast,
  um thunk `<symbol>_DYN(this: JsValue, argv, n) -> JsValue` instalado no
  prototype. Monkey-patch/reflexão/`any`/proxy caem aqui.
- **router** (generaliza o híbrido MIR↔AST): type-checker + escape-analysis por
  call-site. Provado typed & não-escapa & builtin não-patcheado → fast. Senão →
  dyn pela proto. Especulação com **guard** (shape/dirty check) + **deopt**.

### Invariante de performance (nunca regredir o hot path)

1. Valor provado `number`/`bool`/tipo concreto que **não escapa** → NUNCA boxa.
   Compila idêntico a hoje. Verificável: `rts ir bench.ts` não mostra box/guard
   no loop.
2. `inst.m()` com tipo provado → symbol typed direto (sem proto lookup).
3. Box só na **fronteira** fast→dyn, pay-per-use, e número/bool em NaN-box não
   heap-alocam.
4. Guard especulativo = 1 compare+branch predito-tomado; world-assumption global
   ("builtins não-patcheados") evita check por-call.
5. Benchmarks canônicos (`bench/monte_carlo_pi.ts` etc) DEVEM manter o IR atual.
   CI compara via `rts ir`.

### Ganhos de velocidade (além de não regredir)

- dispatch de método class: string-tag O(n) → vtable/IC O(1).
- campo de objeto: map lookup → offset fixo (hidden-class).
- menos bail pro AST: router especializa em vez de desistir.
- especulação+deopt destrava otimizações hoje bloqueadas pela conservadoria.
- silent-parallelism/intrinsics aplicam em mais sítios (análise de tipo, não
  matcher sintático).
- loop numérico puro: **sem ganho** (já no teto nativo) — e sem regressão.

## Estágios (cada um entrega sozinho, build verde)

1. **Fundação macro** — crate `rts-macro` + `rts_abi::ty` aliases + `symbol_hash`.
   `#[rts_fn]` derivando member + extern. Migrar 1 namespace pequeno (`hint`),
   coexistindo com o sistema antigo. **(em andamento)**
2. **linkme registry** — `MEMBER_REGISTRY`/`CLASS_REGISTRY`; `SPECS` derivado;
   migrar namespaces em lote.
3. **jit symbol_lookup_fn** — matar `add_fn!`. Testar AOT export no MSVC.
4. **rts-core rename** — mover `rts-runtime` → `rts-core`, criar `nodespaces/`.
5. **`#[rts_class]` + object model** — prototype real, `instanceof`, getters.
   Trocar tag-string por shape/vtable.
6. **extends nativo + super** — proto chain completa.
7. **valor uniforme + router + deopt** — tier dinâmico, 100% JS. Invariante de
   perf fixada por CI (`rts ir`).

Honestidade: estágios 5-7 são um épico de engine; 1-4 são mecânicos e fecham a
quádrupla-escrita + estrutura + segurança. Sem big-bang: o build compila a cada
estágio; o tier fast nunca regride.
```
