# CLAUDE.md

## REGRA #0 — META-REGRA OBRIGATÓRIA E ABSOLUTA

**Antes de iniciar QUALQUER tarefa, você DEVE ler este CLAUDE.md por inteiro
e seguir TODAS as regras que ele define, sem exceção, sem omissão, sem
"escolher as importantes". Cada regra deste arquivo é vinculante.**

Esta é a primeira e mais importante regra. Ela governa todas as outras.
Não há trabalho neste projeto que dispense a leitura completa deste arquivo
e a aplicação de tudo o que ele determina.

### Como aplicar

1. Na primeira mensagem de cada sessão (e sempre que o arquivo for
   modificado), leia `CLAUDE.md` do começo ao fim antes de tocar em código.
2. Cada seção marcada `## REGRA OBRIGATÓRIA:` é vinculante mesmo quando o
   contexto da tarefa parece não exigir.
3. Cada seção `## Convencoes`, `## Regras`, `## ABI ...`, `## Estrutura ...`
   define convenções que devem ser respeitadas em qualquer mudança de código.
4. Se uma regra entrar em conflito com uma instrução do usuário, peça
   confirmação antes de violar a regra. Não decida sozinho.
5. Se uma regra deste arquivo estiver desatualizada (o codigo nao bate mais
   com o que esta escrito), atualize o CLAUDE.md no mesmo PR — nunca deixe
   regra mentirosa em vigor.

### Lista das regras obrigatorias atualmente neste arquivo

- **REGRA #0** (esta) — ler e seguir tudo
- **REGRA OBRIGATÓRIA: USO DO RTK** (`cat`/`head`/`tail`/`grep`/`find` viram
  `.github/rtk.exe ...`)
- **REQUISITO OBRIGATÓRIO: local-rules.md** (verificar e ler se existir)
- **REGRA OBRIGATÓRIA: ZERO REGRESSÃO ANTES DE MERGE** (suite verde
  obrigatoria)

Esta lista deve ser mantida em sincronia com as proximas secoes deste
arquivo. Se uma regra obrigatoria for adicionada/removida em outro lugar,
atualizar aqui tambem.

## Regras locais do desenvolvedor

## REGRA OBRIGATÓRIA: USO DO RTK PARA COMANDOS ESPECÍFICOS

### Comandos que DEVEM usar `.github/rtk.exe <execution>`:

| Comando | Substituição Obrigatória |
|---------|-------------------------|
| `cat`   | `.github/rtk.exe cat`    |
| `head`  | `.github/rtk.exe head`   |
| `tail`  | `.github/rtk.exe tail`   |
| `grep`  | `.github/rtk.exe grep`   |
| `find`  | `.github/rtk.exe find`   |

use os comandos nativos diretamente (ex: `cat file.txt` → use `.github/rtk.exe cat file.txt`)

### Exceções e Observações:

- **Ferramentas nativas do Claude Code** (Read, Grep, Glob) NÃO passam pelo hook e NÃO são reescritas automaticamente
- Para obter a saída compacta do RTK nesses fluxos de trabalho, use comandos shell com RTK ou chame diretamente:
  - `.github/rtk.exe read` (em vez de Read tool)
  - `.github/rtk.exe grep` (em vez de Grep tool)
  - `.github/rtk.exe find` (em vez de Glob tool)

**REQUISITO OBRIGATÓRIO:** antes de iniciar qualquer tarefa, você **DEVE** verificar se existe um arquivo `local-rules.md` na raiz do projeto. **Se existir, ler é obrigatório** — não é opcional, não pular, não assumir conteúdo, não prosseguir sem ler. Se não existir, prossiga normalmente. Quando existir, trate seu conteúdo como regras adicionais definidas pelo desenvolvedor que está trabalhando nesta cópia local. Essas regras têm prioridade sobre preferências genéricas e devem ser respeitadas durante toda a sessão.

O arquivo `local-rules.md` é pessoal de cada desenvolvedor e **não deve ser versionado** (já está no `.gitignore`).

## REGRA OBRIGATÓRIA: ZERO REGRESSÃO ANTES DE MERGE

**Toda PR — sem exceção — só pode ser merged depois de validar que TODOS os testes da suite atual ainda passam, junto com os testes novos da feature/fix.**

Suite mínima a rodar antes de aprovar merge:

```bash
cargo build --release             # build limpo (zero warnings de erro)
cargo test --release --lib        # 100% dos testes unit + integration verdes
```

Se o PR mexe em código de runtime/codegen/GC, também:

```bash
target/release/rts.exe test       # suite TS via rts:test
```

### Regras práticas

- **Build quebrado bloqueia merge.** Mesmo que "só warning". Investigar antes.
- **1 teste falhando bloqueia merge.** Não importa se "não tem relação com o PR". Falha é falha.
- **Não há excecão de "consertar depois".** Se a feature exige refator que quebra teste, refatore + corrija o teste **no mesmo PR**, com justificativa explícita no commit.
- **Testes de codegen (`tests/*.test.ts`) são parte da suite.** Se mudou comportamento esperado, atualizar os `.test.ts` e justificar.
- **PRs grandes que tocam várias áreas devem rodar a suite incrementalmente** durante o desenvolvimento, não só no fim. Se quebrou no meio, parar e corrigir antes de avançar.

### Por que essa regra existe

Em projeto com 2 devs + IA acelerando velocidade, tentação de "mergear e arrumar depois" mata o projeto em 30 dias. Cada regressão silenciosa acumula até a suite virar mentira (testes verdes mas código quebrado em casos não cobertos). Manter zero regressão é o que separa projeto que cresce em qualidade do que apodrece em features.

Disciplina aqui é inegociável. Se IA propõe solução que quebra suite, IA está errada — independente de quão convincente o argumento. Forçar outra abordagem.

## Projeto

RTS e um compilador/runtime TypeScript-to-native usando Cranelift como backend de codegen.
O objetivo e compilar TS/JS para binarios nativos com runtime minimo em Rust, distribuido como
toolchain standalone (sem runtime support library externa).

A camada de runtime e organizada em torno do contrato `crates/rts-abi/` + `SPECS`, com pipeline
por grafo de modulos + cache incremental. Dois caminhos de execucao: JIT via
`cranelift_jit::JITModule` (memoria executavel direta, `rts run`) e AOT via
`cranelift_object::ObjectModule` (linker externo, `rts compile`).

Consultar `RTS_REFACTOR.md` para a direcao vigente do refator em workspace de crates.

## Arquitetura

