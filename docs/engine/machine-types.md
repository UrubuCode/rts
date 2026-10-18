# Machine types: what a compiled language may declare that JavaScript cannot

RTS compiles to native code, so "how fast can this be" is not bounded by what a
JavaScript engine can infer — it is bounded by what a program can **declare**.
This document is the decision about which declarations exist, what shape each
one takes, and which ones were refused.

It is a design record rather than a guide: `add-builtin-class` is how a surface
gets written, and each module's own documentation is what it does. What is here
is the part neither of those can say — why the set is this set.

## The principle, and the idea it kills

**A machine type is fast because it is not an object.** `new Double(1.5)` is a
box: an allocation, a header, and an indirection on every read. Java's `Integer`
is the canonical demonstration that boxing a typed number makes it slower rather
than faster, and JavaScript already ships the same mistake as `new Number(1.5)`,
which is a pessimisation nobody should write.

So a width is a **type**, a conversion is a **function**, and neither is a class:

| spelling | what it is | run-time cost |
|---|---|---|
| `let x: i32 = 0` | a type, ambient in `rts emit-types` | **none** — a compile-time fact |
| `i32(x)` | a function, folded to one machine instruction | none |
| `new i32(x)` | **throws** `i32 is not a constructor` | — |

The third row is a rule and not a taste, and `crates/rts-std`'s own history is
the argument. `rts:sync` shipped a `mutex_lock` that always succeeded, because
there was nothing to block against, and it was removed under the rule *a surface
that cannot do what its name means does not ship*. A `new Int` answering a boxed
object would be that mistake in a new costume: the name says "a machine integer"
and the value would be a heap cell.

## Why the type names are ambient and the functions are imported

`rts emit-types` already declares its names as **globals, by interface merging**,
and its own header says why. A type costs nothing at run time and cannot collide
at run time, so the width names are ambient: `let x: i32 = 0` needs no import,
and no new runtime global is added to collide with a future edition of the
language or with a program's own name.

A value or an operation is the opposite — a name the program can shadow, observe
and pass around — so it lives behind `rts:*`, which is where everything this
engine adds beyond the language already lives.

## The set

### Ambient widths

`i8 i16 i32 u8 u16 u32 i64 u64 f32 f64`, with conversion functions of the same
names in `rts:num`.

What they buy is not arithmetic — it is the **absence of a guard**. `x | 0` and
`x & 65535` are how a program says "this is an integer" today, and the emitter
*infers* it from that; an inference has failure modes, and every one of them is
a widening back to `f64` with a guard in front of it. A declaration has none.

**Two constraints, stated rather than left to be discovered.**

`i64` and `u64` **cannot cross into a tagged value without loss**. The tag space
has no room for sixty-four bits of payload, which is exactly why `BigInt` is a
separate kind here rather than a wide integer. So an `i64` lives in a local, a
parameter, a return and a struct field; crossing to a JavaScript value is
`Number(x)` (lossy above 2^53) or `BigInt(x)` (allocating), **written by the
program**. An implicit conversion would be a silent wrong answer, which is the
class the honesty floor exists to refuse.

And the narrow reprs — `I8`, `I16`, `F32` — exist in the machine's lattice with
no producer in the language layer, which `the-unwired-keystone.md` attributes to
precise roots. That attribution is right about what it names and has to be read
precisely: what a conservative scan cannot survive is a **machine-typed
derivative of a heap reference** — an unboxed field, a base address, a narrow
element read out of one. A narrow **local holding an integer** is not that: the
scan mis-reads integers as pointers, which retains garbage and never corrupts.
So narrow locals are reachable before precise roots; a hoisted base address is
not, and that line decides how much of `rts:struct` ships now.

### `rts:latin1` — a string of bytes

One cell, the bytes inline, `length` in bytes, index in O(1), no interning, and
`indexOf`/`includes`/`slice`/`split` as `memchr`/`memcmp`/`memcpy`.

Node has `Buffer`, which is a `Uint8Array` with encodings, and **no string that
is bytes**: every `toString()` allocates a real string with the whole plumbing
behind it. Measured 2026-10-01, that plumbing is 48 ns for a five-character
string against 5.9 in C#, while the bytes themselves are free —
`Str::from_latin1(5)` measures 0.00 ns, because they sit inline in
`Narrow::Short`. So the type is not a convenience; it is the plumbing removed.

**And it is the enabling type for the regex path.** The conversion a match pays
today exists because `Str` is Latin-1 or UTF-16 and the `regex` crate wants
UTF-8; over bytes the offsets already *are* the indices. `re.test` is 117 ns
today and the conversion is most of it.

**`String.prototype.toLatin1()` is refused.** A non-standard method on a
standard prototype is observable by feature detection, and the name is one a
future edition of the language may claim. `Latin1.from(s)` reads no worse and
touches nothing that is not ours.

### `rts:struct` — aggregates with no identity

A declared aggregate: no identity, no prototype, no dynamic property, fields of
declared width. An array of them is flat — `n × sizeof`, no cell and no header
per element — which is the one thing C# has that JavaScript cannot express,
because `===` on an object is identity and identity forces a cell.

