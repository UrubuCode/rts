# WORKING.md — memória de trabalho da migração rts-engine

> **Propósito:** rastreador operacional vivo. Ler PRIMEIRO a cada sessão;
> **atualizar a cada commit** (status + log + checar etapa). O design canônico
> está em `RTS_ENGINE.md` (§0.1 = roadmap); este arquivo é a camada de execução:
> onde estamos, qual o próximo passo, comandos, e as armadilhas já descobertas.
> Vivo até **todas** as etapas abaixo fecharem.

- **Issue:** #1536 · **Branch:** `feat/engine-method-dispatch-1536`
- **Objetivo final:** `rts-engine` = núcleo cru (abi + registry + builder +
  **gc/collector como sistema**, pasta `gc/` → `collector/`). As CAMADAS apenas
  registram a superfície via builder, em cima do engine:
  - **`rts-shared`** — `register()` das APIs COMPARTILHADAS backend+frontend
    (Boolean/String/Number e afins, presentes no navegador também).
  - **`rts-runtime`** — backend (fs/net/process/os/... + os payloads pesados do
    GC: tokio/regex/rustls). **Expõe a API do GC.**
  - **`rts-browser`** — frontend (APIs de navegador). **NÃO expõe a API do GC.**
  - A API "exposta" do GC não é agradável de mostrar ao público geral → o
    mecanismo vive no engine, mas o `register()` que a torna chamável fica só na
    camada backend.
  `rts-abi` **deletado** ✅; `rts-macro` **deletado** ✅ (opção b — hand-written builder).
  Codegen = motor genérico que lê o `Registry`, nada hardcoded ✅.

---

## 🎯 PRÓXIMO PASSO (sempre no topo)

