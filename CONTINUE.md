# CONTINUE.md — retomar a partição de crates (Fase 1b)

> Handoff pra sessão nova. Estado em `86c50b8f` (branch `feat/engine-method-dispatch-1536`).
> Contexto canônico: `WORKING.md` + `.claude/plans/partitioned-meandering-milner.md`.
> Tudo abaixo é GATEADO: `cargo build --release` + `target/release/rts.exe test`
> (1710/1710) por batch; AOT pra fixtures de GC/classe.

## TL;DR — onde estamos

Partição de crates do RTS. **FEITO:** rts-macro deletada; nodespace→`rts-node`; heap
GC (Entry+HandleTable+trace+alocadores)→`rts-engine`; `rts-std` (backend) com **17 ns**
(+ runtime/ async); criado `rts-shared` (universal) com **13 ns pure-compute**.

**Progresso desta sessão (commits e8c929c→86c50b8f):**
- A ✅ `src/runtime/` async (async_rt+tokio_ctx) → rts-std (mesclado no mod `runtime`).
- B parcial ✅ thread + http_server → rts-std. ⏳ promise/parallel/crypto/time DEFERIDOS.
- C ✅ criado rts-shared + **13 ns** pure-compute: math num fmt hash mem ptr hint alloc path
  bigfloat buffer regex date. AOT smoke OK (regex/toFixed linkam do staticlib).
- Globais universais → rts-shared/src/globals/ (**16 globais**, 3 commits):
  - symbol; boolean bigint number url weakmap weakset weakref finalization_registry;
    regexp json json5 intl dom_exception global_this date.
  - Padrão: gc::handles→engine; string_pool/gc-surface no_mangle→`unsafe extern "C"{}`;
    ns-irmã em shared→`crate::<ns>`; register resolve via facade (codegen sem mudança).
  - rts-shared Cargo: rts-engine + regex + fancy-regex + indexmap + time(local-offset).

**rts-shared agora = 13 ns + 16 globais. Suite 1710/1710 em cada batch.**

✅ FEITO esta sessão (commit pendente): RELOCADOS os 2 pure-helpers do collector → rts-engine:
- `format_js_number(f64)->String` → `rts_engine::numfmt` (re-export em collector/string_pool).
- `stack_for_handle(u64)->Option<String>` + thread-local ERR_STACKS → `rts_engine::collector::
  err_stack` (re-export em collector/error; CLEAR chama `err_stack::record`). Slot de erro
  pendente (ERROR_SLOT) FICA no runtime. Suite 1710/1710; AOT smoke (error.stack+toString) OK.
- **Cluster destravado.** Próximo: mover cluster function+collections+proxy+error+string+reflect.
  (error tb usa gc::class_registry→engine + gc::error extern.)