Workspace Cargo com 10 crates em `crates/`. O diretorio `src/` continua existindo
mas e' fachada do bin `rts` (re-exports dos crates); `src/main.rs` chama
`rts_codegen::register_runtime_artifacts` + `rts_cli::cli::dispatch`. Subdirs em
`src/` (abi, codegen, namespaces, parser, runtime, etc) sao thin re-exports —
paths reais ficam sob `crates/<crate>/src/`.

```
crates/
  rts-ast/         — AST interno
  rts-parser/      — SWC parse + AST; converte arrow/fn expressions em Item::Function top-level
  rts-diagnostics/ — erros estruturados
  rts-abi/         — contrato unico de ABI (SPECS, tipos, simbolos, guards, assinaturas, Intrinsic,
                     global_class.rs para classes JS globais, handles.rs para HandleTable ABI)
  rts-hir/         — HIR tipado (HirType I8..I128/F32/F64/Bool/Str/Handle/Array/Function/Class/Object/Any/Unknown)
  rts-mir/         — MIR SSA (60+ Insts: aritmetica/bitwise/shifts/conv/cmp/loads/stores/atomics/StrLit/CallUser/CallExtern/DeclareGcValue;
                     Terminators Return/Jump/Brif/Switch/TailCall/Trap; passes fold/fma/cse/dce/narrow/verify/inline; lower HIR→MIR)
  rts-codegen/     — Cranelift codegen + type_system + module/ + pipeline + cache + eval_jit + bundle
    src/codegen/
      emit.rs      — ObjectModule emitter (AOT, producao de .o)
      object.rs    — ObjectArtifact wrapper (slicing por uso, AOT)
      jit.rs       — JITModule emitter (rts run)
      lower/       — lower de expr/stmt/func sobre &mut dyn Module (caminho AST autoritativo)
        expressions/ — basics, calls, members, operators
        statements/  — control, decls, loops
      mir_codegen/ — lower MIR → Cranelift IR (camada paralela ao AST, ativa por default; fallback automatico para AST quando MIR bail)
    src/type_system/ — type checker, registry, resolver
    src/module/      — resolver de modulos e grafo de dependencias
    src/nodespace/   — shims de Node.js built-ins (fs, os, path, process, crypto, util)
    src/pipeline.rs  — orquestra build/run; inclui run_jit para path JIT
  rts-runtime/     — builtin module "rts" + submodulos "rts:<ns>" + 40+ namespaces
    src/namespaces/  — implementacoes dos namespaces runtime (io, fs, gc, math, etc.)
      globals/       — classes JS globais (number, string, date, regexp, error, events, ...)
    src/runtime/     — async_rt.rs (tokio runtime global), tokio_ctx.rs (bridge sync/async)
  rts-linker/      — link nativo (system linker com fallback object backend)
  rts-cli/         — CLI (run, compile, apis, init, repl, eval, ir)

src/                — fachada bin (re-exports), runtime_objects.rs, main.rs
```

> Nota: `rts-codegen` virou catch-all (pipeline, type_system, module, cache,
> eval_jit moram la), divergindo do plano original em `RTS_REFACTOR.md`. Fase 3
> (MIR) esta entregue — `rts-mir` ativo por default desde commits f7b924b/23dd4b7.
> Fase 4 (baixo nivel + extensoes) em progresso, 5/8 entregues: atomics (4.1),
> inline+integracao+fixed-point (4.2/4.3/4.7), CSE (4.5), FMA (4.8), arr[i]=v
> + smoke e2e (4.4/4.6). Restam escape analysis, SIMD e narrow storage real.
> Metricas atuais: rts-mir 59/59, rts-codegen --lib mir_codegen 61/61,
> cargo test --workspace 100/100, rts.exe test **1015/1015** (zero falhas).

Pipeline atual (default, MIR ON):

```
TS → SWC → AST → HIR (rts-hir) → MIR (rts-mir) → inline (fixed-point, max 4 iters)
                                              → optimize (fold → fma → cse → dce)
                                              → mir_codegen → Cranelift → JIT/AOT
                                              ↘ AST autoritativo (fallback automatico)
```

Routing hibrido controlado por `RTS_USE_MIR`:

| Valor | Comportamento |
|---|---|
| unset / `1` / `on` / `all` | MIR ON (default) |
| `0` / `off` / `none` | AST only |
| `fn1,fn2,...` | MIR so' pras fns listadas |

Cada user fn tenta o caminho HIR→MIR→Cranelift; se bate em construct
ainda nao modelado (member em `this`/objetos, classes, async/await,
address-taken fns, string em params/ret de user fn), cai automaticamente
no codegen AST sem perder semantica. 438 user fns reais da suite TS
hoje rodam pelo MIR; suite mantem **1015/1015 verde** (zero falhas
apos PRs #213, #218 (Reflect/Proxy), #261, #287, #289 (sha256
streaming), #377 (timers), #383, #398 (GC transitive), #407 (globals
roots), #450, #573, #584, #592, #602, #617, #618, #619, #224 (UI
event loop), mais randomUUID e varios fixes de bugs).

Pipeline AOT/JIT: ambos passam pelo mesmo `compile_program`; `FnCtx.module`
e' `&mut dyn Module` para servir os dois sem duplicar codegen.

## ABI (`crates/rts-abi/`) — contrato unico

Toda a superficie entre codegen e runtime passa por `crates/rts-abi/`. Nao existe mais
`SPEC/MEMBERS/dispatch()` por namespace e nao existe mais `__rts_call_dispatch`.

- `abi::SPECS` (`mod.rs`) — slice estatico com a `NamespaceSpec` de todos os namespaces
  registrados (40+: `io`, `fs`, `gc`, `math`, `bigfloat`, `time`, `env`, `path`, `buffer`,
  `string`, `process`, `os`, `collections`, `hash`, `fmt`, `crypto`, `net`, `tls`, `thread`,
  `atomic`, `sync`, `parallel`, `mem`, `num`, `ptr`, `ffi`, `regex`, `runtime`, `test`,
  `trace`, `ui`, `alloc`, `hint`, `json`, `date`, `http_server`, `promise`, `events`, e
  todos os sub-namespaces de `globals/`). Fonte unica consumida por codegen, runtime, JIT e
  gerador de `rts.d.ts`.
- `abi::lookup(qualified)` — resolve `"io.print"` → `&NamespaceMember` com simbolo e assinatura.
- `abi::global_class_lookup(class, method)` — resolve metodos de classes JS globais
  (`Number.isNaN`, `Date.now`, etc.) via `GLOBAL_CLASS_SPECS`.
- `member.rs` — `NamespaceSpec`, `NamespaceMember` (const estaticos) e `Intrinsic` (enum das
  ops inlinaveis). Cada membro declara `name`, `kind` (`MemberKind::Function | Constant |
  AsyncFunction`), `symbol`, `args[]`, `returns`, `doc`, `ts_signature`,
  `intrinsic: Option<Intrinsic>`. Quando `intrinsic` e `Some`, codegen emite IR Cranelift
  direto em vez de `call <symbol>`.
