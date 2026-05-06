# Arquitetura — projeto, ABI, namespaces

## Projeto

RTS eh um compilador/runtime TypeScript-to-native usando Cranelift
como backend de codegen. O objetivo eh compilar TS/JS para binarios
nativos com runtime minimo em Rust, distribuido como toolchain
standalone (sem runtime support library externa).

A camada de runtime eh organizada em torno do contrato `crates/rts-abi/` +
`SPECS`, com pipeline por grafo de modulos + cache incremental. Dois
caminhos de execucao: JIT via `cranelift_jit::JITModule` (memoria
executavel direta, `rts run`) e AOT via
`cranelift_object::ObjectModule` (linker externo, `rts compile`).

Consultar `RTS_REFACTOR.md` para a direcao vigente do refator em
workspace de crates.

## Arquitetura

Workspace Cargo com 9 crates em `crates/`. O diretorio `src/` continua
existindo mas eh fachada do bin `rts` (re-exports dos crates); paths
reais ficam sob `crates/<crate>/src/`.

```
crates/
  rts-ast/         — AST interno
  rts-parser/      — SWC parse + AST; converte arrow/fn expressions em Item::Function top-level
  rts-diagnostics/ — erros estruturados
  rts-abi/         — contrato unico de ABI (SPECS, tipos, simbolos, guards, assinaturas, Intrinsic)
  rts-hir/         — HIR tipado (etapa 2.1 do refator); ainda NAO plugado no pipeline (issue #611)
  rts-codegen/     — Cranelift codegen + type_system + module/ + pipeline + cache + eval_jit
    src/codegen/
      emit.rs      — ObjectModule emitter (AOT)
      jit.rs       — JITModule emitter (rts run)
      lower/       — lower de expr/stmt/func sobre &mut dyn Module
    src/type_system/ — type checker, registry, resolver
    src/module/      — resolver de modulos e grafo de dependencias
    src/pipeline.rs  — orquestra build/run; inclui run_jit para path JIT
  rts-runtime/     — builtin module "rts" + submodulos "rts:<ns>" + namespaces runtime
  rts-linker/      — link nativo (system linker com fallback object backend)
  rts-cli/         — CLI (run, compile, apis, init, repl, eval, ir)

src/                — fachada bin (re-exports), runtime_objects.rs, main.rs
```

Pipeline AOT: `Source TS → Parser(SWC) → type_system → codegen(Cranelift) → Object → Linker → .exe`
Pipeline JIT: `Source TS → Parser(SWC) → type_system → codegen(Cranelift) → JITModule → call __RTS_MAIN`

`FnCtx.module` eh `&mut dyn Module` para servir ambos os paths sem
duplicar codegen. O crate `rts-hir` (etapa 2.1 do refator) define HIR
tipado mas ainda nao esta plugado no pipeline; codegen hoje consome
AST direto e emite Cranelift IR em
`crates/rts-codegen/src/codegen/lower/`. MIR esta planejado (ver
`RTS_REFACTOR.md` Fase 3).

## ABI (`crates/rts-abi/`) — contrato unico

Toda a superficie entre codegen e runtime passa por `crates/rts-abi/`. Nao
existe mais `SPEC/MEMBERS/dispatch()` por namespace e nao existe
mais `__rts_call_dispatch`.

- `abi::SPECS` (`mod.rs`) — slice estatico com a `NamespaceSpec` de
  cada namespace registrado (`io`, `fs`, `gc`, `math`, `bigfloat`).
  Fonte unica consumida por codegen, runtime, JIT e gerador de
  `rts.d.ts`.
- `abi::lookup(qualified)` — resolve `"io.print"` →
  `&NamespaceMember` com simbolo e assinatura.
- `member.rs` — `NamespaceSpec`, `NamespaceMember` (const estaticos)
  e `Intrinsic` (enum das ops inlinaveis). Cada membro declara
  `name`, `kind` (Function|Constant), `symbol`, `args[]`, `returns`,
  `doc`, `ts_signature`, `intrinsic: Option<Intrinsic>`. Quando
  `intrinsic` eh `Some`, codegen emite IR Cranelift direto em vez de
  `call <symbol>`.
- `types.rs` — `AbiType`: `Void | Bool | I32 | I64 | U64 | F64 |
  StrPtr | Handle`. `StrPtr` expande para dois slots Cranelift
  (`ptr` + `len`).
- `signature.rs` — `lower_member()` converte a spec em
  `LoweredSignature` Cranelift.
- `symbols.rs` — convencao `__RTS_<KIND>_<SCOPE>_<NS>_<NAME>` (ex:
  `__RTS_FN_NS_IO_PRINT`). Macro `rts_sym!` gera simbolos em
  compile-time; `validate_symbol()` impoe uppercase ASCII.
- `guards.rs` — `guard_for(expected, caller)` decide
  passthrough/coerce/trap em call sites com argumentos de tipo `any`.

Codegen emite `call <symbol>` direto via Cranelift, sem
intermediarios.

## ABI de Maquina — extern "C" tipado, sem dispatch