⏳ DEFERIDOS (próxima sessão):
- **dataview** SKIP por design (nota memória #1378).
- **platform-divergent globais → rts-std** (step D): console(→io) timers fetch(→net)
  performance global_this? blob headers form_data readable_stream event_target
  message_channel abort. (abort/headers usam gc::error/handles; checar.)

**Grafo atual (acíclico):**
```
rts-engine   motor: registry/builder/abi + gc-mechanism(Traceable) + HEAP
             (Entry/HandleTable/trace + alocadores env/closure/instance/this_slot/
             tagged_raw/class_registry/fixed + payload types). deps TEMP:
             indexmap/regex/fancy-regex/serde_json/sha2/rustls.
rts-node     shims node:*. dep engine.
rts-shared   UNIVERSAL (roda em browser/wasm). 13 ns pure-compute: math num fmt hash
             mem ptr hint alloc path bigfloat buffer regex date + collections (Map/Vec).
             16 globais universais + cluster (error function proxy reflect string).
             gc_surface.rs = extern-decl dos no_mangle do collector (STRING_NEW/
             TO_STRING_HANDLE/ERROR_SET/ARRAY_ITERATOR_FN/GEN_SM_DRAIN), `safe fn`.
             deps engine+regex+fancy-regex+indexmap+time+anyhow+unicode-normalization.
             ⚠️ NUNCA depender de rts-std.
rts-std      backend, 17 ns: audio asio_audio io os env runtime(+async_rt+tokio_ctx)
             test net process sync atomic ffi fs tls thread http_server. deps
             engine+cpal+rustls+webpki-roots+tokio+actix-web+actix-rt.
rts-runtime  FACADE + resto (ns não-movidas + globals/ + collector/ gc-surface).
             re-exporta rts-shared/rts-std via namespaces/mod.rs. dep shared+std.
rts-codegen  lê o Registry. usa crate::namespaces::* (= rts-runtime facade).
```

## FALTA (em ordem)

### A. `src/runtime/` (async_rt + tokio_ctx) → rts-std  ✅ FEITO (commit 0a7043ab)
É `crate::runtime` (NÃO `namespaces`). Consumido por: `collector/generator`,
`collector/promise_slot`, `events`, `globals/text_encoding/instance`, `http_server`,
`parallel`, `promise`, `thread`. Move ele antes dos async-coupled.
- `git mv crates/rts-runtime/src/runtime crates/rts-std/src/runtime`
- `async_rt.rs` usa `crate::namespaces::gc::thread_registry` → `rts_engine::collector::thread_registry`.
- rts-std/lib.rs: `pub mod runtime;`. rts-std Cargo += `tokio` (rt-multi-thread/macros/sync).
- rts-runtime/lib.rs: `pub mod runtime;` → `pub use rts_std::runtime;` (facade → consumidores
  que FICAM no runtime, ex.: promise/events/text_encoding, seguem com `crate::runtime::async_rt`).
- Consumidores que JÁ estão no rts-std (nenhum ainda usa async_rt) ou que movem juntos: usam
  `crate::runtime` dentro do rts-std.

### B. Backend async/globals-coupled → rts-std
Depois do A. Cada um: `gc::handles`→`rts_engine::heap::handles`; gc-surface em runtime
(`string_pool`/`error`/`generator`/`promise_slot`) → `extern "C"{}` decl; `crate::runtime`→ok
(já em rts-std após A).
- **thread** ✅ FEITO (commit 999e7056). só tokio. `gc::thread_registry`→engine.
- **http_server** ✅ FEITO (commit 999e7056). actix-web/actix-rt + async_rt.
- **promise** ⏳ DEFERIDO. Coupling MAIOR que o gotcha previa: além de promise_slot, usa
  `globals::text_encoding::instance::{drain_microtasks,enqueue_microtask_*}` (pub fns Rust, NÃO
  externs) + `globals::timers::instance::pump_until` (pub fn) + `globals::function::ops` +
  `gc::generator`. Os enqueue_microtask_*/pump_until são pub fns com tipos Rust → não dá extern-decl.
  **Bloqueado até globals (text_encoding, timers) moverem.** Mover DEPOIS do step D.
- **parallel** ⏳ DEFERIDO. rayon + `globals::function::ops::invoke_array_callback` +
  `collections::map::*` + `gc::string_pool::TRUTHY`. Refs a globals/collections (pub fns) → bloqueado
  até globals/collections irem pra shared.
- **crypto** ⏳ DEFERIDO. sha2 + `gc::promise_slot::new_fulfilled` (retorna `Arc<PromiseSlot>` → não
  extern-able). promise_slot está em collector e é usado por 8 módulos (collector+blob/fetch/
  readable_stream/text_encoding/generator) → mover cedo = muita churn. Mover crypto junto com
  promise_slot DEPOIS, ou quando promise_slot for pro rts-std no step E.
- **time** ⏳ DEFERIDO. `globals::timers::instance::pump_until` (pub fn). Mover depois de timers→std.

### C. Criar `rts-shared` + mover pure-compute + globais universais
`rts-shared` dep só `rts-engine` (+ libs puras). **⚠️ NUNCA depender de rts-std** (senão não
roda no browser). Antes de cada move: `grep` que o arquivo não referencia ns/global que está
no std.
- **ns pure-compute → shared:** ✅ FEITO 13 (commit f417b050): alloc bigfloat buffer fmt hash
  hint math mem num path ptr regex date. (regex usa regex/fancy-regex wasm-OK; date std::time;
  string-returning ns já usam extern-decl p/ `__RTS_FN_NS_GC_STRING_NEW`.)
  - ⏳ DEFERIDOS deste batch: **json** (collections::vec + globals::function::ops + globals::proxy
    + gc::error → bloqueado até collections/proxy/error moverem). **collections** (usa
    globals::symbol+globals::proxy universais → mover quando ambos forem shared). **trace** (usa
    globals::error universal → shared). **events** (usa `crate::runtime`=async_rt → é BACKEND, vai
    pro rts-std não shared).
- **globais universais → shared:** String Number Boolean Array Object Map Set Symbol Error(+subs)
  JSON JSON5 RegExp Date Function WeakMap/Set/Ref FinalizationRegistry BigInt URL TextEncoder/Decoder.
  (As que NÃO tocam backend.)

### D. Globais platform-divergent → rts-std
console(→io) timers fetch(→net) performance global_this blob headers form_data readable_stream
event_target message_channel intl dom_exception dataview abort.

### E. Dissolver `rts-runtime`
Quando tudo migrar: rts-runtime vira facade fino (re-exporta shared+std+engine) OU some e o
codegen passa a `pub mod namespaces { pub use rts_std::*; pub use rts_shared::*; }`.
`collector/` (gc-surface: string_pool/error/generator/collector/stack/promise_slot + mod.rs
register) → vai pro rts-std (é a superfície backend do gc).

## PATTERN do move (provado, mecânico)
```
1. git mv crates/rts-runtime/src/namespaces/<ns> crates/<target>/src/<ns>
2. Fix refs no arquivo movido:
   crate::namespaces::gc::handles            → rts_engine::heap::handles
   crate::namespaces::gc::{thread_registry,global_roots,stack_map_registry,scan,debug}
                                             → rts_engine::collector::{...}
   crate::namespaces::gc::<env|closure|instance|this_slot|tagged_raw|class_registry|fixed>
                                             → rts_engine::heap::<...>
   crate::namespaces::gc::<string_pool|error|generator|promise_slot|collector>::FN
        (esses FICAM no runtime collector) → unsafe extern "C" { fn FN(...); } + call em unsafe
   crate::namespaces::<outra_ns>::FN  → extern decl SE FN é #[no_mangle]; senão move junto/depois
3. <target>/src/lib.rs: pub mod <ns>;   (+ Cargo deps do ns)
4. rts-runtime/src/namespaces/mod.rs: `pub mod <ns>;` → `pub use <target>::<ns>;`
5. Gate: cargo build --release + rts.exe test (1710). AOT se GC/classe.
```

## GOTCHAS / FINDINGS
- **AOT extern SOBREVIVE:** externs `#[no_mangle]` definidos em rts-engine OU rts-std entram no
  staticlib AOT do rts-runtime (PROVADO: closure+instance+classe AOT linkam/rodam). NÃO precisa
  force-link. (Resolve a dúvida antiga de WORKING.md L315.)
- **CICLO:** rts-shared NÃO pode depender de rts-std. Classificar por dep real (grep), não por nome.
- **promise_slot** (runtime collector) usa tokio+async_rt → backend. crypto/promise o referenciam
  por pub fn (`new_fulfilled` etc.) — extern-decl não serve. Opções: (a) mover promise_slot pro
  rts-std junto; (b) marcar `new_fulfilled` como `#[no_mangle] extern` e declarar; (c) mover
  crypto/promise por último, juntos com promise_slot. Recomendo (a)/(c).
- **DEADLOCK cargo:** `cargo check -p X` trava intermitente (lock, sem rustc). Workaround: usar
  `cargo build --release` em background; se travar (vários cargo.exe, 0 rustc):
  `taskkill //F //IM cargo.exe` + retry. Confirmar progresso com `tasklist | grep rustc` (>0 = ok).
- **AOT 2-step:** mexeu runtime/std → `cargo build --release -p rts-runtime` (staticlib) ANTES de
  `cargo build --release` (rts embeda) pra `rts compile` funcionar.
- **Bug GC PRÉ-EXISTENTE (abrir issue):** array de objetos de classe vivo atravessando ciclo de GC
  (>256 allocs) é coletado → UAF/hang. Causa: `Entry::Vec(Vec<i64>)` int/handle ambíguo → elementos
  não-traçados como roots. Repro: loop 2000× `live[i%8] = new N(...)` + ler campo. Funciona <256.

## COMANDOS
```bash
cargo build --release                              # ~4min (use background)
$env:RUST_BACKTRACE="full"; target/release/rts.exe test   # suíte 1710
cargo build --release -p rts-runtime               # staticlib (antes de AOT)
target/release/rts.exe compile -p f.ts out.exe; ./out.exe # AOT
```

## ORDEM RECOMENDADA p/ sessão nova
~~A~~ ✅ ~~B fácil (thread/http_server)~~ ✅ ~~rts-shared + 13 ns~~ ✅ ~~16 globais universais~~ ✅

~~1. Relocar pure-helpers do collector → rts-engine~~ ✅ FEITO (format_js_number→numfmt;
   stack_for_handle+ERR_STACKS→collector::err_stack). Cluster destravado.
~~2. Cluster → rts-shared~~ ✅ FEITO: error function proxy reflect string → globals/; collections
   → src/. gc_surface.rs (extern-decl `safe fn` dos no_mangle do collector); gc::handles/this_slot/
   class_registry → engine heap; gc::error::stack_for_handle → engine err_stack; gc::string_pool::
   format_js_number → engine numfmt; intra-cluster `crate::namespaces::X`→`crate::X`/`crate::globals::X`.
   5 fns bumped pub(crate)→pub (invoke_array_callback/invoke_fn_ptr_with_registry/handle_is_set_kind/
   handle_is_map_kind/mark_set_kind — consumidos por parallel/text_encoding/collector que FICAM no
   runtime). Cargo += anyhow + unicode-normalization. Suite 1710/1710; AOT smoke (Map/Set/Proxy/
   Reflect/bind/Error/string, JIT==AOT) OK. ⟶ destrava json(ns)/trace.
~~3. json(ns) + trace → shared~~ ✅ FEITO: json + trace → rts-shared/src/. read_string_handle
   (pub fn Option<String>, era collector/string_pool) relocado p/ rts_engine::heap::handles
   (re-export em string_pool). Cargo rts-shared += serde + serde_json(preserve_order) + json5.
   Refs: gc::handles→engine; gc::error::ERROR_SET→gc_surface; string_pool::format_js_number/
   read_string_handle→engine; globals/collections→crate::. Suite 1710/1710; AOT smoke (parse/
   stringify/array + error-stack via trace, JIT==AOT) OK.
   ⚠️ BUG PRÉ-EXISTENTE (NÃO regressão — confirmado em worktree do pai 6f3e6c85): `JSON.stringify
   (obj, null, 2)` (pretty, 3-arg) TRAVA em `rts run`/AOT top-level (MIR on E off). Funciona em
   `rts test` (suite json_stringify_pretty 3/3 verde). Abrir issue. Não bloqueia o move.
4. **Platform-divergent globais → rts-std (step D):** PARCIAL.
   ~~console performance headers form_data event_target message_channel abort~~ ✅ FEITO →
   rts-std/src/globals/. Só gc::handles→engine + extern-decl (ERROR_SET no abort; INVOKE_AUTO já
   era extern p/ rts-shared function/ops). Cargo rts-std += indexmap. Suite 1710/1710; AOT smoke
   (console/performance/Headers/FormData/EventTarget/AbortController, JIT==AOT) OK.
   ⏳ DEFERIDOS (blocked por pub-fn não-extern): **timers** (text_encoding::drain_microtasks),
   **fetch** (text_encoding::enqueue_microtask_* + promise_slot::* + ureq dep), **blob** +
   **readable_stream** (promise_slot::new_fulfilled + flate2 dep). Movem quando text_encoding +
   promise_slot saírem (steps 5/6). NB: abort() no-arg tem arity strict pré-existente (spec pede
   reason); `abort("x")` ok.
5. **events + time → rts-std.** PARCIAL.
   ~~events (ns + global EventEmitter)~~ ✅ FEITO → rts-std/src/events + globals/events. Só
   gc::handles→engine; ns events usa crate::runtime::async_rt (já em std); global usa rayon
   (Cargo rts-std += rayon). Suite 1710/1710; AOT smoke (EE on/emit/listenerCount, JIT==AOT) OK.
   NB pré-existente: listener com closure CAPTURANTE crasha (EE_EMIT transmuta p/ extern fn(f64);
   precisa fn nomeada não-capturante — suite usa esse padrão).
   ⏳ **time** DEFERIDO: usa globals::timers::pump_until (timers ainda no runtime, bloqueado por
   text_encoding). Move com timers.

   ⚠️ DESCOBERTA (mapeamento step 6): text_encoding + promise_slot NÃO movem isolados nem só com
   consumers — text_encoding::drain_microtasks chama `gc::generator::async_sm_resume` (pub fn no
   collector/runtime). rts-std não pode depender de rts-runtime (ciclo). Logo o GRUPO async
   (text_encoding + promise_slot + promise + generator + crypto + blob + readable_stream + timers +
   time) tem que mover JUNTO pro rts-std — efetivamente fundir steps 5-tail/6/7. generator/string_pool/
   error/collector (gc-surface) saem do collector p/ rts-std no mesmo movimento. rts-std += rts-shared
   dep (collections::map + function::ops::invoke_fn_ptr_with_registry, ambos em shared). Sem ciclo
   (shared não depende de std). É o grande move final — fazer em sessão dedicada.
6. **promise + parallel + crypto + promise_slot** juntos (globals+promise_slot resolvidos).
   promise_slot sai do collector. Gate + AOT.
7. **Dissolver rts-runtime; collector/ → rts-std.** Gate final + AOT + atualizar WORKING.md.

NB dataview: SKIP (design, nota memória #1378). NB global_this já está em shared (universal).
