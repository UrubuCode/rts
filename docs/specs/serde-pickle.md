# `rts:serde` — deep binary serialization (the RTS pickle)

Status: **shipped** (PRs #2008 phase 1, #2009 phases 2+3; golden format freeze
in `tests/claude-pickle-golden.test.ts`). Wire format **RTSP v1**.

## What it is

Python-pickle-level deep serialization of runtime value graphs to bytes:

```ts
import { serialize, deserialize } from "rts:serde";

const bytes = serialize(anyValue);   // number[] of RTSP bytes
const back  = deserialize(bytes);    // deep copy — cycles + identity intact
```

Cyclic graphs and shared references round-trip through a memo table of
back-references — the property `JSON.stringify` structurally cannot express.
Two fields pointing at one object stay ONE object after a trip.

## Architecture (doctrine placement)

- **One native primitive** in `rts-engine/src/heap/pickle/` (`mod` format +
  ext/fn/class registries, `encode` graph walk, `decode` cursor + revive).
  Consumers: the `serde` namespace (rts-shared, authored with
  `#[rtse::function]`), `node:v8` `serialize`/`deserialize`. The `.ts`
  `structuredClone` is a future consumer (v8.md §5.7).
- **The engine never names a non-primordial class.** Date and RegExp plug in
  through `ExtCodec` (tag + encode/decode fn-ptrs) registered by rts-shared
  (`serde_ns/codecs.rs`) at namespace-register time. Class-instance proto
  attachment goes through a `ClassReviveHook` installed by rts-runtime at
  engine bootstrap (`errslot::install_async_error_hook` →
  `protos::pickle_class_revive`) — the proto tables live above the engine.
- **Class identity**: the generic `name ↔ shape id` registry in
  `rts-engine::heap::shapes` (`register_class_shape`, populated by class synth,
  carried by every prelude snapshot: `PreludeManifest.class_shapes`,
  `prelude_cache`, an appended seed-blob section old blobs read as empty).
- **Function identity**: `module_jit::register_pickle_fns` (post-finalize)
  registers every top-level named fn's uniform-ABI thunk pointer
  (`thunk::thunk_name`, the dynfn contract) in the engine's program-fn
  registry; cleared per program.

## Wire format RTSP v1

```
magic "RTSP" | version u8 = 1 | value stream
```

Varints are LEB128 (zigzag for i32). Strings are varint len + UTF-8.

| Op | Payload |
|----|---------|
| 0–4 | undefined / null / false / true / hole |
| 5 F64 | 8 bytes LE (NaN canonicalized on decode) |
| 6 I32 | zigzag varint |
| 7 STR | len + UTF-8 |
| 8 REF | varint memo id — back-reference (cycles / shared identity) |
| 9 ARRAY | varint len + values |
| 10 OBJECT | varint n + keys block + values |
| 11 BUFFER / 12 ARRAYBUF | varint len + bytes |
| 13 BIGINT | sign u8 + varint word-count + u64 LE words |
| 14 ERROR | name + message + has-cause u8 + [cause value] |
| 15–18 | Boolean box u8 / Number box f64 / String box (inner value) / FloatPrim f64 |
| 19 EXT | tag str + varint len + codec payload (Date: i64 ms; RegExp: src+flags+lastIndex) |
| 20 JSON | serde_json bytes |
| 21 CLASS | class-name + varint n + keys block + values |
| 22 FN_REF | fn-name str |

**Memo discipline** (both sides, the correctness core): a heap value gets its
memo id on FIRST VISIT, BEFORE its children — encoder inserts into the memo
map, decoder allocates a PLACEHOLDER and registers it, then fills children in,
so a back-edge resolves mid-construction.

**Determinism**: encoding a given graph is byte-deterministic (shaped-object
keys are ordered, `Entry::Map` is an IndexMap, memo ids follow visit order).
The golden test asserts byte-identity — an intentional format change must bump
`VERSION`, keep v1 decoding, and regenerate the golden bytes.

## Semantics (the Python parallels)