Nao ha `JsValue`, nem `__rts_call_dispatch`, nem boxing no limite
entre codegen e runtime. Cada funcao de namespace eh um simbolo
`extern "C"` tipado.

### Convencao ABI por tipo

| Tipo TS  | `AbiType`    | Representacao Cranelift         | Observacao                                              |
|----------|--------------|---------------------------------|---------------------------------------------------------|
| `number` | `I64` / `F64`| `i64` / `f64`                   | bits nativos, sem boxing                                |
| `bool`   | `Bool`       | `i8` (0/1)                      | 0 = false, 1 = true                                     |
| `string` | `StrPtr`     | 2 slots: `(i64 ptr, i64 len)`   | UTF-8; ptr estatica do codegen, ou buffer via handle GC |
| handle   | `Handle`     | `u64`                           | `HandleTable` (gen:16 + slot:48)                        |
| void     | `Void`       | —                               | sem retorno                                             |
| inteiros | `I32` / `U64`| `i32` / `u64`                   | usados em contagens, status, tamanhos                   |

### Regras de implementacao

- Cada membro de namespace vira um `#[unsafe(no_mangle)] pub extern
  "C" fn __RTS_FN_NS_<NS>_<NAME>(...)`
- Nenhuma funcao de namespace aceita/retorna `JsValue` no limite
  `extern "C"`
- Strings dinamicas (ex: resultado de leitura) sao alocadas pelo
  `gc` e retornam um handle `u64`; leitura via
  `gc::string_ptr(handle)` + `gc::string_len(handle)`
- Call sites com argumentos `any` passam por
  `abi::guards::guard_for(...)` para decidir coerce/trap

## Estrutura de Arquivos por Namespace

```
crates/rts-runtime/src/namespaces/<ns>/
  mod.rs         — re-exporta submodulos e publica a NamespaceSpec
  abi.rs         — declaracao dos NamespaceMember (tabela estatica)
  <grupo>.rs     — impl operacional (ex: read.rs, write.rs, dir.rs, print.rs, stdout.rs, ...)
```

Regras:
- `mod.rs` eh apenas o import map + export do `NamespaceSpec`
- `abi.rs` eh a fonte da verdade dos membros do namespace (nome,
  simbolo, args, return, doc, ts)
- Cada arquivo operacional agrupa funcoes por responsabilidade
  (io/r-w/dir/metadata/...)
- Nao existe `dispatch()` por namespace — cada funcao eh um
  `#[no_mangle] extern "C"` direto

Namespaces ativos (40+): `io`, `fs`, `gc`, `math`, `num`, `bigfloat`,
`time`, `env`, `path`, `buffer`, `string`, `process`, `os`,
`collections`, `hash`, `fmt`, `crypto`, `net`, `tls`, `thread`,
`atomic`, `sync`, `parallel`, `mem`, `hint`, `ptr`, `ffi`, `regex`,
`runtime`, `test`, `trace`, `ui`, `alloc`, `json`, `date`,
`http_server`, `promise`, `events`, mais os sub-namespaces de
`globals/` (number, string, date, regexp, error, events, console,
json, timers, fetch, performance, global_this, text_encoding, url).
Cobre std::* + paralelismo + HTTPS + UI completos + JSON + Date +
HTTP server nativo via actix-web + classes JS globais completas.

### Namespaces existentes

- `io/` — print, eprint, stdout_{write,flush}, stderr_{write,flush},
  stdin_{read,read_line}
- `fs/` — read, read_all, write, append, exists, is_file, is_dir,
  size, modified_ms, create_dir(_all), remove_dir(_all), remove_file,
  rename, copy
- `gc/` — handles e string pool: string_from_{i64,f64,static},
  string_{new,concat,len,ptr,free}, `HandleTable` slab-based com
  16-bit geracao + 48-bit slot (`u64` handle); `Entry` enumera tipos
  armazenados (`String`, `BigFixed`, `Buffer`, `ProcessChild`,
  `Map`, `Vec`, `Function`, `PromiseAsync`, `Free`)
- `math/` — basic
  (floor/ceil/round/trunc/sqrt/cbrt/pow/exp/ln/log2/log10/abs_f64/abs_i64),
  trig (sin/cos/tan/asin/acos/atan/atan2), minmax
  (min/max/clamp_f64/i64), consts (PI/E/INFINITY/NAN como
  `MemberKind::Constant`), random (xorshift64 com estado em
  `__RTS_DATA_NS_MATH_RNG_STATE`). Intrinsics:
  sqrt/abs_f64/min_f64/max_f64/abs_i64/min_i64/max_i64/random_f64
- `bigfloat/` — decimal fixed-point via i128 (scale decimal ate 36).
  Operacoes: zero/from_f64/from_i64/from_str/to_f64/to_string/
  add/sub/mul/div/neg/sqrt/free. Usado para pi com 29+ digitos via
  Machin + atan de Maclaurin
- `time/` — now_ms/now_ns (Instant monotonico), unix_ms/unix_ns
  (SystemTime), sleep_ms/sleep_ns
