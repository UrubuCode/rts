# RTS Standard Surface — redesign da superfície `rts:*` (v1)

> **Status: PROPOSTA APROVADA EM DIREÇÃO** (cutover duro; módulos `rts:<ns>`;
> buffer absorvido por TypedArrays; execução em fases após aprovação deste
> mapa). Decisões de 2026-07-05 com o owner. Este doc é o mapa canônico
> membro-a-membro; a execução referencia as fases no fim.
>
> Tese: **o RTS exporta o std do Rust para o ambiente JS/TS** — um ambiente
> rico, não limitado — e some com tudo que duplica o que a linguagem JS já
> dá. A superfície é dividida em três anéis: GLOBAIS (spec JS/Web, zero
> import), MÓDULOS `rts:*` (a plataforma nativa, o diferencial), e INTERNO
> (plumbing do motor, invisível no `rts.d.ts`).

## Convenções (binding)

1. **camelCase em todos os membros** (`read_file` → `readFile`). O rename é
   SÓ na superfície TS/Registry (`Member.name`/`ts_signature`); os símbolos
   `__RTS_FN_*` do ABI/linker NÃO mudam.
2. **Import por módulo**: `import { readFile } from "rts:fs"`. O specifier
   `"rts"` único morre (cutover duro — testes/fixtures reescritos no mesmo
   PR de cada fase). Espelha `node:*`; habilita slicing AOT por módulo.
3. **Zero duplicata do JS**: se a linguagem cobre (Promise, Map, JSON, Date,
   Math, RegExp…), não existe namespace. Os externs viram símbolos internos
   consumidos pelo motor/preludes, fora do `rts.d.ts`.
4. **Bytes = `Uint8Array`/`ArrayBuffer`** (primordiais). O namespace `buffer`
   morre; toda API nativa que fala bytes recebe/retorna TypedArray.
5. Nenhum membro novo em snake_case; gate de CI lint contra o gerador de
   `rts.d.ts`.

---

## Anel 1 — GLOBAIS (zero import, spec JS/Web)

Já existem; ficam como estão (só saem dos "namespaces" e passam a globais
declarados no d.ts): `console`, `JSON`, `JSON5`, `Math`, `Date`, `Promise`,
`RegExp`, `Map`/`Set`/`WeakMap`/`WeakSet`, `WeakRef`/`FinalizationRegistry`,
`Symbol`, `BigInt`, `Proxy`/`Reflect`, `Error`+família, `fetch`/`Headers`/
`FormData`/`Request`/`Response`/`Blob`/`File`, `URL`/`URLSearchParams`,
`TextEncoder/Decoder(+Streams)`, `ReadableStream`/`WritableStream`/
`TransformStream`/`CompressionStream`, `Event`/`EventTarget`/
`AbortController`/`AbortSignal`, `MessageChannel`/`MessagePort`,
`setTimeout`/`setInterval`/`setImmediate`+clears, `queueMicrotask`,
`structuredClone`, `atob`/`btoa`, `performance`, `crypto` (ver rts:crypto),
`ArrayBuffer`/`SharedArrayBuffer`/`DataView`/TypedArrays, `Atomics`, `Intl`,
`DOMException`, `EventEmitter` (compat node), `globalThis`.

---

## Anel 2 — MÓDULOS `rts:*` (o std do Rust)

### `rts:fs`
| Novo | Vem de |
|---|---|
| `readBytes(path): Uint8Array` | fs.read_all (Buffer→TypedArray) |
| `readText(path): string` | fs.read_text |
| `writeText(path, s)` / `writeBytes(path, b: Uint8Array)` | fs.write / fs.write_bytes |
| `appendText(path, s)` | fs.append |
| `exists`, `isFile`, `isDir` | exists / is_file / is_dir |
| `size(path): number` | size |
| `modifiedMs(path): number` | modified_ms |
| `createDir`, `createDirAll`, `removeDir`, `removeDirAll`, `removeFile`, `rename`, `copy` | idem snake |
| `readDir(path): string[]` | readdir |
| **NOVO** `watch(path, cb)` | crate notify — file watching |
| **NOVO** `mmap(path): Uint8Array` | view zero-copy memory-mapped |
| **NOVO** `lock(path)` / `unlock(path)` | file locks |
| **COMPTIME** `includeBytes(path): Uint8Array` | embute o arquivo no binário no BUILD (erro de build se ausente) |
| **COMPTIME** `includeText(path): string` | idem, string |