- **Class instances** (`OP_CLASS`): the WHOLE field state serializes,
  `#`-private fields included (they are the state — Python's `__dict__`).
  Revive allocates WITHOUT running the constructor (pickle semantics), uses the
  DESTINATION program's shape id for the class name (so the baked shape-id
  `instanceof` compare holds), matches fields BY KEY (extra stream fields drop,
  missing stay undefined — schema evolution), and attaches the class proto
  (methods/getters work). A class missing in the destination throws — Python's
  unimportable-class error.
- **Map/Set** ride the generic class path: `#keys`/`#vals`/`#items` are the
  source of truth; the `#h`/`#nx`/`#mask` hash index is VALUE-based (object
  keys hash to bucket 0 and compare by identity), so the verbatim field
  round-trip is correct and object-keyed Maps keep working (memo preserves key
  identity).
- **Functions** (`OP_FN_REF`): BY REFERENCE only — a top-level named fn
  serializes as its name and re-resolves in the destination program (Python's
  `module.qualname`). Arrows, closures and bound fns throw — the line Python
  draws for lambdas.
- **Unserializable** (thrown TypeError, named): sockets, threads, pending
  Promises, generators, Proxies, Symbols, N-API externals, detached/borrowed
  ArrayBuffers — like Python's pickle with a socket.
- **Security**: decode of untrusted bytes never executes user code — no
  constructor runs, `FN_REF` only resolves fns already compiled into the
  program, class revive only re-links already-declared classes. This is
  strictly safer than Python's pickle (whose `__reduce__` is arbitrary code
  execution).

## Encoder/decoder implementation notes (the traps)

- **Snapshot-outside-the-lock**: entry data is cloned under `with_entry`'s
  shard lock and the recursion runs outside it — a nested `with_entry` on a
  same-shard child deadlocks (non-reentrant Mutex).
- **Handle-as-number**: container slots may hold a raw handle as an inline-f64
  integer (e.g. `new Error(...)`, ABI return `Handle`). The encoder applies the
  same liveness discipline as `element_to_handle`: finite whole f64 in
  `[2^48, 2^53)` whose slot is live → heap reference.
- **Null-string sentinel**: the word `STR|SLOT_MASK` (what
  `POLY_FROM_HANDLE(0)` boxes — e.g. an unset Error `stack`) normalizes to
  handle 0 and encodes as undefined.
- **GC pinning on decode**: every allocated handle is pinned for the decode's
  duration (the memo Vec lives on the Rust heap, invisible to the conservative
  stack scanner) and unpinned before returning.
- **Exhaustive `Entry` match**: a new `Entry` variant fails compilation in the
  encoder's `kind_name`, forcing an explicit serialize-or-reject decision.
- Depth ceiling 2000 on both sides (error, not stack overflow).

## Known limitations (v1)

- Functions by reference are **JIT-only** — an AOT binary's fn registry is
  empty (TypeError). Class revive in AOT is wired through the seed blob +
  bootstrap hook but not yet exercised by a test.
- Class names are a **flat namespace** — two classes with the same name in
  different modules collide (last registration wins).
- `serialize` returns `number[]` (each byte an f64 word — 8× heap size while
  in memory). Fine for correctness; a `Uint8Array`/Buffer surface is the
  natural upgrade once the TypedArray surface is the default byte carrier.
- Registry-backed instances (`Entry::Map` + `__rts_class`: Hash, Stats, …) and
  `Entry::Rtse` classes without a codec throw; codecs are one `ExtCodec`
  registration each (Date and RegExp are the templates).

## Candidate next phases

1. **AOT parity**: emit the fn name→thunk table into the AOT main shim; an
   `rts compile` pickle test in the gate.
2. **structuredClone / postMessage unification** (v8.md §5.7, threading T5):
   make the `.ts` clone and future worker messaging consume this primitive.
3. **By-value mode** (the cloudpickle analogue): opt-in embedding of TS SOURCE
   for classes/fns + recompile at decode via `COMPILE_FN_HOOK`. Never the
   default (code-from-bytes is an execution risk the base format must not
   normalize).
4. **Fuzzing**: a `cargo-fuzz` target over `deserialize_value`.
