# N-API — Plano de implementação (doc vivo de acompanhamento)

> **Status:** planejamento concluído, implementação não iniciada.
> **Escopo desta entrega:** Fase 0 (loader) + Fase 1 (~40 fns N-API síncronas, núcleo 80/20).
> **Como usar este doc:** cada etapa tem checkboxes `[ ]`. Marcar `[x]` ao concluir,
> sempre com o critério de saída verificado. Atualizar este arquivo no mesmo
> commit da mudança que ele descreve.
> **Base de pesquisa:** `docs/specs/node-format/` (estudo de viabilidade, 8 docs).

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

1. **Escopo:** Fase 0 + Fase 1 (~40 fns síncronas). **Fora:** buffers/typedarray,
   `napi_wrap`/`define_class`/finalizers-disparados, async/promise/threadsafe.
2. **Crate novo `crates/rts-napi/`** depende de `rts-engine` + `rts-shared` + `libloading`.
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

## Etapa 2 — `Entry::NapiExternal` + hooks GC (paraleliza com Etapa 1)

- [ ] `Entry::NapiExternal { data: *mut c_void, finalize: Option<napi_finalize>, finalize_hint: *mut c_void }` em `handles.rs` (~329)
- [ ] `cleanup_entry` (660): **enfileira** `(data, finalize, hint)` — **não** chamar finalize sob lock do shard (deadlock); disparo real = Fase 2
- [ ] `trace_children` (687): no-op (sem filhos GC)
- [ ] `sweep_unmarked`/debug-name: cobrir a variante
- **Saída:** `cargo test -p rts-engine` verde; alloc+free de `NapiExternal` sem chamar finalize sob lock.

## Etapa 3 — Interceptação de import `.node`

- [ ] `ModuleKind::NativeAddon` em `module/mod.rs` (~25) + `as_str()="native-addon"`
- [ ] Espelhar trato `Builtin` nos **5 sites**: `detect_cycle` (221/225), `transitive_deps_hash` (368), `flatten_for_jit` (395), `disk_paths` (477) — esquecer um = parse de binário como TS
- [ ] Loop `ModuleGraph::load` (92): se `kind == NativeAddon`, **não** `read_to_string` (98); inserir `SourceModule` sintético (program default, exports vazio) + `continue`
- [ ] `validate_source_extension(path, allow_native: bool)` aceita `node` quando `allow_native`; atualizar call sites; mapear ext→kind em `resolve_import_target`
- [ ] Flag `--allow-native-addons`: `CliFlags` (20) + `parse_flags` (~127) + `CompileOptions.allow_native_addons`
- [ ] `native_addon_imports: Vec<(String local, String abs_path)>` em `flatten_for_jit` (~405, análogo a `node_import_map`)
- [ ] AOT (`compile_file`): grafo com `NativeAddon` → erro claro ("`.node` não suportado em `rts compile`; use `rts run --allow-native-addons`")
- [ ] Sem flag → erro claro com suggestion
- **Saída:** `rts run --allow-native-addons app.ts` com `import x from "./x.node"` carrega o grafo sem parsear binário; sem flag → erro; `rts compile` com `.node` → erro.

## Etapa 4 — Loader + handshake + bind no codegen

- [ ] `__RTS_FN_NS_NAPI_LOAD_ADDON(path_ptr, path_len) -> u64` (símbolo interno, segue convenção `__RTS_`):
  - [ ] `libloading::Library::new(path)` + registry estático/`mem::forget` (lib viva — fn_ptrs do addon não podem dangle)
  - [ ] resolver opcional `node_api_module_get_api_version_v1`; versão > implementada → erro claro
  - [ ] resolver `napi_register_module_v1` (assinatura **2-args** `(napi_env, napi_value)` — o "5 args" do `node_binding.cc` é interno, não cruza o boundary); ausente → erro claro (registro legado por construtor estático = fora de escopo)
  - [ ] criar `exports = alloc_entry(Entry::Map(...))`; fabricar `RtsNapiEnv` com **handle scope implícito aberto**
  - [ ] chamar `register(env, exports)`; devolver handle do `exports`
- [ ] Codegen de init (onde `node_import_map` é consumido, `main_fn.rs`/`program.rs`): p/ cada `native_addon_imports`, emitir `LOAD_ADDON` + bind do retorno na global do nome local
- **Saída (Fase 0):** addon dummy cujo `napi_register_module_v1` retorna `{}` → `typeof x === "object"` em TS via `rts run`.

---

# FASE 1 — ~40 fns síncronas (addon real)

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

## Fases futuras (fora desta entrega)

- **Fase 2:** buffers/typedarray/arraybuffer; `napi_wrap`/`unwrap`/`define_class`;
  finalizers disparados no sweep (família #217).
- **Fase 3:** promises (#437), async work, threadsafe functions, shim `uv_loop`
  (família #207 — cauda longa, gaps até no Bun).
- **Fase 4:** distribuição/npm (`process.platform`/`arch`/`versions` coerentes;
  priorizar napi-rs/prebuildify por plataforma).
- **AOT self-extracting** (modelo Deno): decisão de produto adiada.
- **Nunca:** addons V8-diretos/NAN.