Aliases node-style (`readFileSync` etc) saem daqui — compat Node vive só em
`node:fs` (rts-node).

### `rts:io`
`print`, `eprint`, `stdout.write/flush`, `stderr.write/flush`,
`stdin.read(): Uint8Array`, `stdin.readLine(): string`.

### `rts:net`
TCP: `tcpListen`, `tcpAccept`, `tcpConnect`, `tcpSend(h, b: Uint8Array)`,
`tcpRecv(h): Uint8Array`, `tcpLocalAddr`, `tcpClose`. UDP: `udpBind`,
`udpSendTo`, `udpRecvFrom`, `udpLastPeer`, `udpLocalAddr`, `udpClose`.
DNS: `resolve`. TLS (absorve o ns `tls`): `tlsConnect`, `tlsSend`,
`tlsRecv`, `tlsClose`. **NOVO**: `unixListen`/`unixConnect` (Unix domain
sockets; named pipes no Windows) — IPC real.

### `rts:http`
`serve(addr, handler)`, `request.method/path/body`, `respond` (absorve
`http_server`, camelCase).

### `rts:process`
`exit`, `abort`, `pid`, `args(): string[]`, `cwd`, `setCwd`, `spawn`,
`wait`, `kill`, `env.get/set/remove` (absorve ns `env`). **NOVO**:
`onSignal("SIGINT", cb)` — sinais reais.

### `rts:os`
`platform`, `arch`, `family`, `eol`, `homeDir`, `tempDir`, `configDir`,
`cacheDir`. Versões **COMPTIME** (`target.os/arch/family` consts folded →
dead-code elimination por plataforma) em `rts:build`.

### `rts:path`
`join`, `parent`, `fileName`, `stem`, `ext`, `isAbsolute`, `normalize`,
`withExt`.

### `rts:time`
`nowMs`, `nowNs` (monotônico), `unixMs`, `unixNs`, `sleepMs`, `sleepNs`.

### `rts:thread`
Absorve `thread` + `sync` + `atomic` + `parallel` numa superfície coesa:
- `spawn(fn, arg?): Thread` (+ `join`/`detach`), `scope(cb)`, `sleepMs`,
  `id`, task-pool (`spawnAsync`/`joinAsync`/`spawnDetached`).
- `Mutex`, `RwLock`, `Once` (mutex_new/lock/… → classes registry).
- `AtomicI64`/`AtomicBool`/`AtomicF64` + `fence*` (o global `Atomics` cobre
  SharedArrayBuffer; estes cobrem células avulsas).
- `parallelMap`, `parallelForEach`, `parallelReduce`, `numThreads` (rayon).
- **NOVO** `channel<T>(): [Sender, Receiver]` — mpsc.
O modelo de execução/regiões: `docs/specs/rts-threading-model.md`.

### `rts:crypto`
`sha256(b: Uint8Array|string): Uint8Array`, streaming `Hash`
(`createHash("sha256")`), `randomBytes(n): Uint8Array`, `randomUuid`,
`hexEncode/Decode`, `base64Encode/Decode`, sip: `hashStr`, `hashBytes`,
`hashI64`, `hashCombine` (absorve ns `hash`). O global Web `crypto`
(`getRandomValues`, `randomUUID`, `subtle.digest`) delega aqui.