- `global_class.rs` — `GlobalClassSpec` e `GLOBAL_CLASS_SPECS`: registry das classes JS
  globais builtin (Number, String, Date, RegExp, Error, TypeError, RangeError, SyntaxError,
  EventEmitter, TextEncoder, TextDecoder, Response, Promise, URL, console, timers, fetch,
  performance). Cada spec lista metodos estaticos e de instancia com seus simbolos ABI.
- `handles.rs` — constantes e helpers de ABI para o `HandleTable` (encode/decode gen+slot).
- `types.rs` — `AbiType`: `Void | Bool | I32 | I64 | U64 | F64 | StrPtr | Handle`. `StrPtr`
  expande para dois slots Cranelift (`ptr` + `len`). `Bool` mapeia para `I64` em Cranelift
  (funcoes extern "C" retornam i64, nao i8).
- `signature.rs` — `lower_member()` converte a spec em `LoweredSignature` Cranelift.
- `symbols.rs` — convencao `__RTS_<KIND>_<SCOPE>_<NS>_<NAME>` (ex: `__RTS_FN_NS_IO_PRINT`,
  `__RTS_FN_GL_NUMBER_IS_NAN`). Macro `rts_sym!` gera simbolos em compile-time;
  `validate_symbol()` impoe uppercase ASCII.
- `guards.rs` — `guard_for(expected, caller)` decide passthrough/coerce/trap em call sites
  com argumentos de tipo `any`.

Codegen emite `call <symbol>` direto via Cranelift, sem intermediarios.

## Estrutura de Arquivos por Namespace

```
crates/rts-runtime/src/namespaces/<ns>/
  mod.rs         — re-exporta submodulos e publica a NamespaceSpec
  abi.rs         — declaracao dos NamespaceMember (tabela estatica)
  <grupo>.rs     — impl operacional (ex: read.rs, write.rs, dir.rs, print.rs, stdout.rs, ...)
```

Regras:
- `mod.rs` e apenas o import map + export do `NamespaceSpec`
- `abi.rs` e a fonte da verdade dos membros do namespace (nome, simbolo, args, return, doc, ts)
- Cada arquivo operacional agrupa funcoes por responsabilidade (io/r-w/dir/metadata/…)
- Nao existe `dispatch()` por namespace — cada funcao e um `#[no_mangle] extern "C"` direto

Namespaces ativos (40+): `io`, `fs`, `gc`, `math`, `num`, `bigfloat`, `time`, `env`,
`path`, `buffer`, `string`, `process`, `os`, `collections`, `hash`, `fmt`, `crypto`,
`net`, `tls`, `thread`, `atomic`, `sync`, `parallel`, `mem`, `hint`, `ptr`, `ffi`,
`regex`, `runtime`, `test`, `trace`, `ui`, `alloc`, `json`, `date`, `http_server`,
`promise`, `events`, mais os sub-namespaces de `globals/` (number, string, date,
regexp, error, events, console, json, timers, fetch, performance, global_this,
text_encoding, url).
Cobre std::* + paralelismo + HTTPS + UI completos + JSON + Date + HTTP server
nativo via actix-web + classes JS globais completas.

### Namespaces existentes

- `io/` — print, eprint, stdout_{write,flush}, stderr_{write,flush}, stdin_{read,read_line}
- `fs/` — read, read_all, write, append, exists, is_file, is_dir, size, modified_ms,
  create_dir(_all), remove_dir(_all), remove_file, rename, copy
- `gc/` — handles e string pool: string_from_{i64,f64,static}, string_{new,concat,len,ptr,free},
  `HandleTable` slab-based com 16-bit geracao + 48-bit slot (`u64` handle);
  `Entry` enumera tipos armazenados (`String`, `BigFixed`, `Buffer`, `ProcessChild`,
  `Map`, `Vec`, `Free`)
- `math/` — basic (floor/ceil/round/trunc/sqrt/cbrt/pow/exp/ln/log2/log10/abs_f64/abs_i64),
  trig (sin/cos/tan/asin/acos/atan/atan2), minmax (min/max/clamp_f64/i64), consts
  (PI/E/INFINITY/NAN como `MemberKind::Constant`), random (xorshift64 com estado em
  `__RTS_DATA_NS_MATH_RNG_STATE`). Intrinsics: sqrt/abs_f64/min_f64/max_f64/abs_i64/
  min_i64/max_i64/random_f64
- `bigfloat/` — decimal fixed-point via i128 (scale decimal ate 36). Operacoes:
  zero/from_f64/from_i64/from_str/to_f64/to_string/add/sub/mul/div/neg/sqrt/free.
  Usado para pi com 29+ digitos via Machin + atan de Maclaurin
- `time/` — now_ms/now_ns (Instant monotonico), unix_ms/unix_ns (SystemTime),
  sleep_ms/sleep_ns
- `env/` — get_var, set_var, remove_var, args_count, arg_at, cwd, set_cwd
- `path/` — join, parent, file_name, stem, ext, is_absolute, normalize, with_ext
  (operacoes puras, sem I/O)
- `buffer/` — Vec<u8> via HandleTable: alloc/alloc_zeroed/free/len/ptr,
  read/write u8/i32/i64/f64 little-endian, copy/fill, to_string (UTF-8)
- `string/` — search (contains/starts_with/ends_with/find), transform
  (to_upper/to_lower/trim/trim_start/trim_end/repeat), replace/replacen,
  char_count/byte_len/char_at/char_code_at (Unicode-aware)
- `process/` — exit/abort, pid, args_count/arg_at (alias de env), spawn
  (args separados por \\n), wait (consume handle), kill. Child handle via
  `Entry::ProcessChild`
- `os/` — platform/arch/family/eol (std::env::consts + cfg!), home_dir,
  temp_dir, config_dir, cache_dir (XDG no Unix, APPDATA/LOCALAPPDATA no
  Windows)
- `collections/` — HashMap<string, i64> (`map_*`) e Vec<i64> (`vec_*`) via
  HandleTable. Valor sempre i64 — caller interpreta como int/handle/bool
- `hash/` — SipHash-2-4 deterministico para str/i64/bytes (hash_str,
  hash_i64, hash_bytes)
- `fmt/` — parse_i64/f64 (tolerante), fmt_hex/oct/bin/f64_prec
- `crypto/` — SHA-256 inline (FIPS 180-4), base64/hex encode+decode,
  CSPRNG via BCryptGenRandom (Windows) / /dev/urandom (Unix)
