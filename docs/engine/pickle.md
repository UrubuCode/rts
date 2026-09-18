# `rts:serde` — the pickle, RTSP v2

**What this is.** The specification of the byte stream `rts:serde` writes and
reads, what each kind of value becomes, where it stops, and what it costs. The
implementation is `crates/rts-core/src/entry/pickle/` and the walk it shares
with `structuredClone` is `crates/rts-core/src/entry/clone/`; each module's own
documentation says why it is shaped the way it is, and this does not repeat it.

```ts
import { serialize, deserialize, version, upgrade } from "rts:serde";

const bytes = serialize(value);   // Uint8Array
const back  = deserialize(bytes); // Uint8Array | Buffer | ArrayBuffer | number[]
```

`node:v8`'s `serialize`/`deserialize` are the same stream (`crates/rts-node/
src/v8/mod.rs` says where that differs from V8), and so is what `Storage`
persists with `persistTo` — one format, three surfaces.

The rulers: `tests/claude-pickle.test.ts`, `tests/claude-pickle-golden.test.ts`
(the v1 and the v2 goldens), `tests/claude-pickle-v2.test.ts`,
`tests/claude-storage-pickle.test.ts`, `tests/node_v8_full.test.ts`,
`tests/aot/claude-pickle.ts` (diffed JIT against AOT in CI), and the fuzz corpus
in `pickle/fuzz_tests.rs`.

---

## 1. The stream

```
"RTSP" | version u8 = 2 | one value
```

A varint is LEB128 (protobuf's); a signed one is zigzag first. A value is one
opcode byte and the payload the opcode decides.

### Strings are a table

Every string the stream carries — a value, an object key, a class or module
name, an error class — is a **strref**: a varint `k`. `k = 0` is a new string,
followed by its byte length and its bytes, and it appends to the table; `k > 0`
is the table's entry `k - 1`. An array of ten thousand records with the same
five keys writes each key once and then one byte per use.

The bytes are **WTF-8**: UTF-8, except that a lone surrogate — legal in a
JavaScript string, unspellable in UTF-8 — is written as the three bytes its code
point would take. For a string with no lone surrogate it IS UTF-8, which is why
v1's strings read through the same decoder.

### Opcodes

| op | name | payload | memo id |
|---:|---|---|:---:|
| 0–4 | undefined, null, false, true, hole | — | |
| 5 | F64 | 8 bytes LE — a number that is not an int32, `-0` and `NaN` included | |
| 6 | I32 | zigzag varint — a number that is a whole int32 | |
| 7 | STR | strref | |
| 8 | REF | varint memo id | |
| 9 | ARRAY | n, n values, m, m × (strref key, value) — the elements, then named members | ✓ |
| 10 | OBJECT | n, n strref keys, n values | ✓ |
| 11 | BUFFER | varint length + bytes — a Node `Buffer` | ✓ |
| 12 | ARRAYBUF | varint length + bytes — an `ArrayBuffer` | ✓ |
| 13 | BIGINT | sign u8, varint word count, words u64 LE | |
| 14 | ERROR | class, flags u8, [message] [stack] [cause], n × (strref key, value) | ✓ |
| 15 | BOOLBOX | u8 — `new Boolean(b)` | ✓ |
| 16 | NUMBOX | f64 LE — `new Number(n)` | ✓ |
| 17 | STRBOX | value — `new String(s)` | ✓ |
| 19 | EXT | tag (varint length + bytes), payload (varint length + bytes) | ✓ |
| 21 | CLASS | strref module, strref name, varint version, n, n strref keys, n values | ✓ |
| 22 | FN_REF | strref module, strref name | |
| 23 | MAP | n, n × (key value, value value) | ✓ |
| 24 | SET | n, n values | ✓ |
| 25 | VIEW | kind u8, varint byte length + bytes — a typed array | ✓ |
| 26 | BARE | OBJECT's payload — an `Object.create(null)` object, revived with no prototype | ✓ |

ERROR's class is `0` + strref (a standard class: `Error`, `TypeError`, …,
`AggregateError`) or `1` + a CLASS header's three fields (a class the program
declared). Its flags say which of message (1), stack (2) and cause (4) follow.

EXT carries the two kinds v1 gave a codec, with v1's payloads, so one reader
serves both versions: `Date` — the time value as i64 LE, `i64::MIN` for an
invalid date — and `RegExp` — u32 source length + source, u32 flags length +
flags, i64 `lastIndex`.

VIEW's kind byte: Int8 0, Uint8 1, Uint8Clamped 2, Int16 3, Uint16 4, Int32 5,
Uint32 6, Float32 7, Float64 8, BigInt64 9, BigUint64 10.

### Memo discipline

A container gets its memo id when its opcode is written — BEFORE its children —
and the reader reserves its node at the same moment; a back-reference to it from
inside itself therefore names something that exists. Ids are handed out in the
order the writer reaches containers, which is the order the reader meets them.
Strings, numbers, bigints and function references take no id: none has an
identity a program can observe apart from its value.

### Determinism

The same graph writes the same bytes: members in the engine's enumeration
order, memo ids and table indices in visit order, and nothing else chosen. The
v2 golden test pins it, and the JIT-against-AOT step in CI pins that both builds
write the same stream.

### Depth

Neither side recurses — the walk, the writer and the reader each keep their own
stack — so nesting costs heap and not Rust stack. `MAX_DEPTH` (100 000) bounds
the memory a stream can make the reader commit to open containers, and past it
both sides refuse by name. A linked list of that many nodes pickles.

---

## 2. What each kind becomes

| written | read back as |
|---|---|
| a plain object | a plain object with `Object.prototype` |
| `Object.create(null)` | an object with no prototype (BARE) — a dictionary comes back a dictionary, where `structuredClone` gives it `Object.prototype` |
| an array | an array; holes stay holes, named members stay |
| a number | the same number; `-0` and `NaN` survive |
| a string | the same string, lone surrogates included |
| a bigint | the same bigint |
| `Date`, `RegExp` | the same class; a pattern keeps its `lastIndex` |
| an error | its class (standard or declared), `message`, `stack`, `cause`, and every own enumerable member |
| `Map`, `Set` | the same, keys and members in order; an object key keeps its identity |
| `ArrayBuffer`, typed array, `Buffer` | the same kind over a copy of the bytes |
| `new Number/String/Boolean` | the same wrapper |
| a class instance | an instance of the class the READING program declares under that name |
| a top-level function | that function, in the reading program |
| a cycle, a shared reference | the same shape |

An own **accessor** on a plain object is read through its getter while writing,
as `structuredClone` reads one; what is written is the value.

A **symbol-keyed** property is not written — a symbol has no spelling a stream
can carry — and nothing refuses it: the rule `JSON.stringify` has.

A typed array is written with its own bytes and revives over a private buffer:
two views of one `ArrayBuffer` come back as two buffers. `structuredClone` has
the same limit, for the same reason.

### Refused, by name

A `TypeError` whose message names the kind: a symbol, a proxy, a closure or
arrow or method, a bound function, a `DataView`, a `WeakMap`/`WeakSet`/
`WeakRef`, a subclass of `Map` or `Set`, a class instance whose own members
include an accessor, and any object whose prototype is neither
`Object.prototype`, `null`, nor a declared class — which is how a `Promise`, a
generator, a `URL` or a host object is refused without a list of them.

---

## 3. Class instances

The whole state is written: public fields and `#private` ones, because they ARE
the state. Revival makes an object with the destination class's prototype and
the stream's fields, and **runs no constructor** — methods, getters and
`instanceof` hold, inheritance and cycles through instances included. A class
the reading program does not declare is a `TypeError` naming it.