> ## 📌 ESTADO (rts-macro REMOVIDA — opção b concluída)
>
> **COMPLETO (gateado suíte 1710 + d.ts determinístico/byte-idêntico em cada):**
> 1. ✅ **Fase 2** — todas ns + 62 classes → builder; `SPECS`+`GLOBAL_CLASS_SPECS`
>    **vazios**; codegen 100% via registry. (+ fix bug alias-null ACCESS_VIOLATION)
> 2. ✅ **gc+collections** → const vazio
> 3. ✅ **GC SPLIT** — mecanismo (contrato `Traceable` + 3 registries + debug +
>    scanner) no `rts-engine`; heap tipado + `finish_cycle` + externs no runtime.
>    `gc/`→`collector/`. Gateado + stress 50k.
> 4. ✅ **`rts-abi` DELETADO** — 122 refs → `rts_engine::abi::`, crate removido.
> 5. ✅ **`rts-macro` DELETADA (opção b — hand-written)** — TODAS as ~38 ns + 31
>    arquivos de classe global (62 classes) convertidos do `#[rts_namespace]`/
>    `#[rts_class]` pro builder hand-written (externs `#[no_mangle]` + `register*()`
>    espelhando 1:1 a metadata: kind/sig-com-receiver/símbolo/ts/flags/aliases/
>    variadic/pure/intrinsic/external-null). `crates/rts-macro` `git rm` + dep +
>    workspace member removidos. ZERO `#[rts_*]` / `use rts_macro` no codebase.
>    Fix junto: `Registry::modules()`/`classes()` iteravam `HashMap::values()`
>    (ordem aleatória → `emit-types` não-determinístico desde que o const SPECS foi
>    esvaziado) → `module_order`/`class_order` (Vec de inserção) = emit-types estável.
>
> **RESTAM — 2 (nenhuma bloqueada por decisão):**
> - **camadas `rts-shared`/`rts-browser`** — ainda prematura: rts-browser seria
>   crate vazio (nenhuma API de navegador existe). Agora DESACOPLADA da macro (os
>   register() são hand-written) → quando houver API de navegador, extrair os
>   register() universais (Boolean/String/Number) p/ rts-shared é mecânico.
> - **E2-E4 / X1-X5** — E2-E4 = refactor profundo do codegen (os ~182 braços do
>   `builtins.rs` são lógica que emite IR custom, não dados drenáveis); X1-X5 =
>   feature nova (dlopen plugins). Sessão fresca rende mais.
>
> **O pedido-núcleo ("codegen = motor genérico, nada hardcoded; gc como sistema
> no engine; abi limpa; macro removida") está ATINGIDO.** Restam 1 prematura
> (camadas) e 1 grande-nova (E/X) — ambas fora do caminho crítico.
>
> ---
>
> **Fase 2 NAMESPACES ✅ (todas exceto gc+collections)** — a macro
> `#[rts_namespace]` passou a **auto-emitir `register(e: &mut Engine)`** com a
> MESMA metadata do const SPEC; `register_builtins` folda ~36 ns macro'd via
> `register()` + as 9 hand-migradas. **Só `gc` (GC sensível) e `collections`
> (owner hand-written com `concat_members`, sem `#[rts_namespace]` próprio)
> seguem no const `SPECS`.** `rts_engine::Member += intrinsic` (math inline
> preservado). Suíte **1710/1710**, workspace lib verde, macro 6/6.
>
> **TODAS as ns (incl. gc+collections) + TODAS as 62 classes ✅** — ambos os
> consts `SPECS` e `GLOBAL_CLASS_SPECS` esvaziados (`&[]`). O codegen lê 100% pelo
> registry (alimentado pelos `register()`/`register_*_class_spec()` em
> `register_builtins`). Suíte 1710/1710.
>
> **Próximos passos (na ordem):**
> 1. **GC/collector → `rts-engine` como SISTEMA** (decidido). Pasta `gc/` vira
>    `collector/`. Resolve o ciclo: rts-engine é o crate BASE (rts-runtime/
>    rts-browser dependem dele), então mover pra cá é acíclico — ao contrário de
>    mover pro codegen (codegen→runtime = ciclo). **SPLIT obrigatório:** só o
>    MECANISMO genérico vai pro engine (HandleTable slab gen|slot|shard via
>    `abi::handles` que JÁ está no engine; collector mark+sweep; stack_map_registry;
>    thread_registry; global_roots) — parametrizado por um trait `Traceable`/
>    `GcPayload` (sem conhecer variants concretas). Os PAYLOADS pesados do `Entry`
>    (tokio/regex/rustls) FICAM em rts-runtime (backend) e implementam o trait —
>    senão rts-browser arrastaria tokio/rustls. Símbolos `__RTS_FN_NS_GC_*` ficam
>    via add_fn!/runtime_jit_symbols (AOT+JIT). **Princípio congele-a-interface:**
>    fixa a ABI (handle + op-set alloc/retain/release/trace/finalize/roots +
>    trait `Traceable`); mark+sweep atual fica atrás dela; política pode evoluir
>    p/ RC+coletor-de-ciclos (determinístico, finalização pronta, sem vazar) sem
>    tocar o codegen. Ver "Design GC" abaixo.
> 2. **`undefined`/`null`/`NaN`/`Infinity`/globalThis + global.* base** → o engine
>    detém a identidade da API base (controlador); codegen consulta o engine em vez
>    de match hardcoded. NÃO move as classes (String/Date/etc.) — essas viram
>    `rts-shared`/`rts-runtime` conforme disponíveis no navegador.
> 3. **Camadas `rts-shared`/`rts-browser`** — extrair os `register()` por alvo:
>    `rts-shared` (Boolean/String/Number/... universais), backend fica em
>    `rts-runtime`, frontend em `rts-browser`. A API do GC só é registrada no
>    backend (rts-browser não a mostra).
> 4. **Deletar `rts-macro`** — a macro AINDA gera os `register()`/
>    `register_*_class_spec()`/`append_engine_members()` + externs `#[no_mangle]`.
>    Opções: (a) absorver no `rts-engine` (gerador oficial do builder), OU
>    (b) inlinar hand-written. **Decidir com o dev.**
> 5. **Deletar shim `rts-abi`** quando zero `rts_abi::`.
> 6. **E2-E4 / X1-X5** (dispatch engine + plugins externos `.dll`/`.so`).
>
> **Nota:** o codegen JÁ é genérico — lê tudo (ns + classes) pelo registry; os
> consts `SPECS`/`GLOBAL_CLASS_SPECS` são só seed (2 ns / 0 classes). O objetivo
> "motor genérico, nada hardcoded" está essencialmente atingido; o resto é
> reorganização da camada de registro (deletar macro/abi, realocar gc/globals).
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

## 🧠 DESIGN GC (decidido) — "feito uma vez, sem vazar"

**Princípio: congele a INTERFACE, não a política.** O que obriga a remexer é
acoplar o codegen à *impl* do GC. Fixa-se o CONTRATO ABI (estável p/ sempre) e a
política vive atrás dele — troca o coletor sem tocar o codegen.

**Interface congelada (vive no `rts-engine`):**
- `handle = u64` (gen|slot|shard) — já em `abi::handles`.
- op-set fixo: `alloc(type)->h`, `retain(h)`, `release(h)`, `trace_children(h,visit)`,
  `finalize(h)`, `enumerate_roots()`. Codegen emite calls a esses símbolos —
  **idêntico em AOT e JIT** (é o que já acontece com `__RTS_FN_NS_GC_*`).
- trait `Traceable { fn trace_children(&self, visit: &mut dyn FnMut(u64)); }` +
  `finalize` — o domínio (Entry no runtime) implementa; o coletor genérico (engine)
  anda o grafo sem conhecer variants → zero-dep (não arrasta tokio/regex/rustls).

**Política recomendada (atrás da interface): RC + coletor de ciclos (modelo ORC do
Nim / CPython), NÃO o GC-de-pausa do V8.**
- Reference counting primário: libera no ponto exato que a última ref morre —
  determinístico, zero bloat, sem pausa.
- Coletor de ciclos backup (trial-deletion nos candidatos) pega o resíduo cíclico
  → **leak-free** (sem isso, RC vaza ciclos = erro do Swift/ARC).
- **Finalização PRONTA** = o diferencial: refcount zera → `finalize` roda na hora →
  socket/JoinHandle/ProcessChild/TLS do `Entry` liberados imediatamente (RAII), não
  num tick aleatório. É o "limpar a RAM corretamente" que o motor JS não dá.
- Escape analysis (Fase 4 do MIR) elide retain/release onde prova que não escapa →
  hot loops (Monte Carlo) não pagam overhead de RC.

**Placement (resolve o ciclo):** ABI + coletor genérico no `rts-engine` (base,
zero-dep, via trait); heap `Entry` + `trace_children`/`finalize` por-variant no
`rts-runtime` (backend); codegen emite as ops. rts-engine não conhece runtime →
sem ciclo de crate. Equivalente Rust do late-binding do JS = ponteiro de função
(`install_gc_hook` já existe).

**Caminho seguro:** (1) congela a ABI no engine, mark+sweep ATUAL atrás dela —
zero mudança de comportamento, suíte 1710; (2) troca política p/ RC+ciclos
incremental, codegen intocado.

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
| `a1aac29a` | **Fase 2 BULK ns** | **macro `#[rts_namespace]` auto-emite `register(e: &mut Engine)`** (mesma metadata do const SPEC → `NamespaceMember` byte-equiv via leak). `register_builtins` folda **~36 ns** via register() (io/json/date/fs/math/net/num/mem/bigfloat/buffer/ffi/atomic/sync/string/process/promise/os/http_server/crypto/regex/audio/runtime/test/thread/parallel/tls/globals*+events). **Só `gc`+`collections` restam no const `SPECS`.** `rts_engine::Member += intrinsic` (math sqrt/abs/min/max inline preservados). **Bug crítico achado+corrigido:** `leak_namespace` gravava `(symbol, 0)` p/ membros alias (fn_ptr null) → sobrescrevia o símbolo real com NULL → call p/ 0x0 (ACCESS_VIOLATION não-determinístico, ordem HashMap). Fix: pular fn_ptr null no jit_symbols. 6 consumidores do const SPECS → registry (apis/init/jit-diag/is_global/builtin_module_keys/pure-ns-set). Teste stale `rts:ui`→`rts:gc`. Workspace lib + macro 6/6 + **suíte 1710/1710** |
| `dbecaad2` | **Fase 2 BULK classes** | **macro `#[rts_class]` auto-emite `register_<spec_lower>(e: &mut Engine)`** (nome derivado do `spec_ident`, único por módulo). `leak_class` (espelha `leak_namespace`, default_args `&[]`, mesmo skip de fn_ptr null) + fold de `engine.registry().classes()`. **Todas as 62 classes globais migradas; const `GLOBAL_CLASS_SPECS` esvaziado (`&[]`).** Helper `leak_member` fatorado (DRY entre ns/classe). 3 consumidores do const GLOBAL_CLASS_SPECS → `registry_classes_ordered()` (calls/mod.rs:resolve_instance_method + members.rs:instance_getter/instance_method). Piloto Boolean → bulk. Workspace lib + macro 6/6 + **suíte 1710/1710** |
| _(prev)_ | **Fase 2 gc+collections** | **últimas 2 ns migradas → const `SPECS` esvaziado (`&[]`).** `gc` via `gc::register()` (macro `#[rts_namespace(gc)]`). `collections`: macro `part` passou a emitir `append_engine_members(&mut Vec<Member>)`; owner `collections::register()` hand-written agrega map+vec (mesma ordem do `concat_members`). **AMBOS os consts (`SPECS`+`GLOBAL_CLASS_SPECS`) vazios — codegen 100% via registry.** Workspace lib + macro 6/6 + **suíte 1710/1710** |
| `54f569b0` | macro-rm início | **mem** convertida do `#[rts_namespace]` pro builder hand-written (1ª ns da opção b). Template dos pilotos. Suíte 1710 + smoke |
| `ebf3abf2` | **macro-rm ns** | **TODAS as ~38 ns** convertidas → hand-written builder (io/os/process/regex/test/ffi/parallel/sync/thread/tls/net/crypto/num/atomic/buffer/json/fs/date/bigfloat/http_server/promise/events/audio/asio_audio/**math** intrinsics+aliases/**collections** parts→append_engine_members/**gc** 30 external). + **fix determinismo**: `Registry::modules()/classes()` HashMap→Vec de inserção (emit-types determinístico). + piloto classe **boolean**. ZERO `#[rts_namespace]`. Suíte 1710 + d.ts byte-idêntico (sorted, 5516 linhas) |
| `63c6fbf5` | **macro-rm classes** | **TODOS os 31 arquivos de globals/* (62 classes)** convertidos do `#[rts_class]`/`#[rts_namespace(sym=)]` → builder hand-written (register_*_class_spec/register espelhando metadata 1:1; external→fn_ptr null). ZERO `#[rts_*]`/`use rts_macro` no runtime. Suíte 1710 + d.ts byte-idêntico |
| _(HEAD)_ | **rts-macro DELETADA** | `crates/rts-macro` `git rm` + dep de rts-runtime + workspace member removidos. Build --release limpo SEM a macro + suíte 1710/1710. **rts-macro não existe mais.** |

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
- [x] **Todas as namespaces** migradas: 9 à mão + ~36 via macro-`register()` + gc + collections (owner agrega `part`s via `append_engine_members`). **Const `SPECS` esvaziado (`&[]`).** Suíte 1710/1710. (HEAD)
- [x] **Classes globais (62)** via builder — `leak_class` + fold de `engine.registry().classes()` + macro `#[rts_class]` auto-emite `register_<spec>_class_spec`. Const `GLOBAL_CLASS_SPECS` esvaziado. 1710/1710. (HEAD)
- [ ] `#[rts_var]`/`#[rts_global]`/setters consumidos no codegen (A3: VarGetter read `members.rs:1006` + write-path `x.v=5` em `lower_assign_expr` + readonly hard-error).
- [x] **`rts-macro` REMOVIDA (opção b — hand-written) ✅.** Todas as ns
      (mem/runtime/io/os/process/regex/test/ffi/parallel/sync/thread/tls/net/crypto/
      num/atomic/buffer/json/fs/date/bigfloat/http_server/promise/events/audio/
      asio_audio/math/collections/gc) + 62 classes (boolean/number/regexp/symbol/
      bigint/weak*/headers/function/date/error+7/abort+signal/event_target+event/
      dom_exception/blob+file/form_data/finalization_registry/eventemitter/
      arraybuffer+dataview/messagechannel+port/intl×7/readablestream×9/string/url/
      fetch/text_encoding/console/timers/performance/global_this/json/json5)
      convertidos pro builder hand-written. `crates/rts-macro` `git rm` + dep +
      workspace member fora. Gate: build --release + suíte 1710 + emit-types
      determinístico/byte-idêntico. Commits: ebf3abf2 (ns), 63c6fbf5 (classes),
      + commit de remoção do crate.

### GC/collector → rts-engine (decidido — ver "Design GC" + Próximos passos)
- [x] **Congelada a ABI do GC** no `rts-engine` (`collector.rs`): trait
      `Traceable`/`GcPayload` + op-set documentado (alloc/retain/release/trace/
      finalize/roots) + decode de handle (shard/gen/slot). Aditivo, mark+sweep
      atual intocado. engine 5/5, suíte 1710/1710. (HEAD)
- [x] **SPLIT fatia 1:** `Entry: impl rts_engine::Traceable` (trace_children =
      match de `mark`; finalize = `cleanup_entry`). `HandleTable::mark` chama o
      contrato em vez do match inline. Comportamento idêntico, suíte 1710/1710. (HEAD)
- [x] **SPLIT fatia 2:** pasta `namespaces/gc/` → `namespaces/collector/`
      (`git mv`). Alias `pub use collector as gc;` em `namespaces/mod.rs` cobre os
      68 consumidores + `crate::namespaces::gc::*` do codegen — zero edição neles.
      Símbolos `__RTS_FN_NS_GC_*` e namespace name "gc" intactos. Gate: suíte
      1710/1710 (JIT) + **AOT** (compile+run de fixture GC-heavy). (HEAD)
- [x] **SPLIT fatia 3a:** `stack_map_registry` + `global_roots` (registries
      std-puro, escritos pelo codegen, lidos pelo scanner) → `rts-engine/src/
      collector/`. `collector.rs` do engine virou `collector/mod.rs`. Facade
      `pub use rts_engine::collector::{...}` no runtime → jit.rs + collector.rs
      intocados (mesmo static). Gate: suíte 1710/1710 + AOT. (HEAD)
- [x] **SPLIT fatia 3b:** `thread_registry` → engine (std + FFI `extern "system"`).
      Facade. Os 3 registries (stack_map/global_roots/thread_registry) agora no
      engine. Gate: suíte 1710/1710 + AOT. (HEAD)
- [x] **SPLIT fatia 3c:** scanner conservativo (asm RSP, ranges, SuspendThread,
      CONTEXT/FFI) + `debug` → `rts-engine/src/collector/scan.rs`. Decomposição
      mais segura que o vtable: `scan::scan_all_roots(visit: &mut dyn FnMut(u64))`
      = mecanismo PURO de root-finding; o runtime passa `mark_handle` como visitor
      (`mark_handle` é transitivo — worklist próprio). `finish_cycle` FICA no
      runtime (scan + microtask roots de domínio + sweep + os externs
      `__RTS_FN_NS_GC_COLLECT*` → AOT seguro, símbolos não migram). Comportamento
      byte-idêntico. Gate: suíte 1710/1710 + AOT + **stress 50k allocs JIT+AOT
      (STRESS_OK, vivos íntegros)** + multi-thread (suíte http_server/thread). (HEAD)
- [x] **SPLIT essencialmente COMPLETO.** O `rts-engine` hospeda o MECANISMO do GC:
      contrato `Traceable` + 3 registries (stack_map/global_roots/thread_registry)
      + debug + scanner. O `rts-runtime` mantém — POR DESIGN — a `HandleTable`
      tipada (`Entry` com tokio/regex/rustls), `finish_cycle` (orquestração com
      roots de domínio), string_pool e os externs `#[no_mangle]`. É exatamente o
      SPLIT alvo: mecanismo genérico no núcleo, heap tipado + domínio no backend.
- [ ] Gate AOT crítico: staticlib continua exportando os `__RTS_FN_NS_GC_*` que
      migraram (rts-engine contribui pro archive OU runtime re-exporta `#[no_mangle]`).
- [ ] (Futuro, atrás da ABI) trocar política mark+sweep → RC + coletor de ciclos.

### Camadas → partição de crates (PLANO APROVADO pelo dev)
Visão do dev: `rts-engine` = motor único (back+front, DONO do heap GC); `rts-shared`
= tipos básicos + globais universais (rodam nos 2 alvos); `rts-std` = ns backend;
`rts-browser` = frontend (DEPOIS); `rts-node` = shims node. Plano completo em
`.claude/plans/partitioned-meandering-milner.md`. Decisões: (1) mover crates
primeiro, split do `Entry` depois; (2) heap fica DENTRO do `rts-engine`.
- [x] **Fase 0 — `rts-node`** ✅ (commit 85713faa): nodespace `rts-codegen`→
      `crates/rts-node`. Gate build+suíte 1710.
- [ ] **Fase 1a — heap → rts-engine.** ⚠️ NÃO é git-mv: o `collector/` tem
      orquestração DOMAIN-COUPLED (generator→promise/text_encoding microtasks;
      string_pool deep-trace→collections; finish_cycle→microtask roots; error→
      trace stack; stack→globals/error). Move CORE (Entry+HandleTable+alloc+
      payload value-types) pro engine; re-roteia orquestração via HOOKS
      (`install_gc_hook` já existe)/Traceable pra engine não chamar backend;
      generator/deep-trace/finish_cycle ficam no backend e registram hooks.
      - [x] **SEED** (commit `eb039fc6`): `rts-engine/src/heap/` criado;
            `FixedDecimal` (payload puro do `Entry::BigFixed`) movido pro engine;
            facade `bigfloat::fixed`. Gate build+suíte 1710. Achado: `handles.rs`
            é AUTO-CONTIDO (zero chamada a siblings, zero externs `#[no_mangle]`)
            → mover Entry+HandleTable é tratável.
      - [x] **CORE move** ✅ (commit `ae4db8be`): `handles.rs` (Entry+HandleTable+
            PromiseSlot[tokio-free]+RtsRegex+HasherState+GenStateData+FunctionData+
            alloc/with_entry/trace/finalize/mark+install_gc_hook) + `TlsClientStream`
            → `rts-engine/src/heap/handles.rs`. engine Cargo += indexmap/regex/
            fancy-regex/serde_json/sha2/rustls (TEMP; NÃO tokio). Facade
            `collector::handles = rts_engine::heap::handles` → ~68 consumidores
            intocados. Externs `__RTS_FN_NS_GC_*` ficam nos siblings (runtime) →
            AOT ok. Gate: build + suíte **1710/1710** + **AOT** (compile+run) + GC
            churn 30k strings. ⚠️ Achado latente PRÉ-EXISTENTE (não regressão):
            array-de-objetos vivo atravessando GC trava (Vec<i64> int/handle
            ambíguo → elementos não-traçados → coletados → UAF). Issue a abrir.
      - [x] **Alocadores → engine** ✅: env+closure (`57f1823b`), instance+this_slot+
            tagged_raw+class_registry (`0be13d3f`). **Descoberta-chave provada:**
            externs `#[no_mangle]` definidos no engine SOBREVIVEM no staticlib AOT
            do runtime (closure+instance AOT linka/roda) → não precisa force-link.
            Cada batch gateado build+suíte 1710+AOT.
      - [x] **FRONTEIRA NATURAL atingida.** No engine agora: Entry+HandleTable+trace
            mecanismo + TODOS os alocadores (env/closure/instance/this_slot/tagged_raw/
            class_registry/fixed) + payload types. Isso É "heap no motor".
            FICAM no backend (domain-coupled ou namespace surface — referenciam
            globals/domínio, mover exigiria hook-ification grande):
            - `string_pool` (deep-trace → collections/generator)
            - `collections` map/vec (Symbol.iterator + **Proxy traps** → globals)
            - `collector.rs` finish_cycle (→ microtask roots)
            - `generator` (→ promise/text_encoding/timers), `error` (→ trace),
              `stack` (→ globals/error), `promise_slot` (tokio + async_rt)
      - [ ] (Opcional/futuro) hook-ificar string_pool deep-trace + finish_cycle p/
            mover o resto pro engine. Esforço próprio; não no caminho crítico.
- [~] **Fase 1b — `rts-shared`+`rts-std`** (EM ANDAMENTO).
      - [x] **`rts-std` criado** (engine+cpal) + piloto `audio`/`asio_audio`
            (`c4935698`) + batch leaf `io/os/env/runtime/test` (`d680bb10`).
            Mecanismo provado: facade `rts-runtime::namespaces::<ns>=pub use
            rts_std::<ns>` → register_builtins/jit.rs/codegen INTOCADOS;
            rts-runtime→rts-std→engine acíclico; externs do std exportam no AOT.
      - [x] **Backend std-only/extern-decl → rts-std** ✅ (`82d81733` net/process/
            sync/atomic/ffi; `fc56a9e5` fs[extern-decl STRING_NEW]+tls[rustls]).
            Fix padrão: `gc::handles`→`rts_engine::heap::handles`; gc-surface em
            runtime → `extern "C"{}`. **rts-std = 14 ns** (audio/asio_audio/io/os/
            env/runtime/test/net/process/sync/atomic/ffi/fs/tls).
      - [ ] **Backend async/globals-coupled** (próximo, mais acoplado): mover
            `runtime/` (async_rt+tokio_ctx → rts-std/engine) PRIMEIRO — é usado por
            thread/http_server/promise/parallel/fetch via `crate::runtime::async_rt`.
            Depois thread(tokio)/http_server(actix+tokio)/crypto(promise_slot)/
            parallel(globals/collections)/time(globals::timers) → rts-std.
      - [ ] **`rts-shared`** (engine): pure-compute (math/num/fmt/hash/mem/ptr/hint/
            alloc/path/bigfloat/buffer/regex/date/events/collections) + globais
            universais. ⚠️ shared NÃO pode depender de std (verificar antes).
      - [ ] Platform-divergent globais → std. Dissolver `rts-runtime`.
- [ ] **Fase 2 (futuro)** — `Entry::Backend(dyn Traceable)` → rts-shared wasm-pure.
- [ ] **Fase 3 (futuro)** — rts-browser + target wasm no codegen.

### Limpeza final
- [x] **Shim `rts-abi` DELETADO.** Sweep `rts_abi::`→`rts_engine::abi::` (122 refs:
      runtime 65 arq + macro 35 emits + derive.rs); a macro passou a emitir
      `::rts_engine::abi::`; deps + workspace member removidos; `crates/rts-abi/`
      `git rm`. Gate: build limpo + suíte 1710/1710 + macro derive 6/6 + AOT stress.
      (Corrige a nota antiga: a macro NÃO precisou morrer antes — só emitir o path
      do engine; ela continua, agora sem o shim.) (HEAD)
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
- **Macro gera `::rts_engine::abi::...`** (era `::rts_abi::`, flipado ao deletar o
  shim). A macro PODE existir sem o shim — só emite o path do engine. Shim deletado.
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