### `rts:decimal`
Renomeia `bigfloat`: `Decimal.from(x)`, `add/sub/mul/div/neg/sqrt`,
`toString`, `toNumber` — classe registry, sem `free` manual (GC).

### `rts:ffi`
Absorve `ffi` + `ptr` + `mem` + `alloc` + `hint` (superfície unsafe única):
- `open(lib)`, `symbol`, CString/OsString helpers.
- `Ptr`: `readI64/I32/U8/F64`, `write*`, `copy`, `offset`, `null`, `isNull`.
- `alloc/allocZeroed/realloc/dealloc`, `sizeOf`/`alignOf` consts.
- hints: `blackBox`, `spinLoop`, `unreachable`, `assertUnchecked`.
- **NOVO — exports reverse-FFI**:
  ```ts
  import { exportC } from "rts:ffi";
  export const rtsCall = exportC("rts_call_function",
    (a: i32, b: f64): i32 => { ... });
  ```
  Marcador COMPTIME (shape-based, como `getPointer`): força ABI
  monomórfica (i32/i64/f64/bool/ptr+len; `any` = erro de compile), declara
  o símbolo `Linkage::Export` no ObjectModule. Prólogo registra a thread no
  GC (`thread_registry`) e instala error-slot (pânico JS → código de erro,
  nunca unwind cruzando C). Novo alvo `rts compile --lib` → `.dll`/`.so`/
  `.a` + header `.h` gerado das assinaturas. `rts_init()` explícito ou
  init-lazy na primeira chamada.

### `rts:runtime`
`eval`, `evalFile`, `importModule`, `gc.collect`, `gc.liveCount`,
**NOVO** `memoryUsage()`, hot-reload. (Dev/avançado; `trace_*` vira
interno do crash-handler, fora do d.ts.)

### `rts:test`
Framework atual (describe/test/expect via prelude) + `test_core` interno.

### `rts:build` (**NOVO — comptime**)
`includeBytes`/`includeText` (aliases dos de rts:fs), `env(name)` (env de
BUILD, constante), `target.os/arch/family` (consts folded), `buildId()`,
`version()`, `compileTimestamp()`. Todos resolvidos no front-end; nunca
existem em runtime.

### `rts:simd` (**NOVO — fase posterior**)
Vetores Cranelift expostos: `f64x2`, `i32x4`, `f32x4` + lanes/shuffle/
fma. Depende de design de tipos no HIR; gate atrás do doc próprio.

### `rts:compress` (**NOVO**)
`gzip`/`gunzip`/`deflate`/`inflate` sobre `Uint8Array` (impl já existe
interna nos CompressionStreams — só expor).

### `rts:tty` (**NOVO**)
`isTTY(fd)`, `size(): {cols, rows}`, `setRawMode(bool)`, detecção de cor —
essencial pro público CLI do AOT.

### Domínio (fora deste doc, plano próprio congelado)
`rts:dom`, `rts:render`, `rts:input`, `rts:egui`, `rts:audio` — seguem
`docs/specs/html-engine/*`. Só herdam as convenções (camelCase já ok).

---

## O que MORRE da superfície pública (vira símbolo interno)

| Namespace | Substituto | Nota |
|---|---|---|
| `promise` | global `Promise` | externs viram plumbing do motor |
| `collections` | `Map`/`Set`/`Array` | vec_*/map_* = representação interna |
| `json` | global `JSON` | |
| `date` | global `Date` | |
| `math`, `num`, `fmt`, `util` | `Math`/`Number` globals | checked/wrapping/bits → `rts:ffi` (numéricos de máquina) |
| `string` | `String.prototype` | |
| `regex` | `RegExp` | |
| `events` | `EventEmitter` global / `node:events` | |
| `timers`, `text_encoding`, `performance`, `fetch`, `url`, `console`, `JSON5`, `globalThis` | já são globais | registro muda de "ns" p/ global |
| `buffer` | `Uint8Array`/`ArrayBuffer`/`DataView` | decisão: TypedArrays são A representação de bytes |
| `gc` | `rts:runtime.gc` | |
| `trace` | interno (crash handler) | |
| `env` | `rts:process.env` | |
| `tls` | `rts:net` | |
| `sync`, `atomic`, `parallel` | `rts:thread` | |
| `hash` | `rts:crypto` | |
| `alloc`, `mem`, `ptr`, `ffi`, `hint` | `rts:ffi` | |
| `test_core` | interno de `rts:test` | |
| `engine` | interno (bridges de prelude) | JAMAIS público |
| `bigfloat` | `rts:decimal` | |
| `asio_audio` | `rts:audio` (feature-gated) | |