- `env/` — get_var, set_var, remove_var, args_count, arg_at, cwd,
  set_cwd
- `path/` — join, parent, file_name, stem, ext, is_absolute,
  normalize, with_ext (operacoes puras, sem I/O)
- `buffer/` — Vec<u8> via HandleTable: alloc/alloc_zeroed/free/
  len/ptr, read/write u8/i32/i64/f64 little-endian, copy/fill,
  to_string (UTF-8)
- `string/` — search (contains/starts_with/ends_with/find),
  transform
  (to_upper/to_lower/trim/trim_start/trim_end/repeat),
  replace/replacen, char_count/byte_len/char_at/char_code_at
  (Unicode-aware)
- `process/` — exit/abort, pid, args_count/arg_at (alias de env),
  spawn (args separados por \n), wait (consume handle), kill. Child
  handle via `Entry::ProcessChild`
- `os/` — platform/arch/family/eol (std::env::consts + cfg!),
  home_dir, temp_dir, config_dir, cache_dir (XDG no Unix,
  APPDATA/LOCALAPPDATA no Windows)
- `collections/` — HashMap<string, i64> (`map_*`) e Vec<i64>
  (`vec_*`) via HandleTable. Valor sempre i64 — caller interpreta
  como int/handle/bool
- `hash/` — SipHash-2-4 deterministico para str/i64/bytes
  (hash_str, hash_i64, hash_bytes)
- `fmt/` — parse_i64/f64 (tolerante), fmt_hex/oct/bin/f64_prec
- `crypto/` — SHA-256 inline (FIPS 180-4), base64/hex encode+decode,
  CSPRNG via BCryptGenRandom (Windows) / /dev/urandom (Unix)
- `net/` — TCP listener/stream + UDP socket + DNS resolve via
  `std::net`. Handles via
  `Entry::TcpListener/TcpStream/UdpSocket(UdpEntry)`. Sync, sem deps
  externas
- `tls/` — TLS 1.2/1.3 client via `rustls` + `webpki-roots`
  (Mozilla CAs embutidos). Wraps `TcpStream` em conexao TLS. HTTPS
  funciona ponta-a-ponta sem OpenSSL nem schannel
- `thread/` — 4 mecanismos coexistindo, dev escolhe pelo workload:
  `spawn` + `join`/`detach` (`std::thread`, JoinHandle real, ~30k
  spawn/s, bom pra CPU-bound longo); `spawn_async_join` +
  `join_async` (tokio `spawn_blocking`, retorna i64, ~400k spawn/s,
  bom pra leve/IO); `spawn_async` (tokio fire-and-forget, ~400k
  spawn/s); `spawn_detached` (pool fixo 8 workers, 5M spawn/s mas
  queue ilimitada — cuidado OOM). Mais `scope` auto-join +
  `sleep_ms`. Doc-comments em `crates/rts-runtime/src/namespaces/thread/abi.rs` tem
  tabela comparativa
- `http_server/` — servidor HTTP/1.1 nativo via `actix-web` sobre
  runtime tokio compartilhado. Bridge sync→async:
  `serve(addr,handler)` bloqueia, cada request entra num shard map
  de slots, handler TS chamado direto na thread async, response
  volta via oneshot. Suporta keep-alive, pipelining, parsing
  correto. Pico medido 29k req/s (78% do actix puro Rust)
- `atomic/` — `std::sync::atomic`: AtomicI64
  (load/store/fetch_*/cas/swap), AtomicBool, AtomicF64 (via
  AtomicU64 + bit-transmute), fences
- `sync/` — `std::sync`: Mutex<i64>, RwLock<i64>, Once. Guards
  thread-local pra atravessar chamadas extern "C"
- `parallel/` — `rayon`: map/for_each/reduce + num_threads. Backing
  dos passes silent (purity_pass, reduce_pass, array_methods_pass)
- `mem/` — size_of/align_of constantes, swap_i64,
  drop/forget_handle
- `num/` — checked/saturating/wrapping arith, bit ops (rotate,
  count_ones/zeros, leading/trailing_zeros, reverse_bits,
  swap_bytes), bitcast f64<->bits
- `ptr/` — copy_nonoverlapping, raw pointer ops
- `ffi/` — CString, OsString
- `regex/` — backend `regex` crate, compile +
  test/find/replace/replace_all
- `runtime/` — eval_file (dynamic import) + eval (compila TS source
  em runtime via `runtime_eval_src_jit`) + hot-reload primitives
- `test/` — test_core (suite/case begin/end, fail) + bundle.ts
  (`rts:test` describe/test/expect)
- `trace/` — push/pop/capture/print frame stack pra erros estilo Bun
- `ui/` — FLTK 1.x bindings (Button, Window, Input, Slider, ...)
- `alloc/` — malloc-style raw allocations
- `hint/` — black_box, spin_loop, unreachable, assert_unchecked
- `events/` — EventEmitter primitivo: emitter_new/free, on,
  emit0/emit1, listener_count, remove_all_listeners