Fields are matched **by key**, and a field the destination class no longer has
is kept on the instance rather than dropped — the old engine dropped them by
consulting a fixed layout, and a class here has no layout to consult without
running its constructor. `upgrade` (§5) is where a class that wants to drop one
says so.

### The name: module and class

A class is written as the module that declared it and its name. The **module
key** is the file's path relative to the program's entry, `/`-separated, `""`
for the entry itself — computed by the compiler from the specifiers the host
resolved, and deliberately not the absolute path, which would make a stream
depend on where the repository was checked out.

Reading: the qualified name first. Failing it — another program, a moved file —
the plain name, **only** when exactly one declaration in the reading program
answers to it; two is a `TypeError` naming both, never a choice made silently.
v1 wrote flat names, and goes through the same rule.

### Private fields

A private name is interned as `@@#<n>#name`, where `<n>` is the order in which
the parser met the class — so a subclass's `#x` and its base's are two fields.
That number is meaningless outside one compilation, so the stream writes a
private field by its **depth in the class chain** instead, `@@#^<d>#name`, and
the reader maps it back through its own numbering (each declaration passes its
number to the runtime). Checked: a stream written by one program reads
correctly in another that declares two more classes above the pickled ones. A
private name whose class is not in the chain the registry knows keeps its
memory spelling, and revives only in the same program.

