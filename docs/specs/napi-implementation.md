# N-API — Plano de implementação (doc vivo de acompanhamento)

> **Status:** Fase 0+1+2 + classes + ArrayBuffer + async-work + threadsafe +
> **BigInt real** + **Promise↔async** ✅ — **158/159 fns implementadas** (só
> `napi_module_register` legado resta). Paridade Node v22: crc32, xxhash
> (+classe), uuid, **bcrypt hashSync** (60), ArrayBuffer (`fill(100)`=22532),
> async-work callback (`compute(7)`=49), BigInt (round-trip exato >2^53),
> Promise napi (`resolve(42)`→`.then`→42).
>
> **Issues de rastreamento:**
> - [#1547](https://github.com/UrubuCode/rts/issues/1547) — tracking geral do N-API
> - [#1548](https://github.com/UrubuCode/rts/issues/1548) — APIs do **engine** necessárias (para @drysius): `Entry::ArrayBuffer` ptr estável, slot escondido p/ `wrap`, ganchos do event loop (#207), BigInt real (#219)

## Cobertura atual (124/159 fns)

| Categoria | Status |
|---|---|
| Loader, valores, strings, objects, props, exceções, external | ✅ implementado |
| functions/callbacks, handle scopes, references | ✅ |
| type checks, coerce, Buffer, Date, Symbol, BigInt(i64), wrap/unwrap | ✅ Fase 2 |
| Promise/deferred, type tags, finalizer, syntax errors, make_callback | ✅ Fase 2c/2d |
| **classes nativas** (`napi_define_class` + `new addon.X()` + `inst.method()`) | ✅ |
| **ArrayBuffer/TypedArray/DataView** (`Entry::ArrayBuffer` ptr estável, #1548) | ✅ |

### 1 stub restante — fora de escopo

| Fn | Qtd | Razão |
|---|---|---|
| ~~arraybuffer/typedarray/dataview~~ | ~~11~~ | ✅ FEITO (`Entry::ArrayBuffer` ptr estável) |
| ~~async_work + callback scopes~~ | ~~9~~ | ✅ FEITO síncrono |
| ~~threadsafe functions~~ | ~~8~~ | ✅ FEITO **inline** (limitação cross-thread, ver abaixo) |
| ~~bigint uint64/words~~ | ~~4~~ | ✅ FEITO (`Entry::BigInt` real, #219) |
| ~~uv_event_loop + cleanup hooks~~ | ~~3~~ | ✅ FEITO (uv_loop ptr fake; hooks no-op) |
| **`napi_module_register`** | **1** | registro **legado** acoplado a V8 — fora de escopo (addons N-API usam `napi_register_module_v1`) |

**Limitações conhecidas (dependem do event loop real, #207):**
- **threadsafe function** roda `call_js_cb` **inline** (na thread que chamou),
  não posta na thread JS. Addons que chamam a TSFN de **outra thread**
  (ex.: `bcrypt.hash()` async, que usa threadpool interno do napi-rs) crasham —
  o callback precisa rodar na thread JS via fila drenada pelo loop.
  `bcrypt.hashSync()` funciona.
- **`napi_get_uv_event_loop`** devolve um `uv_loop_t` opaco **fake** (não-nulo
  estável). Addons que só repassam o loop funcionam; os que chamam `uv_*` direto
  precisam do **shim libuv sobre tokio**.
- **Promise↔async** funciona para resolução **na mesma thread**: `napi_create_
  promise` + `resolve_deferred` + `.then()` do TS (validado `resolve(42)`→42).
  O `.then` é roteado p/ `__RTS_FN_GL_PROMISE_THEN`; o valor é desempacotado
  (FloatPrim→bits f64) antes do resolve.

Os stubs degradam graciosamente (falha, não crash). Tudo na export table para o
`napi-sys` `load_all()` completar.

## Mapa dos módulos (`crates/rts-napi/src/`)

| Arquivo | Conteúdo |
|---|---|
| `lib.rs` | `napi_get_version`/`node_api_module_get_api_version_v1` + `force_link()` (retenção de todos os símbolos no bin) + `napi_property_descriptor` |
| `types.rs` | tipos ABI: `napi_value`/`napi_env`/`napi_ref`/... (`repr(transparent)`), `napi_status`/`napi_valuetype` (`repr(C)`, ordem fixa), `napi_callback`/`napi_finalize` |
| `env.rs` | `RtsNapiEnv` (api_version + pending_exception + scopes + refs); `value_from_handle`/`handle_from_value` |
| `loader.rs` | `__RTS_FN_NS_NAPI_LOAD_ADDON` (libloading + `napi_register_module_v1` + cache por path) |
| `values.rs` | escalares (double/int32/uint32/int64/bool) + sentinelas + `napi_typeof` |
| `strings.rs` | `create/get_value_string_utf8` (protocolo 2 passagens) |
| `objects.rs` | object/array/props/elementos + `get_global`/`is_array`/`instanceof` |
| `functions.rs` | `create_function`/`get_cb_info`/`call_function` + trampolim de callback (`__RTS_FN_RT_NAPI_DISPATCH_CALLBACK`) + `invoke_napi_callback` helper |
| `scopes.rs` | handle scopes (chunks `[u64;32]` em Box como GC roots via `global_roots`) |
| `references.rs` | `napi_ref` strong (root) / weak |
| `errors.rs` | throw/create error + exceção pendente no env |
| `externals.rs` | `napi_create/get_value_external` (`Entry::NapiExternal`) |
| `classes.rs` | `napi_define_class`/`new_instance` + `__RTS_FN_RT_NAPI_NEW_INSTANCE`/`INVOKE_METHOD` |
| `phase2.rs` | type checks, property checks, coerce, Buffer, Date, Symbol, BigInt(i64) |
| `phase2b.rs` | strings latin1/utf16, wrap/unwrap, instance-data, version, property keys |
| `phase2c.rs` | Promise/deferred, type tags, finalizer, coerce_to_string, syntax errors, cleanup hooks |
| `phase2d.rs` | external strings, make_callback, is_sharedarraybuffer |
| `arraybuffer.rs` | ArrayBuffer/TypedArray/DataView sobre `Entry::ArrayBuffer` (engine, #1548) — ptr estável, views via Map |
| `async_work.rs` | async work **síncrono** (queue roda execute+complete), async_init/destroy, callback scopes, post_finalizer, cleanup hooks, uv_event_loop fake (#1548) |
| `threadsafe.rs` | threadsafe functions **inline** (call_js_cb na thread chamadora) — limitação cross-thread (#207) |
| `bigint.rs` | BigInt real sobre `Entry::BigInt` (negative+words u64, arbitrário, #219) |
| `surface.rs` | os 35 stubs restantes (`napi_generic_failure`) — gerados a partir da lista |
| `napi_symbols.list` | **fonte única** dos nomes exportados (consumida por `symbols.rs` e pelo `build.rs` raiz) |

Pontos de integração no codegen: `import_resolver.rs` (intercepta `.node`),
`calls/mod.rs` + `indirect.rs` (`addon.method()`, `inst.method()`),
`new_expr.rs` (`new addon.X()`), `jit.rs` (registra os símbolos internos),
`build.rs` raiz (export-table), `heap/handles.rs` (`Entry::NapiExternal` + fila
de finalizers).

> **Como usar este doc:** cada etapa tem checkboxes `[ ]`. Marcar `[x]` ao concluir,
> sempre com o critério de saída verificado. Atualizar este arquivo no mesmo
> commit da mudança que ele descreve.
> **Base de pesquisa:** `docs/specs/node-format/` (estudo de viabilidade, 8 docs).

---

## Como testar com um addon npm REAL (win32)

Validado com `@node-rs/crc32`, `@node-rs/xxhash` (função + **classe**), `@napi-rs/uuid`.

```bash
# 1. Instala um addon napi-rs prebuilt
mkdir scratch && cd scratch && npm init -y
npm install @node-rs/xxhash
cp node_modules/@node-rs/xxhash-win32-x64-msvc/*.node xxhash.node

# 2. (Windows) addons sem delay-load precisam de import lib do host.
#    Os prebuilts napi-rs já usam delay-load → resolvem GetModuleHandle(NULL)=rts.exe.
#    Se um addon falhar no link, gere a import lib:
dumpbin /EXPORTS rts.exe | grep napi_  > napi_host.def   # + cabeçalho "EXPORTS"
lib /DEF:napi_host.def /OUT:rts.lib /MACHINE:X64 /NAME:rts.exe

# 3. Roda no RTS e compara com o Node
echo 'import a from "./xxhash.node"; console.log(a.xxh32("hello world"));' > t.ts
rts run --allow-native-addons t.ts          # → 3468387874
node -e 'console.log(require("./xxhash.node").xxh32("hello world"))'  # idem
```

**Lição-chave (por que TODOS os símbolos são exportados):** o `napi-sys`
(runtime do napi-rs) resolve a superfície N-API **inteira** de uma vez no
`load_all()` via `GetProcAddress`. Exportar só os implementados faz addons reais
panicarem com *"symbol has not been loaded"* mesmo usando só o núcleo. Por isso
`surface.rs` exporta os 35 não-implementados como stubs — o `load_all()`
completa e só as fns realmente chamadas precisam funcionar.

---

## Contexto

O RTS compila TS/JS para binário nativo com runtime Rust mínimo e ABI de tipos de
máquina. Queremos carregar **addons nativos `.node`** do ecossistema npm — a porta
de compatibilidade com pacotes binários (parsers, hashing, compressão).

O estudo `docs/specs/node-format/` concluiu: **viável só pela N-API** (ABI estável,
`napi_value`/`napi_env` opacos → mapeáveis à `HandleTable` sem V8), **preferencialmente
no JIT** (`dlopen` é natural), **nunca para addons V8-diretos/NAN** (exigiriam emular o
layout binário do V8 — fora de escopo, como Bun e Deno). Um `.node` é uma
DLL/`.so`/`.dylib` comum cujo entry point é `napi_register_module_v1(napi_env, napi_value)`.

### Decisões canônicas (tomadas; não reabrir sem justificativa)

1. **Escopo entregue:** Fase 0 (loader) + Fase 1 (núcleo síncrono) + Fase 2
   (Buffer, Date, Symbol, wrap/unwrap, type tags, Promise/deferred, finalizer) +
   **classes nativas** (`napi_define_class`/`new`/método). **124/159 fns.**
   **Ainda fora (bloqueado pelo engine, #1548):** arraybuffer/typedarray/dataview
   com ptr estável, async/threadsafe (event loop #207), BigInt real (#219).
   *(O escopo original era só Fase 0+1; foi estendido conforme o engine permitiu.)*
2. **Crate `crates/rts-napi/`** depende de `rts-engine` + `rts-shared` + `libloading`
   (+ `indexmap`). Símbolos cross-crate (`__RTS_FN_GL_FUNCTION_CALL`,
   `__RTS_FN_NS_PROMISE_*`) chamados via `extern "C"` resolvido no link do bin.
3. **AOT (`rts compile`):** proibir `.node` com erro claro. Self-extracting (estilo Deno)
   só projetado, não implementado.
4. **Só N-API puro.** V8-direto/NAN → erro claro.
5. **Modelo de símbolos (resolvido adversarialmente):** as fns `napi_*` existem
   **apenas** como símbolos crus `#[unsafe(no_mangle)] pub extern "C"` em `rts-napi`.
   **NÃO** criar `NamespaceSpec`/membros SPECS para elas — `validate_symbol`
   (`crates/rts-engine/src/abi/symbols.rs`) exige prefixo `__RTS_` + scope ∈
   {NS,GC,ABI,GL}; `napi_create_double` não passa, e abrir exceção seria peso morto
   (codegen nunca emite `napi.*`). A "camada de registro" vira uma **lista declarativa**
   `pub const NAPI_EXPORTED_SYMBOLS: &[&str]` (fonte única para gerar a export-table e
   um teste de coerência). **Consequência:** napi fica **fora** do `rts.d.ts`
   automaticamente (sem `NamespaceSpec`, `emit_types.rs` não as vê).

### Invariantes de correção (não violar)

- **`napi_value` é SEMPRE um `u64` handle estável** (slot vivo da `HandleTable`) **ou**
  uma das 5 sentinelas JS (`i64::MIN..=MIN+4`, `gen==0`). **Nunca** um i64 escalar cru.
- **Todo número** (`create_double/int32/uint32/int64`) é **sempre boxed** em
  `Entry::FloatPrim(f64)` — nunca inline — para ter identidade estável e ser
  GC-rastreável dentro do frame nativo opaco do addon.
- **Anti-UAF:** gravar o handle no chunk do handle-scope **antes** de retornar o
  `napi_value` ao addon. `alloc_entry` dispara GC a cada 256 allocs — zero janela
  entre alloc e registro como root.
- **`.node` resolve undefined contra a export table do bin `rts`** (loader do SO),
  **não** contra `JITBuilder::symbol`. São mecanismos ortogonais: `jit.symbol(name,ptr)`
  só serve para o código JIT-gerado achar externs (e TS nunca referencia `napi_*`).

---

## Mapa de fatos do código (verificados)

| Peça | Arquivo : símbolo | Nota |
|---|---|---|
| Bin `rts` | raiz `Cargo.toml` `[[bin]]` `src/main.rs` | **NÃO** em `rts-cli` (que é lib) |
| Release profile | raiz `Cargo.toml` `[profile.release]` | `lto=true`, `opt-level="z"`, `codegen-units=1`, `strip="symbols"` ← **maior risco** |
| `.cargo/config.toml` | **não existe** | criar ou usar `cargo:rustc-link-arg-bin=rts=...` no `build.rs` raiz |
| Interceptar `.node` | `crates/rts-codegen/src/module/import_resolver.rs` | `validate_source_extension` (~435), `resolve_source_candidate` (236), `resolve_package_entry` (331), `resolve_node_modules_import` (461) |
| `ModuleKind` | `crates/rts-codegen/src/module/mod.rs` (~24-43) | falta `NativeAddon`; espelhar trato `Builtin` em `detect_cycle` (221/225), `flatten_for_jit` (395), `transitive_deps_hash` (368), `disk_paths` (477); loop `load` lê `read_to_string` (98) ← quebra em binário |
| Pipeline JIT | `crates/rts-codegen/src/codegen/jit.rs` | `register_runtime_symbols` (137) = só externs do JIT; **irrelevante** p/ resolver o `.node` |
| Pipeline run/compile | `crates/rts-codegen/src/pipeline.rs` | `run_jit_with_imports` (run); `compile_file` (AOT) ← proibir `.node` |
| `HandleTable`/`Entry` | `crates/rts-engine/src/heap/handles.rs` | `Entry` (327-509), `alloc_entry` (1083), `get` (917), `free` (881), `cleanup_entry` (660), `trace_children` (687), `sweep_unmarked` (984); sentinelas `i64::MIN..MIN+4`; `Entry::FloatPrim` (492) |
| GC roots | `crates/rts-engine/src/collector/global_roots.rs` | `add(addr)`/`remove(addr)`/`for_each`; scanner lê `*(addr as *const u64)`, filtra `gen!=0` |
| Error slot | `crates/rts-std/src/collector/error.rs` | `__RTS_FN_RT_ERROR_SET` (59), `_ERROR_GET` (70), `_ERROR_CLEAR` (87) |
| String pool | string_pool.rs | `__RTS_FN_NS_GC_STRING_NEW(ptr,len)`, `_STRING_PTR`, `_STRING_LEN`; `read_string_handle` |
| Trampolim callconv | `crates/rts-primitives/src/function/ops.rs` | `packed_shim` `extern "C" fn(*const i64,i64)->i64` (62, **sem teto**), `invoke_all_i64` (187, teto 16 — **não usar**), `__RTS_FN_GL_FUNCTION_CALL` (801), `FunctionData.keep_alive` (793) |
| User call conv | `crates/rts-codegen/src/codegen/lower/compile/util.rs` | `user_call_conv` → `default_call_conv` (extern "C") p/ address-taken/lifted |
| CLI flags | `crates/rts-cli/src/cli/mod.rs` | `CliFlags` (20), `parse_flags` (~127), `CompileOptions` em `compile_options.rs` |
| Collections | `crates/rts-shared/src/collections/{map,vec}.rs` | reusar ops de Map/Vec — não reimplementar |

---

# FASE 0 — Loader (addon dummy carrega)

## Etapa 0 — SPIKE export-table (de-risk; bloqueia tudo)

Provar que um símbolo `#[unsafe(no_mangle)] pub extern "C" fn napi_create_double(...)`
linkado no bin `rts` aparece na export table em **debug E release** (com
`lto+strip="symbols"+opt-level="z"`), e que um `.node` mínimo o resolve via
`dlsym(GetModuleHandle(NULL), ...)`.

- [x] 1 símbolo `napi_test_export` `#[unsafe(no_mangle)] pub extern "C"` no bin `src/main.rs` (`#[used]` é inválido em fn — a retenção vem do `/EXPORT`, não dele)
- [x] `build.rs` raiz: `emit_napi_export_args()` emite `cargo:rustc-link-arg-bin=rts=/EXPORT:<sym>` (win/MSVC), `-Wl,--export-dynamic` (linux), `-Wl,-exported_symbol,_<sym>` (macOS), condicional por `CARGO_CFG_TARGET_OS`/`_ENV`
- [x] **Validado no Windows:** `dumpbin /EXPORTS target\release\rts.exe` mostra `napi_test_export` **mesmo com `strip="symbols"`+`lto`+`opt-level="z"`** → `/EXPORT` força a entrada na export directory do PE e sobrevive ao strip. Nenhum override de profile necessário no Windows.
- [ ] Linux/macOS: validar no CI quando disponível (risco 1/2 da lista) — só Windows provado localmente
- **Saída (Windows):** ✅ símbolo presente na export table do release. `.node` real resolvendo via `GetModuleHandle(NULL)` = teste de integração da Etapa 4.
- **Nota:** o símbolo de teste foi revertido após validação; o mecanismo do `build.rs` permanece e a Etapa 1 o alimenta com `NAPI_EXPORTED_SYMBOLS`.

## Etapa 1 — Crate `rts-napi` esqueleto + export-table completa ✅

- [x] `crates/rts-napi/` membro no workspace `Cargo.toml`; deps `rts-engine` + `rts-shared` + `libloading = "0.8"` (já no `Cargo.lock`)
- [x] bin `rts` depende de `rts-napi` (dep direta)
- [x] **Fonte única `crates/rts-napi/napi_symbols.list`** (um nome/linha) — consumida por `symbols.rs` (`include_str!` → `exported_symbols()`) E pelo `build.rs` raiz (`include_str!` → args de export). **55 símbolos** (núcleo 80/20)
- [x] `src/types.rs`: `napi_value`/`napi_env`/`napi_ref`/`napi_handle_scope`/`napi_callback_info` (`#[repr(transparent)]`), `napi_status`/`napi_valuetype` (`#[repr(C)]`, ordem ABI-fixa), `napi_callback`/`napi_finalize`, `NAPI_AUTO_LENGTH`
- [x] `src/env.rs`: `RtsNapiEnv` (esqueleto: `api_version`; scopes/refs entram nas Etapas 8/9) + `into_raw`/`from_raw` + `value_from_handle`/`handle_from_value` + `RTS_NAPI_VERSION=8`
- [x] As ~55 fns como stubs `#[unsafe(no_mangle)] pub extern "C"` (macro `napi_stub!`) retornando `napi_generic_failure`, com **assinaturas reais da ABI**. `napi_get_version`/`node_api_module_get_api_version_v1` já implementadas. (`#[used]` é inválido em fn — retenção via `/EXPORT`+`force_link`)
- [x] `build.rs` `emit_napi_export_args()`: `/EXPORT:<sym>` (win/MSVC), `--export-dynamic`+`-u <sym>` (linux), `-u`+`-exported_symbol _<sym>` (macOS), da fonte única
- [x] **Retenção de símbolo:** `rts_napi::force_link()` referencia o ptr de toda fn `napi_*`; `main.rs` chama via `black_box` → impede o LTO de descartar o rlib (sem isto: `LNK2001: 55 unresolved externals`)
- [x] Testes `cargo test -p rts-napi` (3/3): sem duplicatas, prefixo N-API, contagem 55
- [x] **Validado:** `dumpbin /EXPORTS target/release/rts.exe` → **55 nomes `napi_*`/`node_api_*` distintos** na export table (ICF funde os corpos-stub idênticos num RVA só — esperado; os nomes são todos resolvíveis por `dlsym`; o fold se desfaz quando as Etapas 5-12 derem corpos distintos)
- [x] Smoke: `rts run` funciona (sem regressão do `force_link`)
- **Saída:** ✅ `rts.exe` linka; 55 `napi_*` na export table release.

## Etapa 2 — `Entry::NapiExternal` + hooks GC ✅

- [x] `Entry::NapiExternal(Box<NapiExternalData>)` em `handles.rs` (antes de `Free`); `NapiExternalData { data, finalize: Option<extern "C" fn(env,data,hint)>, finalize_hint }` (ponteiros crus — engine não depende de `rts-napi`). `Debug` manual + `unsafe impl Send` (ponteiros opacos, nunca dereferenciados pelo engine; finalize só disparado na thread JS)
- [x] Fila global `PENDING_NAPI_FINALIZERS` (Mutex<Vec>) + `pub fn drain_pending_napi_finalizers()` — o `rts-napi` drena fora do lock e dispara com o `napi_env` certo
- [x] `cleanup_entry`: arm `NapiExternal` **enfileira** `(data, finalize, hint)` — **não** chama finalize sob o lock do shard (deadlock/reentrância); disparo real = Fase 2
- [x] `trace_children`: cai no `_ => {}` (sem filhos GC) — correto
- [x] Sem match exaustivo de debug-name a cobrir
- [x] Teste `napi_external_finalizer_is_queued_not_called`: round-trip do ptr opaco; free enfileira (0 chamadas sob lock); drain retorna 1 com data/hint corretos; external sem finalizer não enfileira
- **Saída:** ✅ `cargo test -p rts-engine` 51/51 (+5+1); alloc+free de `NapiExternal` sem chamar finalize sob lock.

## Etapa 3 — Interceptação de import `.node` ✅

- [x] `ModuleKind::NativeAddon` + `as_str()="native-addon"` + helper `is_synthetic_leaf()` (agrupa `Builtin`+`NativeAddon` p/ não esquecer sites)
- [x] Sites de folha sintética via `is_synthetic_leaf()`: `detect_cycle` (2×), `transitive_deps_hash`, `flatten_for_jit`. `disk_paths` **inclui** `NativeAddon` de propósito (é arquivo real em disco — só `Builtin` sintético excluído)
- [x] Loop `ModuleGraph::load`: se `kind == NativeAddon`, **não** `read_to_string`; insere `SourceModule::from_native_addon` (program default, exports vazio) + `continue`
- [x] `validate_source_extension(path, allow_native)` aceita `node` quando `allow_native`; `resolve_source_candidate` passa `true` (deixa o path passar), `resolve_entry_path` passa `false` (entry `.node` = erro). Helper `is_native_addon`
- [x] `classify_resolved()` — ponto único que mapeia ext→kind + aplica o gate `--allow-native-addons`; usado nos 3 caminhos (relativo, node_modules, dependência de manifest)
- [x] Flag `--allow-native-addons`: `CliFlags` + `parse_flags` + `CompileOptions.allow_native_addons`
- [x] `native_addon_imports: HashMap<String, String>` (local→abs path) no `Program`; capturado em `flatten_for_jit` quando um `Item::Import` resolve para módulo `NativeAddon` (default + named)
- [x] AOT (`compile_file`): `graph.first_native_addon()` → erro claro proibindo `.node` em `rts compile`
- [x] Sem flag → erro `E005` claro com suggestion
- [x] **Validado e2e:** (1) sem flag → `E005`; (2) com flag → grafo carrega (`.node` vira folha, não parseado como TS), programa roda; (3) `rts compile` → erro AOT
- [x] **Suite TS 1710/1710** (630 arquivos), zero regressão
- **Saída:** ✅ interceptação completa; gate de segurança; AOT proibido.

## Etapa 4 — Loader + handshake + bind no codegen ✅

- [x] `crates/rts-napi/src/loader.rs` — `__RTS_FN_NS_NAPI_LOAD_ADDON(path_ptr, path_len) -> u64`:
  - [x] `libloading::Library::new(path)` + cache global por path (`LOADED_ADDONS: Mutex<HashMap>`) que mantém a `Library` viva pelo processo (fn_ptrs do addon não podem dangle) e dá **idempotência** (mesmo `.node` → mesmo handle)
  - [x] resolve `napi_register_module_v1` (**2-args** `(napi_env, napi_value)`); ausente → erro claro (registro legado = fora de escopo)
  - [x] cria `exports = alloc_entry(Entry::Map)`; fabrica `RtsNapiEnv`; chama `register(env, exports)`; usa o retorno se não-nulo, senão o exports criado
  - [x] path inválido/nulo → handle 0 (sem panic)
- [x] **Fiação:** `rts-runtime` re-exporta `rts-napi as napi`; `rts-codegen::napi`; loader registrado no JIT (`jit.rs` `add_fn!`). `force_link` retém o símbolo
- [x] **Bind no codegen:** `Program.native_addon_imports` → thread-local (`passes::native_addon`) populado em `compile_program`; `lower_ident_expr` emite `LOAD_ADDON(path)` quando o ident é um addon; `lower_typeof` classifica addon como `"object"` (antes do fallback "undefined")
- [x] **Testes:** loader unit (path inválido/nulo) + **teste de integração** (`tests/loader_integration.rs`) que compila um addon dummy real via `rustc` cdylib→`.node`, carrega, valida `Entry::Map` vivo, e idempotência
- [x] **e2e validado:** addon `.node` real (cdylib Rust com `napi_register_module_v1`) → `import addon from "./real.node"` → `typeof addon === "object"` via `rts run --allow-native-addons`
- [x] **Suite TS 1710/1710**, `rts-napi` 7/7, `rts-engine` 56/56 — zero regressão
- **Saída (Fase 0):** ✅ **um `.node` real carrega e roda em `rts run`.**

---

## ✅ FASE 0 COMPLETA

O ciclo de carga de um addon `.node` funciona ponta a ponta no JIT:
interceptação do import → gate de segurança → loader dinâmico → handshake N-API
→ exports vinculado ao TS. As ~40 fns `napi_*` da Fase 1 ainda são stubs
(`napi_generic_failure`); o addon carrega mas ainda não pode **fazer** nada útil
até a Fase 1 dar corpo a elas.

---

# FASE 1 — ~40 fns síncronas (addon real)

> **✅ FASE 1 COMPLETA** — todas as 8 etapas (5-12) implementadas; **0 stubs
> restantes** (as ~55 fns têm corpo real). `rts-napi` 30 unit + 2 integração
> (loader + paridade-vs-Node); 55 símbolos na export table; suite TS 1710/1710.
>
> **🎯 PARIDADE COM NODE CONFIRMADA:** o mesmo addon N-API (`add(a,b)` via
> `napi_create_function`/`get_cb_info`/`get_value_double`/`create_double`)
> produz saída **idêntica** no Node v22 e no RTS — `add(2,3)=5`, `add(10,7)=17`,
> `add(-1,1)=0`. Validado por comparação diferencial direta.

## Etapa 5 — Marshalling escalar + typeof

- [ ] `napi_create_double/int32/uint32/int64` (sempre `FLOAT_BOX` → `Entry::FloatPrim`)
- [ ] `napi_get_value_double/int32/uint32/int64/bool` (`FLOAT_UNBOX` + cast ToInt32/ToUint32)
- [ ] `napi_get_boolean/undefined/null/global` (sentinelas `i64::MIN..` + Map singleton p/ `global`)
- [ ] `napi_typeof` (handle inválido → `napi_undefined`; não assumir number como `__RTS_FN_RT_TYPEOF_HANDLE`)
- **Saída:** round-trip `create_double(3.14)` → typeof number → `get_value_double`==3.14; cada sentinela classifica certo.

## Etapa 6 — Strings

- [ ] `napi_create_string_utf8` (`NAPI_AUTO_LENGTH=-1` → strlen; `__RTS_FN_NS_GC_STRING_NEW`)
- [ ] `napi_get_value_string_utf8` (**protocolo 2 passagens**: `buf=NULL` → `*result=byte_len` sem NUL; cópia → `min(len, bufsize-1)`, **`floor_char_boundary`**, NUL obrigatório, `*result` exclui NUL)
- **Saída:** medição sem NUL; cópia trunca em char boundary; round-trip "café".

## Etapa 7 — Objects / arrays / props

- [ ] `napi_create_object/array/array_with_length` (`alloc_entry(Map/Vec)`, holes = `i64::MIN+4`)
- [ ] `napi_set/get_named_property`, `napi_set/get_property`, `napi_set/get_element`, `napi_get_array_length`, `napi_is_array`
- [ ] Reusar ops de `rts-shared/src/collections/{map,vec}.rs` — não reimplementar
- **Saída:** obj set/get prop; array len 3 set/get; typeof object.

## Etapa 8 — Handle scopes (risco principal de correção)

- [ ] `ScopeChunk { slots: [u64; N], used, next: Option<Box<ScopeChunk>> }` em `Box` (endereço **estável** — **não** usar `Vec<u64>`, que realoca e muda a base que o scanner lê)
- [ ] pilha de scopes no `RtsNapiEnv`; `napi_open/close_handle_scope` → `global_roots::add/remove` **por chunk**; ao encher, encadeia novo chunk + novo `add`
- [ ] gravação automática de cada `napi_value`-handle no scope topo **antes** de retornar (anti-UAF)
- [ ] `napi_open_escapable_handle_scope` + `napi_escape_handle` (promove ao pai 1×; 2ª vez → `napi_escape_called_twice`)
- **Saída:** abrir scope, criar 200 strings (>1 chunk, >256 allocs = GC tick), forçar mark+sweep, nenhum coletado; após close+GC, coletados; escape sobrevive ao close; 2º escape → erro.

## Etapa 9 — References

- [ ] `RefTable` (Slab) no env; `napi_create/delete_reference`, `napi_reference_ref/unref`
- [ ] strong (refcount>0) = `Box<u64>` + `global_roots::add`; weak (0) = sem root, guarda handle
- [ ] `napi_get_reference_value` → weak coletado retorna undefined (`get(handle).is_none()` via gen check)
- **Saída:** strong sobrevive a GC; unref → weak + GC → `get_reference_value`==undefined; delete remove root.

## Etapa 10 — Exceções

- [ ] `napi_throw` (`__RTS_FN_RT_ERROR_SET`)
- [ ] `napi_throw_error/type_error/range_error` (`msg: *const c_char` → `CStr`; `make_error_obj(name,msg)` → `Entry::ErrorObj`; seta slot)
- [ ] `napi_create_error/type_error/range_error` (`msg` como `napi_value` String → `read_string_handle`; **não** seta slot)
- [ ] `napi_is_exception_pending` (`_ERROR_GET()!=0`); `napi_get_and_clear_last_exception` (`_ERROR_GET`+`_ERROR_CLEAR`)
- **Saída:** `throw_type_error` → pending true → `get_and_clear` retorna obj name="TypeError"; throw escapando ao top-level TS reportado com nome correto.

## Etapa 11 — Functions / callbacks (trampolim bidirecional)

- [ ] **Sentido 1 (TS chama fn nativa):** `napi_create_function(env,name,len,cb,data,&result)` → `Entry::Function` com `packed_shim` apontando p/ trampolim genérico `extern "C" fn(*const i64,i64)->i64` (**sem teto** — não `invoke_all_i64`). `(cb,data)` vivo via `FunctionData.keep_alive: Arc<...>` ou tabela lateral por handle. Trampolim monta `NapiCallbackInfoData`, chama `cb(env,info)`, converte retorno
- [ ] **Sentido 2 (addon chama fn TS):** `napi_call_function(env,recv,func,argc,argv,&result)` → empacota argv num `Entry::Vec` → `__RTS_FN_GL_FUNCTION_CALL(func_handle, recv, args_vec)` (`ops.rs:801`)
- [ ] `napi_get_cb_info` (in/out `argc`: capacidade→real; resto = undefined; `*this`, `*data`)
- [ ] `napi_define_properties` (`.value`→set_named; `.method`→create_function+set_named; ignora getter/setter/attributes na Fase 1)
- **Saída:** addon `add(a,b)` roda; callback bidirecional (addon invoca fn TS passada como arg) roda.

## Etapa 12 — `napi_create/get_value_external` + polimento

- [ ] `Entry::NapiExternal` round-trip; `napi_typeof` → external
- [ ] Confirmar napi **fora** de `rts.d.ts` (sem `NamespaceSpec` → `emit_types.rs::generate()` inalterado)
- **Saída (Fase 1):** addon síncrono real (hashing ou compressão síncrona) roda.

---

## Etapa 8 — Handle scopes ✅

- [x] `crates/rts-napi/src/scopes.rs`: `ScopeChunk { slots: [u64; 32], used, next }` em `Box` (endereço **estável** — `Vec` realocaria e quebraria os roots). `Scope` = lista encadeada de chunks; `ScopeStack` no `RtsNapiEnv`
- [x] Cada slot usado é registrado individualmente em `global_roots::add(&slots[i])`; fechar o scope desregistra todos (via `Drop`)
- [x] `napi_open/close_handle_scope`, `napi_open/close_escapable_handle_scope`, `napi_escape_handle` (promove ao pai 1×; 2ª vez → `napi_escape_called_twice`)
- [x] `track_in_env` integrado nas fns de criação (`box_number`, `create_string_utf8`, `create_object/array/array_with_length`) — grava o handle no scope topo **antes** de retornar (anti-UAF)
- [x] Testes: open+track(35 handles, >1 chunk)+close registra/desregistra N roots; escape promove ao pai e sobrevive ao close; 2º escape falha
- **Saída:** ✅ handles vivos dentro do frame nativo do addon são GC roots; coletados ao fechar o scope.

## Etapa 9 — References ✅

- [x] `crates/rts-napi/src/references.rs`: `RefTable` (slab Vec+free-list) no `RtsNapiEnv`; `RefEntry { target: Box<u64>, refcount, rooted }`
- [x] strong (refcount>0) = `Box<u64>` registrado em `global_roots`; weak (0) = sem root. `set_strong` re-registra/desregistra na transição
- [x] `napi_create_reference` (refcount inicial), `napi_delete_reference`, `napi_reference_ref/unref`, `napi_get_reference_value` (weak coletado → undefined via `with_entry(...).is_none()`)
- [x] Testes: strong↔weak alterna o root; unref→0 remove root, ref→1 re-adiciona, delete remove; weak coletado (free_handle) → `get_reference_value` undefined; refcount inicial 0 = weak
- **Saída:** ✅ refs strong mantêm o valor vivo entre chamadas; weak refletem coleta.

## Etapas restantes (get_global / instanceof / define_properties) ✅

- [x] `napi_get_global` (objects.rs): Map singleton lazy por processo (`globalThis`)
- [x] `napi_instanceof` (objects.rs): heurística sobre `__rts_class` da instância vs `Function.name` do constructor (caso comum; sem hierarquia)
- [x] `napi_define_properties` (functions.rs): honra `utf8name`/`value`/`method`/`data` (method → `create_function`+`set_named`); ignora getter/setter/attributes na Fase 1
- **0 stubs restantes** — `napi_stub!` macro removida.

## Etapa 11 — Functions / callbacks (trampolim bidirecional) ✅

- [x] `crates/rts-napi/src/functions.rs`:
  - [x] `napi_create_function`: aloca um `Entry::Function` marcador (`fn_ptr=0`) e registra `(cb, env, data)` num `NAPI_CALLBACKS: Mutex<HashMap<handle, NapiFn>>` indexado pelo handle
  - [x] `__RTS_FN_RT_NAPI_DISPATCH_CALLBACK(handle, this, args_handle, out_result) -> i64`: shim chamado no início de `__RTS_FN_GL_FUNCTION_CALL` (rts-primitives, via `extern "C"` resolvido por link/JIT). Se o handle está no registry, monta um `CallbackInfo`, chama `cb(env, info)`, escreve o handle do resultado em `out_result` e devolve 1; senão devolve 0 (dispatch normal segue)
  - [x] `napi_get_cb_info`: lê o `CallbackInfo`; `argc` in/out (capacidade→real, resto preenchido com undefined); `this_arg`/`data`
  - [x] `napi_call_function`: empacota argv num `Entry::Vec` e chama `__RTS_FN_GL_FUNCTION_CALL` (sentido inverso: addon chama fn TS)
- [x] **Codegen:** `addon.method(args)` interceptado em `calls/mod.rs` (antes do lookup de namespace, senão vira "unknown namespace member") → `lower_native_addon_method_call` (`indirect.rs`): `LOAD_ADDON` → `MAP_GET_STR(method)` → empacota args como `napi_value` (números via `FLOAT_BOX`) → `FUNCTION_CALL` → handle do resultado (ambíguo, desembrulhado pela concat)
- [x] **JIT:** `__RTS_FN_RT_NAPI_DISPATCH_CALLBACK` + `__RTS_FN_RT_MAP_GET_STR` registrados; `force_link` retém o dispatch
- [x] **e2e validado:** addon real `add(a,b)` (`napi_create_function`+`get_cb_info`+`get_value_double`+`create_double`+`set_named_property`) → `addon.add(2,3)===5` no RTS
- [x] **Paridade vs Node v22:** mesma saída (5/17/0) — `tests/parity_vs_node.rs` (skip gracioso sem MSVC/Node)
- **Saída:** ✅ addon que expõe funções chamáveis do TS roda, com paridade Node.

### ⚠️ Import lib do host (necessário para addons reais no Windows)

Um `.node` deixa os símbolos `napi_*` undefined, resolvidos contra o host em
runtime. No Windows o linker exige uma **import library** na hora de compilar o
addon. Hoje gera-se manualmente:
```
dumpbin /EXPORTS rts.exe | grep napi_  → napi_host.def
lib /DEF:napi_host.def /OUT:rts.lib /MACHINE:X64 /NAME:rts.exe
# compila o addon com  -L <dir> -l rts
```
**Follow-up:** o `rts` deveria emitir um `rts.lib`/`.def` no diretório de
distribuição (como o Node faz com `node.lib`) para que `npm`/`node-gyp`/`napi-rs`
linkem addons contra ele sem passos manuais. Prebuilts N-API (napi-rs/
prebuildify) usam delay-load e resolvem por `GetModuleHandle(NULL)` →
funcionam sem relink (o `win_delay_load_hook` cai no rts.exe).

## Riscos / unknowns (spike antes da etapa correspondente)

1. **[CRÍTICO] `strip="symbols"` + `lto=true` + `opt-level="z"` vs export-dynamic.**
   Provar que `/EXPORT`/`--dynamic-list`/`-exported_symbols_list` sobrevivem ao strip
   release no bin `rts` nos 3 OS. Se não, override de profile p/ o bin. → Etapa 0.
2. **[ALTO] macOS two-level vs flat namespace.** dlopen resolver undefined contra a
   imagem principal pode exigir `-exported_symbols_list` (mantém two-level) — evitar
   `-flat_namespace`. Spike no CI macOS.
3. **[ALTO] `cargo:rustc-link-arg-bin=rts=` com LTO.** Confirmar granularidade (só o
   bin) e que `#[used]`+`no_mangle`+`/EXPORT` sobrevivem à finalização do LTO. → Etapa 0.
4. **[MÉDIO] `napi_value` + GC reentrante.** Provar "gravar no scope antes de retornar"
   e "número sempre `FloatPrim`" sob GC tick a cada 256 allocs dentro de uma chamada ao
   addon. Spike de design antes da Etapa 8.
5. **[MÉDIO] Trampolim callconv + keep-alive.** `Entry::Function` não tem campo `data`
   nativo; decidir `keep_alive: Arc` vs tabela lateral. Confirmar caminho `packed_shim`
   (não `invoke_all_i64`, teto 16). `Library` não-droppada enquanto handles do addon
   vivem. Spike antes da Etapa 11.
6. **[MÉDIO] Windows delay-load fallback.** Depende do `win_delay_load_hook` embutido no
   `.node` cair em `GetModuleHandle(NULL)`. Addons sem o hook (raros) → erro claro.

**Unknowns secundários (erro-claro na Fase 0, não bloqueiam):** registro legado
`napi_module_register` por construtor estático; pacotes napi-rs com **wrapper JS** que
`require('./x.node')` (Fase 0 cobre só import direto do `.node` + `main` literal `.node`);
addons sem `win_delay_load_hook`.

---

## Addon de teste (win32 x64)

- **Etapas 0-4 (loader/export-table):** baixar um **prebuilt** `.node` win-x64 sem
  toolchain — `npm i` num projeto scratch de um pacote napi-rs simples e pegar o
  `node_modules/.../*.node`. Valida export-table + loader sem compilar nada.
- **Etapa 11 (`add(a,b)`):** compilar com **napi-rs** (`@napi-rs/cli`: `napi new` →
  template `add(a,b)` → `napi build`). O repo já tem MSVC + Rust → mais limpo que
  node-gyp (que exigiria Python). **Fixar a versão NAPI do addon ≤ a implementada.**
  `node-addon-api` é alternativa equivalente.
- Validar que o addon exporta `napi_register_module_v1` por dlsym (não registro legado).

---

## Fases futuras (35 stubs restantes — bloqueadas pelo engine, ver #1548)

- **✅ Fase 2 (FEITO):** Buffer, Date, Symbol, `napi_wrap`/`unwrap`,
  `napi_define_class` + classes nativas, type tags, Promise/deferred,
  `napi_add_finalizer` (via `Entry::NapiExternal`).
- **arraybuffer/typedarray/dataview (11 fns):** precisa de `Entry::ArrayBuffer`
  com **ponteiro mutável estável** no engine (#1548 item 1). Eu ploto as fns por
  cima assim que existir.
- **async/threadsafe (19 fns):** `napi_create_async_work`/threadsafe functions/
  `napi_get_uv_event_loop` — dependem do **event loop real** (#207, #1548 item 3).
  Cauda longa (gaps até no Bun).
- **BigInt real (4 fns):** `bigint_uint64`/`bigint_words` — dependem de
  `Entry::BigInt` real (#219). Hoje `bigint_int64` usa `FloatPrim` (perde >2^53).
- **distribuição/npm:** emitir `rts.lib` na distribuição (como `node.lib`) para
  `npm`/`node-gyp`/`napi-rs` linkarem addons contra o `rts.exe` sem passos manuais.
- **AOT self-extracting** (modelo Deno): decisão de produto adiada.
- **Nunca:** addons V8-diretos/NAN (registro legado `module_register` é o único
  stub fora-de-escopo, não bloqueado por engine).

Rastreamento: tracking geral #1547, APIs de engine #1548.