- `net/` — TCP listener/stream + UDP socket + DNS resolve via `std::net`.
  Handles via `Entry::TcpListener/TcpStream/UdpSocket(UdpEntry)`. Sync,
  sem deps externas
- `tls/` — TLS 1.2/1.3 client via `rustls` + `webpki-roots` (Mozilla CAs
  embutidos). Wraps `TcpStream` em conexao TLS. HTTPS funciona ponta-a-
  ponta sem OpenSSL nem schannel
- `thread/` — 4 mecanismos coexistindo, dev escolhe pelo workload:
  `spawn` + `join`/`detach` (`std::thread`, JoinHandle real, ~30k spawn/s,
  bom pra CPU-bound longo); `spawn_async_join` + `join_async` (tokio
  `spawn_blocking`, retorna i64, ~400k spawn/s, bom pra leve/IO);
  `spawn_async` (tokio fire-and-forget, ~400k spawn/s); `spawn_detached`
  (pool fixo 8 workers, 5M spawn/s mas queue ilimitada — cuidado OOM).
  Mais `scope` auto-join + `sleep_ms`. Doc-comments em
  `crates/rts-runtime/src/namespaces/thread/abi.rs` tem tabela comparativa
- `http_server/` — servidor HTTP/1.1 nativo via `actix-web` sobre
  runtime tokio compartilhado. Bridge sync→async: `serve(addr,handler)`
  bloqueia, cada request entra num shard map de slots, handler TS
  chamado direto na thread async, response volta via oneshot. Suporta
  keep-alive, pipelining, parsing correto. Pico medido 29k req/s
  (78% do actix puro Rust)
- `atomic/` — `std::sync::atomic`: AtomicI64 (load/store/fetch_*/cas/swap),
  AtomicBool, AtomicF64 (via AtomicU64 + bit-transmute), fences
- `sync/` — `std::sync`: Mutex<i64>, RwLock<i64>, Once. Guards thread-
  local pra atravessar chamadas extern "C"
- `parallel/` — `rayon`: map/for_each/reduce + num_threads. Backing dos
  passes silent (purity_pass, reduce_pass, array_methods_pass)
- `mem/` — size_of/align_of constantes, swap_i64, drop/forget_handle
- `num/` — checked/saturating/wrapping arith, bit ops (rotate, count_ones/
  zeros, leading/trailing_zeros, reverse_bits, swap_bytes), bitcast
  f64<->bits
- `ptr/` — copy_nonoverlapping, raw pointer ops
- `ffi/` — CString, OsString
- `regex/` — backend `regex` crate, compile + test/find/replace/replace_all
- `runtime/` — eval_file (dynamic import) + hot-reload primitives
- `test/` — test_core (suite/case begin/end, fail) + bundle.ts (`rts:test`
  describe/test/expect)
- `trace/` — push/pop/capture/print frame stack pra erros estilo Bun
- `ui/` — FLTK 1.x bindings (Button, Window, Input, Slider, ...)
- `alloc/` — malloc-style raw allocations
- `hint/` — black_box, spin_loop, unreachable, assert_unchecked
- `promise/` — Promise stub (resolve/reject/then/catch)
- `events/` — EventEmitter primitivo (on/off/once/emit/removeAllListeners/listenerCount)

### Globals (`crates/rts-runtime/src/namespaces/globals/`)

Classes JS globais implementadas como sub-namespaces sob `globals/`:

```
crates/rts-runtime/src/namespaces/globals/<class>/
  mod.rs   — re-exporta e publica GlobalClassSpec
  abi.rs   — tabela de membros (estaticos + instancia)
  rt.rs    — implementacao extern "C"
```

- `number/` — `Number`: isNaN/isFinite/isInteger/isSafeInteger, MAX_SAFE_INTEGER,
  MIN_SAFE_INTEGER, NaN, POSITIVE_INFINITY, NEGATIVE_INFINITY, EPSILON;
  instancia: toFixed/toString(radix)/toExponential/toPrecision/valueOf; coercao `Number(x)`
- `string/` — `String`: fromCharCode; instancia: length, charAt, charCodeAt, indexOf,
  lastIndexOf, includes, startsWith, endsWith, slice, substring, split, join, replace,
  replaceAll, trim, trimStart, trimEnd, padStart, padEnd, repeat, toUpperCase, toLowerCase
- `date/` — `Date`: now()/parse(); instancia: getTime/getFullYear/getMonth/getDate/
  getHours/getMinutes/getSeconds/getMilliseconds, toISOString/toLocaleDateString
- `regexp/` — `RegExp`: instancia: test/exec/source/flags/global/ignoreCase/multiline
- `error/` — `Error`, `TypeError`, `RangeError`, `SyntaxError`: message/name/toString
- `events/` — `EventEmitter`: on/off/once/emit/removeAllListeners/listenerCount;
  construtor aceita bool `async` (segundo arg)
- `console/` — `console`: log/error/warn/info/debug/time/timeEnd/assert/dir/count
- `json/` — `JSON`: parse/stringify (bridge para namespace `json`)
- `timers/` — `setTimeout`/`setInterval`/`clearTimeout`/`clearInterval` (sync stubs)
- `fetch/` — `fetch()` global via TLS/TCP
- `performance/` — `performance.now()` (monotonic ms)
- `global_this/` — `globalThis`, `undefined`, `null`, `Infinity`, `NaN`; globals `isNaN`/
  `isFinite`/`parseInt`/`parseFloat`/`encodeURIComponent`/`decodeURIComponent`
- `text_encoding/` — `TextEncoder`/`TextDecoder` (UTF-8)
- `url/` — `URL`: href/protocol/host/pathname/search/hash/searchParams
  + `URLSearchParams`: get/set/has/delete/append/getAll/keys/values/
  entries/toString
- `symbol/` — `Symbol`: `Symbol(desc)`, `Symbol.for(key)`, `keyFor`,
  `description`, `toString`; well-known: `iterator`, `asyncIterator`,
  `hasInstance`, `toPrimitive`, `toStringTag`
- `weakmap/`, `weakset/` — stubs com semantica strong (refs fortes
  no HandleTable). Issue #217 rastreia FinalizationRegistry e weak
  refs reais
- `boolean/` — `Boolean`: `toString`, `valueOf`, coercao `Boolean(x)`

## Silent parallelism (Level-1)

O codegen tem 3 passes que reescrevem padroes TS comuns para chamadas
`parallel.*` automaticamente. User nao precisa mencionar threads/workers:

- **`array_methods_pass`** — detecta `arr.map(fn)`, `arr.forEach(fn)`,
  `arr.reduce(fn, init)` quando `fn` e Ident de user fn → reescreve para
  `parallel.map/for_each/reduce`. Roda primeiro.