### The registry

The compiler emits one `SerdeDeclare` per named class and per named top-level
function (`crates/rts-codegen/src/emit/serde_names.rs`). The runtime records it
on the heap: a hidden `@@serdeName` on the constructor — how the writer names a
class in one property read — and a hidden `@@serdeNames` **native `Map`** on
the global object, from `"module\0name"` to the declaration — how the reader
finds one. The qualified lookup is one hash probe; the plain-name fallback
(§3, "the name") scans the map, and is taken only for a stream from another
program.

It is **rooted by construction**: the global object is a root and the map is
traced as every `Map` is, so there is no hand-written root list for it to be
missing from (`docs/engine/lost-roots.md`). It is **bounded by the source**: a
(module, name) declared again replaces its own entry, so a class written in a
loop does not grow it. It adds **nothing to `Context`**. Code compiled by
`eval`, `new Function` or a page script registers nothing.

It is a `Map` and not a plain object because **every program pays for the
registry at startup**, whether or not it imports `rts:serde`, and a plain
object with one property per name made that cost quadratic: each new key was a
new layout whose index the write built from the whole chain. Measured
2026-09-18 in release on a program of N top-level functions and one
`console.log`: 218 ms at N=500, 523 at 1 000, 1 922 at 2 000, **9 317 at
4 000**, against 115/145/191/318 without the pickle. `pickle/names_tests.rs`
pins the mechanism by a count — the shape tree grows by the same number of
layouts for 1 000 declarations as for 4 000.

That was the runtime half, and it was not the larger one. The compiler
registered a script's functions in a pass AFTER the hoist, reading every
hoisted closure back — N SSA values live across N runtime calls, and the
machine's register allocation is quadratic in that. `RTS_TIMING` put 8.4 of
the 8.5 s at N = 4 000 in `machine-compile`, before the program ran a line.
`emit/hoist.rs` now registers each closure where it is made, which is the
shape a class always had, and `crates/rts-host/tests/serde_declare_order.rs`
pins the order. Measured on `fast` binaries, medians of five, base → fixed:
N = 500 142 → 54 ms, 1 000 482 → 87, 2 000 1 979 → 157, **4 000 9 205 → 352**.

---

## 4. Functions by reference

A named function declared at the top level of a module or script is written as
its module and name, and reads back as the function the reading program
declared there — Python's `module.qualname`. Anything else that is callable is
refused: an arrow and a closure because their state is not a name, a method
because it is its class's, a bound function because its binding is not either.

**AOT works**, which v1 did not: the registration is compiled code, so a binary
built with `rts compile` fills the registry exactly as `rts run` does. Checked
both ways — a stream written by one read by the other — and pinned by the
`tests/aot/claude-pickle.ts` step in CI, which diffs the two outputs, bytes
included.

---

## 5. Schema versions

```ts
class Save {
  static [version] = 2;
  static [upgrade](fields, fromVersion) { /* migrate */ return fields; }
}
```

The writer puts the class's `version` (0 when it declares none) in the stream.
The reader compares it with the version the same class declares in the reading
program; when they differ and the class declares `upgrade`, it is called with a
plain object of the stream's fields and the version they were written under,
**before** the instance is revived, and what it answers is what the instance
gets. The class's own private fields appear in that object as `"#name"`; an
ancestor's are carried through untouched. Without `upgrade`, or at equal
versions, nothing is called.

**Decoding still runs no code from the stream.** `upgrade` is code the
destination program declared, on a class it declared, found by the same lookup
that finds the prototype. The stream chooses a declared class and a number; it
cannot supply a function — the line Python's `__reduce__` crosses.