The machinery exists: `emit/class_layout` already proves a class's fields and
`emit/escape.rs` already proves an instance is never seen. What the declaration
adds is that the proof stops being an inference, so it survives a module
boundary and survives being stored in an array — which is precisely where the
inference gives up today.

Deliverable before precise roots by re-deriving the element's address from the
reference on each access: one load per access, no allocation at all. The fully
hoisted form — one base address held in a register across a loop — is
`Inst::ElementLoad`, and that one waits.

### `rts:stack` — scratch with a lexical lifetime

A buffer valid only in the scope that declares it, which is what a native does
by hand today: `number/format.rs` writes a radix expansion and a fixed-point
expansion into a stack array and answers the string in one copy, and a program
cannot do the same. `escape.rs` is already the analysis that would check the
lifetime.

### `rts:span` — a view whose bound is proven once

Over a typed array, a `Latin1`, or a struct array. What a `TypedArray` does not
give is the compiler knowing the length is invariant across a loop; a span
handed over with a proven length makes `for (let i = 0; i < s.length; i++) s[i]`
a load with no check, which is what `docs/codegen/element-load.md` is about.

## What was refused, and why each refusal is a decision

**`new Double`, `new Int`, `new Long` as constructors.** Boxing, as above. The
conversion functions cover every legitimate use, and the constructor spelling
would advertise a machine type while allocating a cell.

**A storage system beside the typed arrays.** `Int32Array` and its family are
the language's flat storage and are already implemented here. `Span` views them;
it does not replace them. Two ways to hold a machine-width number is the "one
source, generated views" rule broken on purpose.

**SIMD as a surface, for now.** The machine's `Repr` lattice has no vector type,
so a surface needs the machine layer first — and the biggest wins from
vectorising a JavaScript engine are **inside** the runtime rather than in a
program: the ASCII scan in `Str::from_str`, the escape scan in `JSON.stringify`,
`memchr` behind `indexOf`. Those are reachable with Rust's own intrinsics and no
surface at all. A surface is worth revisiting once the internal ones are
measured.

## Raw memory, FFI, and the one limit that is not a preference

This document first listed `rts:ptr` and `rts:mem` among the refusals, on the
grounds that they were "removed by decision". That is wrong about the decision
and the paragraph is gone. They were removed to leave the OLD ABI behind, not
because a compiled language should be unable to name memory — and the goal
stated for this engine is the JavaScript API **and** everything a machine can
do, a TLS stack written here included. The crates in the tree are there to avoid
basic work, not to draw a boundary.

So the question is not whether these come back but in what shape, and there is
exactly one constraint that no policy can relax: **the collector decides what a
word may be.** A raw address stored where the scan can see it is a word the scan
will follow, and a moving collector would later have to rewrite it. That is the
same rule `i64` obeys, for the same reason. Three shapes satisfy it:

- **Memory outside the region.** Not scanned, never moved, with a stated
  lifetime — explicit release or a lexical scope. This is what a byte buffer for
  a socket, a record, or a cipher block wants, and it is what `rts:mem` should
  be: an allocation the collector is told about and does not walk.
- **A handle carrying a length, not a forgeable integer.** `Span` over region
  memory or over outside memory. A length is checkable, and it is what lets the
  runtime stay correct the day the collector moves: the handle is updated, an
  integer could not be.
- **A raw address only where the scan cannot see it** — in a typed array's
  bytes, or in an `i64` local — never as a tagged value. A program that needs
  to hand a pointer to a C function is handing it from one of those.

**FFI.** `rts-napi` already loads a real npm addon and exports 146 `napi_*`
symbols, so calling native code is not a missing capability — it is a capability
with one spelling. A direct `rts:ffi` (a library opened by name, a signature
declared, the call emitted) is the general form, and its real cost is the
boundary rather than the call: a collection must not run while a native holds a
raw pointer into the region, and a callback from C needs the context. Both are
statable — a frame the collector knows about, which is the same machinery a
moving collector needs — and neither is a reason to leave the capability out.

**What a TLS stack actually needs**, since it is the stated example: sockets
(`node:net`, here), raw bytes (`Uint8Array`, here), and then the parts that
decide whether it is fast or merely correct — declared widths so the arithmetic
stays in integer registers, spans so the inner loops carry no repeated bounds
check, a constant-time comparison that the optimiser is forbidden to shorten,
and **intrinsics for AES-NI and the SHA extensions**. The first three are items
in the list above. The last is a machine-layer capability rather than a library,
and it is the honest reason a TLS written against this surface would still want
one addition: a CPU feature named, and an instruction emitted.

## The order, by what was measured

1. **`rts:latin1`**, together with the string-in-one-cell change it shares a
   mechanism with: the same work seen from the library side and from the runtime
   side, and it unlocks the regex path.
2. **The ambient widths**, which add no runtime object and remove guards.
3. **`rts:struct`**, the largest number and the largest job.
4. **`rts:stack`**, then **`rts:span`**, which feeds `ElementLoad`.
5. **Vectorisation inside the runtime**, measured before any surface is drawn.