- **`reduce_pass`** — detecta padrao classico de acumulador
  (`let s = 0; for (x of arr) s = s + EXPR;` ou `s += EXPR`) e reescreve
  para `parallel.reduce`. So aceita ops associativas (+, *).
- **`purity_pass`** — detecta `for...of` cujo corpo so chama membros
  `pure: true` de namespaces e nao tem assignments → reescreve para
  `parallel.for_each`.

Os 3 passes cobrem top-level + body de cada user fn. Counters
compartilhados sem colisao de nomes. 96 fns marcadas `pure: true` hoje
(math, string, num, fmt, path, hash, mem) — base do reconhecimento.

`parallel.*` aceita arrays literais, em variavel, e retornados de fn
(todos viram Vec<i64> via codegen de array literal). Bridge pra Buffer
e typed arrays e follow-up.

## HandleTable shard-aware

`HandleTable` esta dividido em 32 shards lock-free entre si. `alloc_entry`
distribui round-robin por thread; `shard_for_handle` decodifica O(1) o
shard de qualquer handle (encoded nos low bits). Todos os 17 namespaces
handle-based migrados pra essa API — sem contenção em workloads paralelos.

## Runtime tokio compartilhado (issue #399)

`crates/rts-runtime/src/runtime/async_rt.rs` exporta `rt()` — `OnceLock<tokio::runtime::Runtime>`
multi-thread global. Hooks `on_thread_start`/`on_thread_stop` registram cada
worker no `gc/thread_registry` para o GC scanner ver handles vivos em tasks
tokio (sem isso o sweep coletava indevidamente sob carga concorrente).

Toda feature async deve reusar este runtime em vez de criar um proprio:

- `http_server::serve` chama `rt().block_on(...)`
- `thread::spawn_async*` usa `rt().handle().spawn_blocking(...)`
- `runtime::tokio_ctx` oferece "id u64 opaco + shard map por TypeId"
  como bridge sync↔async generico (substitui `slots()` ad-hoc do http_server)

Convencao: o que cruza o JIT (extern "C") e' apenas u64 opaco. Tipos
Rust-rich (Arc<T>, Channel, JoinHandle) ficam no shard map indexado por
esse id.

## GC stack scanner Win32

`mark_stack_roots()` em `crates/rts-runtime/src/namespaces/gc/collector.rs` usa
`GetCurrentThreadStackLimits` (API Win32 oficial) em vez de `gs:[0x10]`
da TIB. O TIB.StackBase em alguns contextos retornava valor < RSP,
deixando o scanner sem marcar nada e o sweep coletando handles vivos
(bug encontrado em 2026-05-01 testando http_server sob carga). Mesmo
caminho usado para varrer threads no `thread_registry` via
`SuspendThread + GetThreadContext` + scan de registers callee-saved.

## Disciplina de regressao zero

Rever a `REGRA OBRIGATÓRIA: ZERO REGRESSÃO ANTES DE MERGE` no topo deste
arquivo. Em projeto com IA acelerando velocidade, eh essa regra que
mantem a suite confiavel ao longo de centenas de PRs.

## Capacidades de linguagem ativas (codegen)

- Object/array literals: `{k: v}` e `[1,2,3]` via `collections.map_*`/`vec_*`.
- Classes: constructor, method, this, extends, super(args), super.method(args),
  static methods, getters/setters. Instance armazena `__rts_class` para
  dispatch virtual real (override em subclasse roteado via comparacao de
  string sobre o tag de runtime).
- Operator overload Rust-style: `a + b` vira `a.add(b)` em compile-time
  quando classe define o metodo (`add`/`sub`/`mul`/.../`eq`/`lt`/`bit_*`).