---

## Realocação de primitivos (rts-primitives)

Regra (doutrina existente): **impl de primordial mora em `rts-primitives`**
(puro Rust, wasm-safe, sem tokio/io). Vazamentos atuais a corrigir:

1. `rts-std/src/collector/string_pool.rs` — typeof/toString/inspeção de
   PRIMITIVOS (String/Number/Boolean) misturados com pool/GC. Separar: a
   lógica de valor (coerções, formatação numérica, string ops `str_*` do ns
   `engine`) → `rts-primitives`; o pool/handles fica no collector.
2. `rts-shared/src/globals/symbol` → `rts-primitives` (Symbol é PRIMORDIAL
   desde 2026-06-26; CLAUDE.md já manda; mover o crate de lugar).
3. `rts-shared` BigInt/Proxy/Reflect (primordiais desde 2026-07-03) →
   `rts-primitives`.
4. Externs de String/Boolean/Number em `rts-primitives` que declaram
   `__RTS_FN_NS_GC_STRING_NEW` com assinaturas divergentes (warnings
   `clashing_extern_declarations` atuais) → UMA declaração canônica em
   `gc_surface.rs` de cada crate, `pub use` interno.
5. Meta: `String`/`Boolean`/`Number`/`Array` primordiais rodando 100% em
   Rust dentro de `rts-primitives`, sem depender de nada acima de
   `rts-engine`.

---

## Fases de execução (cada uma = PR verde + suíte)

- **F0 — Gerador/Registry**: builder ganha `module("rts:fs")` + flag
  `internal` (fora do d.ts); gerador de `rts.d.ts` emite `declare module
  "rts:fs"` por módulo; resolver aceita `rts:<ns>`.
- **F1 — Esconder internos**: marca `engine/trace/test_core/gc/collections/
  promise/json/date/math/num/fmt/util/string/regex/events` como internal;
  globais registrados como globais. Nada renomeia ainda; suíte inteira
  deve continuar verde (ela usa a superfície velha só onde é pública).
- **F2 — camelCase + módulos novos**: renomeia membros (mapa acima),
  reagrupa (`tls`→net, `sync/atomic/parallel`→thread, `hash`→crypto,
  `ffi+ptr+mem+alloc+hint`→ffi, `env`→process, `bigfloat`→decimal);
  **cutover duro**: testes/fixtures/exemplos reescritos no mesmo PR (por
  módulo, PRs pequenos: F2a fs/io, F2b net/http, F2c thread, F2d ffi, …).
- **F3 — buffer→TypedArrays**: APIs de bytes migram p/ Uint8Array;
  namespace buffer morre.
- **F4 — Primitivos**: realocação rts-primitives (mapa acima).
- **F5 — Comptime**: `rts:build` + `includeBytes/Text` + `env!` no front.
- **F6 — Novos sistemas**: watch/mmap/locks, sinais, tty, compress,
  unix sockets, channels, memoryUsage. Cada um issue própria.
- **F7 — exportC + `rts compile --lib`** (doc de detalhe se necessário).
- **F8 — simd** (doc próprio antes).

Piso de honestidade vale em toda fase: paridade real, build verde, nenhuma
regressão silenciosa; fixtures cross-runtime que usam superfície antiga são
ATUALIZADOS (mudança intencional, documentada por PR).