When it runs: every object of the stream exists before the first `upgrade` is
called, so the fields may point anywhere in the graph, including at an instance
whose own `upgrade` has not run yet and which is still empty.

---

## 6. Reading v1

`deserialize` reads version 1 — the old engine's format, which saves such as
CRIPTA's `MetaSave` are written in. The differences it absorbs:

- a string took a memo id, and so did a bigint and a function reference;
- keys and class names were plain length-prefixed text, not table entries;
- a CLASS named `Map` held `#keys`/`#vals` (and a hash index `#h`/`#nx`/`#mask`,
  which indexed the old engine's memory and is ignored): it reads as a real
  `Map`; one named `Set` held `#items`; one named `Error` (or another standard
  error) held `message`/`name`/`stack`/`cause`, and reads as that error;
- ERROR was name, message and an optional cause;
- a private field was `#name`, read as the instance's own class's;
- FLOATPRIM (18) and JSON (20) were old-engine boxes, read as a number and as
  the value the JSON text describes.

The 587-byte v1 golden stays in `tests/claude-pickle-golden.test.ts` as the v1
reader's test, with every assertion it had.

---

## 7. Cost

What was done for each of the five requirements the pickle was built against,
and what was left.

**1. One buffer, one copy.** The writer estimates the stream's length from the
arena — nodes, elements, members, texts and raw bytes — reserves it once, and
writes every opcode into that `Vec<u8>`; the `Uint8Array` is made from it in one
copy (`make_bytes`). Nothing is written through the heap per byte, and there is
no `number[]` anywhere on the writing path. Left: reading copies the input's
bytes once out of the heap (the arena is built while the context is borrowed
mutably, and the input lives in that context), and `ArrayBuffer`/`Buffer`
payloads are copied into the arena and again into their new store.

**2. Borrows.** The shared walk was a recursion that took a borrow per value —
classification, one per key for its text, one per member through `get_indexed`.
It is now a worklist that reads every object whose members are plain data inside
ONE borrow and leaves only for an accessor or a proxy. `serialize` of a graph of
data is two borrows (walk; write and copy), `deserialize` is one (read, build,
answer). The build that `structuredClone` shares went from two borrows per node
to none of its own. Lookups the classification repeated per value — the class
registry is a list searched by name — are made once per walk: `Error`, `Map`,
`Set`, `Buffer`, `Object.prototype`, the `Date` key, each constructor's declared
name and each class chain's private-name numbers. The reader resolves each class
a stream names once, by its table indices, and each resolution is one hash
probe of the registry. Left: `key_list` still allocates a
few small vectors per object, and a class instance's private members are found
by walking its shape.

**3. Strings.** A narrow ASCII string is written as its own bytes with no
intermediate `String`; the rest is encoded unit by unit into the output. Keys go
through the string table found by key NUMBER first, so a repeated key is a hash
of a `u32` and one to two bytes on the wire. Measured on a debug build, for the
size and not the time: 1 000 records of three keys pickle to 10 990 bytes against
37 391 for `JSON.stringify` of the same array. Left: a wide string is encoded
unit by unit.

**4. Allocation on read.** An object's keys are all known before any of its
values, so its layout is reached once — `clone::populate`, which `JSON.parse`
wrote first and which the clone and the pickle now share — rather than one shape
transition per `put`. An array's element vector is built at its final size and
handed over whole. Left: a `Map`'s or `Set`'s table grows by insertion, having no
way to reserve.

**5. `Context`.** Nothing was added to it. The registry is on the heap (§3); the
two symbols live in the shared symbol table that already existed.

What was NOT done: a benchmark. This was built on debug binaries only — the
brief forbids release builds here — so every sentence above is about the work
the code does, not about nanoseconds. The comparison against `JSON.stringify`/
`JSON.parse` and `structuredClone` on one graph is to be measured in release.

---

## 8. Limits, stated

- A class declared inside a loop revives as the LAST evaluation of that
  declaration, whichever one wrote it.
- `v8.serialize` pickles rather than cloning: a class instance stays one and a
  top-level function is written by name, where V8 flattens the first and throws
  for the second.
- Fields a class dropped are kept (§3).