- for...of em arrays; bind herda classe quando array tem anotacao `: C[]`.
- try/catch/finally fase 1: slot de erro thread-local, sem unwind real
  (#128 rastreia fase 2).
- String equality: `s1 == s2` compara conteudo via `gc.string_eq`.

## Convencoes

- Linguagem do codigo: Rust (ingles nos identificadores)
- Linguagem de comunicacao: portugues
- Commits seguem conventional commits: `feat:`, `fix:`, `perf:`, `refactor:`, `docs:`, `chore:`
- Novo namespace precisa ser registrado em: `abi::SPECS` (e o `rts.d.ts` gerado a partir dai)
- O `rts.d.ts` e gerado a partir de `abi::SPECS` — CI lintao committed file contra o gerador
- Build e via `cargo` direto — `xtask` foi removido

## Progress bar em tarefas longas

Quando o usuario pede um trabalho com varias etapas (ex: novo namespace,
feature feat:js/feat:ts, fix multi-arquivo) mostra uma barra de progresso
ASCII a cada modificacao significativa, ancorando a percepcao do usuario
do quanto falta.

Formato:

```
[▰▰▰▱▱▱▱▱▱▱] 30% — descricao curta da etapa atual
```

Regras:
- 10 segmentos: `▰` preenchido, `▱` vazio. Percentual eh o valor real,
  nao o numero de segmentos (ex: 25% = 2 segmentos cheios + 50% do 3o
  arredondado pra cheio).
- Atualizar a cada modificacao concreta: arquivo criado, build passou,
  test rodou, commit feito.
- Em caso de erro: prefixar `❌ erro:` e voltar a percentagem para o
  ponto onde a confianca caiu. Continuar a partir dali.
- Marco final: `[▰▰▰▰▰▰▰▰▰▰] 100% ✅ — resumo (PR #N, X/Y testes)`.

Exemplos de etapas tipicas (namespace novo):
- 10% mod.rs criado
- 25% abi.rs definido
- 45% ops.rs implementado
- 55% rt.rs criado
- 70% registrado em SPECS + mod.rs + rt_all
- 80% JIT registrado + build.rs atualizado
- 90% build passou + teste basico ok
- 100% PR aberto/merged

## Assumindo issues do GitHub

Quando comecar a trabalhar em uma issue (ex: usuario diz "vamos fazer
a #97"), antes de codar marca a issue como assumida via `gh issue edit`
ou comentando — para que outros contribuintes saibam que ja tem alguem
trabalhando.

Forma minima: comentar na issue indicando inicio de trabalho.

```bash
gh issue comment <num> --body "Assumindo essa issue. Trabalho em andamento."
```

Quando possivel, atribuir a si mesmo via `gh issue edit <num> --add-assignee @me`
(funciona se a conta autenticada e collaborator do repo).

Ao terminar (PR mergeado), comentar de novo com link do PR e fechar
quando apropriado.

## Criatividade ao testar

Ao adicionar/modificar features, nao basta um teste happy-path. Seja
criativo e cubra varias variacoes de codigo na pasta `tests/`:

- Caminho normal **e** caminhos atipicos (vazio, condicional, aninhado,
  dentro de loop, dentro de try/catch, em member call, etc).
- Combinar a feature com features adjacentes (ex: arrow + classe,
  arrow + generics, arrow + spread).
- Casos de borda do TS/JS — undefined, null, recursao, tail call,
  identificadores comuns (`__rts*`, `this`, palavras reservadas).
- Quando uma variacao falhar e estiver fora do escopo da PR atual,
  abrir issue com o snippet minimo que reproduz e remover do teste
  ate o follow-up.

Os testes vivem em `tests/*.test.ts` (formato `rts:test`). Reaproveite
o template padrao: `__rtsCapturedOutput`, `print()` shim, `describe()`
com 1 ou mais `test()`/`expect().toBe()`. Multiplos `test()` por arquivo
sao bem-vindos pra cobrir variacoes sem inflar o numero de arquivos.

## Status do epic #226 (paridade JS/TS)

Suite TS: **1015/1015 (100%)**. Lotes recentes (sessao 2026-05-09) entregaram:

- **#213/#618/#619** module system completo: named/default/star imports,
  re-exports, alias `import { x as y }`, `export * as ns`, AOT module graph
- **#218** Reflect API completa (13 metodos) + Proxy fase 1+2+3 com 13 traps
  (get/set/has/delete/apply/construct/ownKeys/getPrototypeOf/setPrototypeOf/
  defineProperty/getOwnPropertyDescriptor/isExtensible/preventExtensions)
- **#224** ui.app_check/add_timeout/repeat_timeout/add_idle
- **#261** computed key (`obj["x"]` e `obj[k]`) propagam field type
- **#287** node:fs.readFileSync funcional (READ_TEXT)
- **#289 fase SHA-256** streaming hash via `sha2` crate
  (`createHash`/`update`/`digest`) + `randomUUID` v4
- **#377** setTimeout/setInterval/setImmediate + clear* (cobertura formal)
- **#383** ident desconhecido vira erro de compilacao (era warning + segfault)
- **#398** GC scanner com transitive marking em Map/Vec/Function/Proxy
- **#407** top-level globals registrados como GC roots (cobertura formal)
- **#450** \`arguments.length\` em fn user nao-arrow + fix em arrow VarDecl
- **#573** \`console.log(null)\` -> \"null\", U64 ambiguo via TPL_COERCE_AUTO
- **#584** divisao `/` segue JS spec (sempre f64)
- **#592** \`users[i].field\` em \`Cls[]\` propaga tipo
- **#602** optchain 3+ niveis em obj literal
- **#617** AOT classes (Function archive)

Issues filhas pesadas que continuam abertas (refactor necessario, fora
de escopo de PR pequena):

- **#195** mutable closures (env-record refactor; bloqueado por #90)
- **#207** event loop async/await real
- **#216** Symbol como chave computada
- **#217** WeakMap/WeakSet semantica fraca + FinalizationRegistry
- **#222** Map/Set Symbol.iterator real
- **#223** dynamic import
- **#301** var hoisting em fn user (top-level ja' funciona)
- **#304** toString/valueOf em coercao implicita
- **#305** integer overflow JS spec (i64 wraparound vs f64 promotion)
- **#477** generator infinito (vec push estoura — refator state machine)
- **#211/#219/#225** generators / BigInt / Intl (candidate-discard)

## Como testar

```bash
cargo test --lib                                  # testes unitarios Rust
cargo build --release                             # build release
$env:RUST_BACKTRACE="full"; target/release/rts.exe run file.ts                # executar via JIT in-memory
$env:RUST_BACKTRACE="full"; target/release/rts.exe compile -p file.ts output  # compilar nativo (AOT)
$env:RUST_BACKTRACE="full"; target/release/rts.exe test tests/foo.test.ts     # rodar suite TS
target/release/rts.exe apis                       # listar APIs disponiveis
```

**Padrão obrigatório:** sempre definir `RUST_BACKTRACE=full` antes de executar o `rts.exe`.
Sem isso, crashes e panics mostram stack trace raso (símbolos stripped, frames FLTK/STL
misturados sem contexto). Com `full`, o crash handler em `src/crash.rs` exibe o trace
completo com localização de arquivo e linha (em build debug) ou pelo menos nomes de símbolo
Rust desmanglados (em release).

```powershell
# PowerShell — definir uma vez na sessão:
$env:RUST_BACKTRACE = "full"
```

```bash
# Bash/sh:
export RUST_BACKTRACE=full
```

### Iteracao rapida: `cargo run -- run` vs `build --release`

Para iterar em mudancas de Rust, escolha o modo pelo objetivo:

| Comando | Quando usar | Tempo full rebuild | Binario |
|---|---|---|---|
| `cargo run -- run file.ts` | Iterar fix de codegen/runtime, ver se compila + roda | ~30s (debug) | lento (~10x release) |
| `cargo run --release -- run file.ts` | Mesmo que abaixo, em um comando so | ~100s | rapido |
| `cargo build --release` + `target/release/rts.exe run file.ts` | Benchmarks, suite TS completa, validar performance | ~100s | rapido |
| `target/release/rts.exe run file.ts` (direto) | Re-rodar `.ts` sem mudar Rust | 0s (skip cargo) | rapido |

Pontos importantes:

- **`cargo run` SEMPRE checa staleness e recompila se algo mudou.** Nao existe "rodar sem compilar" — a frase eh incorreta. Mudancas em Rust **entram** porque o cargo recompila antes de executar.
- **Debug (`cargo run -- run`) eh ~3x mais rapido pra compilar** que release, mas o binario gerado eh ~10x mais lento. Bom pra "compila? executa? bate o caminho que eu mudei?". Ruim pra qualquer medicao de tempo.
- **Para benchmarks, suite TS, ou qualquer validacao de performance**, use sempre `--release`. Numero de debug mente.
- **Se voce so' mudou `.ts` (nao Rust)**, chame `target/release/rts.exe` direto — pula o overhead de ~100s do cargo checar o workspace.
- `cargo run` envolve o exit code do programa (exit 1 do programa vira "process didn't exit successfully" no cargo). Isso eh esperado, nao eh bug.

Testes de codegen vivem em `tests/*.test.ts` (formato `rts:test`). Para
adicionar novo teste, criar `tests/<name>.test.ts` com:

```ts
import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// codigo a testar no top-level (resultados pre-computados)
const result = expr;

describe("<name>", () => {
  test("<caso>", () => expect(result).toBe(expected));
  test("saida capturada", () => expect(out).toBe("...\n"));
});
```

**Regra de ouro:** pre-computar valores no top-level (antes dos `describe`).
Chamar metodos de instancia diretamente dentro de closures `test()` pode
causar problemas de GC (handle coletado antes do uso).

## Debug do codegen — `rts ir`

Para inspecionar o IR Cranelift gerado de qualquer programa antes
do define+compile, use o comando `rts ir`:

```bash
target/release/rts.exe ir file.ts 2>&1 | head -100
```

Para snippets curtos sem precisar criar arquivo temp, use `-e` ou `eval`
com `rts run` — mas para ver o IR de um snippet crie um arquivo temporario
e rode `rts ir`:

```bash
target/release/rts.exe ir file.ts 2>&1 | head -30
```

Imprime o IR completo de cada `user fn` mais o `__RTS_MAIN`
(top-level). Saida vai para stderr. Nao executa o programa.

**Use sempre `-e`/`eval` para snippets de teste/debug** — evita
criar arquivos temporários soltos no projeto. Imports relativos
(`./mod`) nao funcionam em eval (so' builtins `import { x } from "rts"`).

**Quando o Claude deve usar isso:** sempre que estiver debugando
desempenho ou suspeitando de codegen ineficiente. Ler o IR mostra
imediatamente:

- loops com `load`/`store` redundantes (vars nao promovidas a
  Cranelift Variable, sites sem cache de `gv`);
- subexpressoes lower duplicadas (try_operator_overload /
  try_bin_imm chamando lower_expr antes de checar se vao usar);
- `uextend` desnecessarios em comparacoes que vao direto pro `brif`;
- conversoes f64↔i32 em loop hot (literals como `1.0` mal-classificados);
- `global_value` repetidos para o mesmo simbolo;
- chamadas extern (calls externas) que poderiam ser intrinsics inline.

**Padrao de uso:**

1. Rodar bench (RTS lento? conferir gap com Bun/Node).
2. `rts ir file.ts 2>&1 | sed -n '/<fn-de-interesse>/,/^---/p'` —
   isolar a fn problematica.
3. Olhar `block` que e' header/body do hot loop. Procurar:
   - quantos `load`/`store` por iteracao (idealmente 0 para vars locais);
   - quantos `call` (cada call extern e' caro);
   - duplicacao de subexpressoes (mesma `fmul`/`fadd` repetida).
4. Identificar a causa no codegen (`crates/rts-codegen/src/codegen/lower/`) e corrigir.
5. Re-dump pra confirmar; rodar `cargo test --release --lib` +
   `target/release/rts.exe test` pra garantir 0 regressao.

**Exemplo real (commit 4a418d1):** `x*x + y*y <= 1.0` em loop tinha
6× `fmul x x` + 3× `fmul y y` + 3× `fadd` no IR — `try_operator_overload`
e `try_bin_imm` faziam lower duplicado de subexprs antes de saber se
iam usar. Fix reduziu pra 1× cada (~6% mais rapido em Monte Carlo).

## Benchmarks

Benches canonicos em `bench/`:

- `monte_carlo_pi.ts` — estimacao de pi por Monte Carlo 10M (xorshift64 inline)
- `pi_bigfloat.ts` — pi via Machin 30 digitos usando `bigfloat`
- `pi_machin.ts` — pi via Machin em f64 (16 digitos)

Placar atual (medianas, atualizado 2026-05-01):

| Bench                       | RTS JIT | RTS AOT | Bun    | Node    |
|-----------------------------|---------|---------|--------|---------|
| Monte Carlo 10M             | 26.8 ms | 16.9 ms | 91.8 ms| 113.9 ms|
| Monte Carlo 10M (8 workers) | 30.3 ms | —       | 147.6 ms (Workers) | — |

RTS AOT vs Bun: **5.14× mais rapido**. RTS multi-thread vs Bun Workers:
**4.66× mais rapido**. Numeros antigos do CLAUDE.md (JIT 119ms / AOT
156ms) eram pre-otimizacoes — fix em `try_operator_overload`,
`try_bin_imm`, intrinsics inline, jump tables, etc.

HTTP server (issue #399 + actix-web): pico **29k req/s** (78% do actix
puro Rust em mesmo workload, 2× mais que `Bun.serve`).

Suite completa:

```bash
powershell.exe -ExecutionPolicy Bypass -File bench/benchmark.ps1
```

## Regras

- Nao implementar APIs de alto nivel em Rust — Rust so expoe primitivas raw via `"rts"`
- Classes JS globais (Number, String, Date, etc.) vivem em `crates/rts-runtime/src/namespaces/globals/<class>/`
  e sao registradas em `GLOBAL_CLASS_SPECS`; codegen as resolve via `global_class_lookup`
- `rts.d.ts` so contem `declare module "rts"` — nao adicionar outros modulos
- Handles numericos (u64) para recursos runtime (buffers, sockets, strings dinamicas, etc)
- Distribuicao standalone: runtime support resolvido por objetos `.o/.obj`
  precompilados (via `RTS_RUNTIME_OBJECTS_DIR` ou pasta `runtime-objects` ao lado do `rts`);
  nao dependemos de download externo em tempo de build

## Sem Codigo Legacy

**Regra absoluta: codigo morto e removido imediatamente. Nunca comentar, nunca deixar "por precaucao".**

- Qualquer codigo que nao e chamado por nenhum caminho vivo deve ser deletado no mesmo commit
  que o tornou morto
- Stubs `todo!()` / `unimplemented!()` sao aceitaveis como marcador temporario de WIP;
  codigo comentado nao
- Warnings de `dead_code` sao tratados como erros — o build nao pode terminar com warnings

## ABI de Maquina — extern "C" tipado, sem dispatch

Nao ha `JsValue`, nem `__rts_call_dispatch`, nem boxing no limite entre codegen e runtime.
Cada funcao de namespace e um simbolo `extern "C"` tipado.

### Convencao ABI por tipo

| Tipo TS  | `AbiType`    | Representacao Cranelift         | Observacao                                              |
|----------|--------------|---------------------------------|---------------------------------------------------------|
| `number` | `I64` / `F64`| `i64` / `f64`                   | bits nativos, sem boxing                                |
| `bool`   | `Bool`       | `i64` (0/1)                     | 0 = false, 1 = true; assinatura Cranelift usa I64        |
| `string` | `StrPtr`     | 2 slots: `(i64 ptr, i64 len)`   | UTF-8; ptr estatica do codegen, ou buffer via handle GC |
| handle   | `Handle`     | `u64`                           | `HandleTable` (gen:16 + slot:48)                        |
| void    | `Void`       | —                               | sem retorno                                             |
| inteiros| `I32` / `U64`| `i32` / `u64`                   | usados em contagens, status, tamanhos                   |

### Regras de implementacao

- Cada membro de namespace vira um `#[unsafe(no_mangle)] pub extern "C" fn __RTS_FN_NS_<NS>_<NAME>(...)`
- Nenhuma funcao de namespace aceita/retorna `JsValue` no limite `extern "C"`
- Strings dinamicas (ex: resultado de leitura) sao alocadas pelo `gc` e retornam um handle `u64`;
  leitura via `gc::string_ptr(handle)` + `gc::string_len(handle)`
- Call sites com argumentos `any` passam por `abi::guards::guard_for(...)` para decidir coerce/trap

## Runtime vs Compile

Dois caminhos de execucao compartilhando o mesmo codegen Cranelift:

- **`rts run`**: compila direto para memoria executavel via `JITModule`. Sem disco, sem
  linker externo. Todos os simbolos do ABI sao registrados em `JITBuilder::symbol` no
  startup do modulo JIT (`crates/rts-codegen/src/codegen/jit.rs`).
- **`rts compile`**: aplica slicing por uso, gera apenas os objects dos modulos efetivamente
  utilizados, produz binario final.

`FnCtx.module` e `&mut dyn Module` — `ObjectModule` e `JITModule` implementam o mesmo trait
e passam pelo mesmo pipeline de `compile_program`.

Convencao de nomes de object: `<module>.o` (e `.m` quando houver metadata para cache
incremental).

## Otimizacoes de codegen notaveis

- **Intrinsics inline** (`abi::Intrinsic`): `sqrt`, `abs_f64`, `min/max_f64`, `abs_i64`,
  `min/max_i64`, `random_f64` — emitidos como IR Cranelift direto em `lower_intrinsic`
- **Tail call optimization**: user functions em `CallConv::Tail`; `return f(x)` em posicao de
  tail emite `return_call` (exige `preserve_frame_pointers=true` em x86-64)
- **First-class function pointers** (#97 fase 1): `Expr::Ident` resolvendo a user fn
  materializa `func_addr` como i64; call via ident local/param faz `call_indirect` com
  signature provisoria Tail
- **Jump table switch**: quando todos os non-default cases sao literais inteiros, usa
  `cranelift_frontend::Switch` (backend decide `br_table` vs binary search)
- **Imm forms**: `x + N` / `x & MASK` / `x << K` emitem `iadd_imm` / `band_imm` / `ishl_imm`
  sem iconst intermediario
- **MemFlags::trusted** em loads/stores de globals e RNG state
- **f64 modulo** via libc `fmod` (antes truncava via i64 perdendo a parte fracionaria)
- **Constantes como propriedades** (`math.PI` sem parens) via `MemberKind::Constant` +
  `emit_constant_load`

## Otimizacoes pendentes / backlog

Ver issues abertas #90, #96, #97 (fases 2/3). #92 autovec foi fechada como inviavel sem
loop vectorizer proprio (Cranelift nao tem um); Bun ganha em Monte Carlo >1B iter por
autovec do V8.

## Layout de Artefatos do Usuario

Alvo da Fase 1 do roadmap (em progresso):

```
<project>/
  src/main.ts
  package.json
  tsconfig.json

  node_modules/.rts/
    objs/
      runtime/        — objects completos do builtin (todos os modulos)
      compile/        — objects AOT com slicing (apenas em rts compile)
    modules/          — modulos resolvidos e cacheados (com metadata .ometa)

  release/            — apenas em rts compile
    <project_name>    — .exe / .dll / .so / .node conforme target
```

## GC — mark+sweep com Cranelift stack maps

**Estado atual (2026-05-01):** o crate `gc-arena = "0.5"` esta declarado no
`Cargo.toml` mas **nao esta integrado de fato**. O sistema real e' mark+sweep
preciso usando `UserStackMap` do Cranelift, com scanner conservativo via
`SuspendThread + GetThreadContext` para cobrir todas as threads RTS
registradas no `thread_registry`. Detalhes:

- Codegen chama `builder.declare_value_needs_stack_map(val)` para cada handle
- `jit.rs` extrai `UserStackMap` apos `define_function` e registra
  return-PC absolutos no `stack_map_registry`
- A cada N alocacoes (`GC_TICK_INTERVAL = 256`), `finish_cycle()` roda
  `mark_stack_roots()` (varre stack da thread atual + stacks de outras
  threads via SuspendThread) e `sweep_all_shards()` libera o que nao foi
  marcado
- `mark_stack_roots()` no Windows usa `GetCurrentThreadStackLimits` (API
  Win32 oficial). Nao usar `gs:[0x10]` — em alguns contextos retorna
  StackBase < RSP, deixando o scanner sem marcar nada e o sweep coletando
  handles vivos (bug PR #400)

**Migracao real para gc-arena** (issue #393) seria refator grande: todas
as 25+ variantes de `Entry` precisariam derivar `Collect`, com
`Mutation<'gc>` token cruzando o JIT — incompativel com a ABI extern "C"
plana atual. Adiada.

A intenção original (do CLAUDE.md historico) era usar gc-arena com
`safe_collect()` em pontos de quiescencia (retorno de fn, fim de metodo
de classe, fim de closure). Esse modelo **nao e' o atual** — coleta hoje
e' periodica via tick-counter. Atualizar este texto se a migracao
acontecer.

## State

Estado de namespace usa `Arc<Mutex<T>>` direto quando necessario, ou `thread_local!` para caches
por-thread. Nao ha sistema centralizado de state — cada namespace gerencia o seu.

### Pattern para estado compartilhado

```rust
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

static FS_STATE: std::sync::OnceLock<Arc<Mutex<FsState>>> = std::sync::OnceLock::new();

fn fs_state() -> Arc<Mutex<FsState>> {
    FS_STATE.get_or_init(|| Arc::new(Mutex::new(FsState::default()))).clone()
}

#[derive(Default)]
struct FsState {
    open_files: HashMap<u64, std::fs::File>,
}
```

### Pattern para caches thread-local

```rust
use std::cell::RefCell;

thread_local! {
    static EXPR_CACHE: RefCell<HashMap<u64, Expression>> = RefCell::new(HashMap::new());
}

pub fn reset_cache() {
    EXPR_CACHE.with(|cache| cache.borrow_mut().clear());
}
```

## Docs e especificacoes

A pasta `docs/specs/` contem especificacoes de features, decisoes de design e notas tecnicas.
Consultar o indice em `docs/specs/INDEX.md`. Direcao de alto nivel fica em `RTS_REFACTOR.md`
na raiz (plano canonico do refator em workspace de crates).
