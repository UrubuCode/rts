# RTS_RWK_IMPLEMENT — authoring natives for the new engine

**What this is.** The direction for how `rts-core-rwk` grows from fifty-odd
built-ins to the full primordial surface, and what the authoring layer
(`rts-macro-rwk`, `rts-symbol-baker`) has to emit for that to be a declaration
rather than a file.

**Where it belongs, said out loud.** `docs/README.md` would split this: the
decisions below into `docs/engine/`, the queue at the end into
`crates/rts-core-rwk/PLAN.md`. It is one file at the root instead because the
work has not started and splitting a direction across two homes before anyone
has followed it is how both go stale. **This file is split or deleted when the
first three classes land through the attribute.** That sentence is the expiry
condition, and it is here because the last generation of root `RTS_*.md`
documents died of not having one.

---

## 0. The correction this exists to record

`CLAUDE.md` used to say `generated/entries.rs` was "read by the new engine". It
is not, and never was: nothing reads it, `TABLE_HASH` and `ENTRY_COUNT` appear
nowhere outside `rts-symbol-baker`, and the baker does not scan `rts-core-rwk`,
`rts-cranelift` or `rts-codegen`. The row was the intent, written where a reader
takes it for the state.

The intent survives; the shape does not, and the reason is the whole of this
document:

> **A native in the new engine is not a linker symbol.**

In the old engine every built-in is one. `#[rtse::abi]` derives a name, the baker
renders 2 047 of them into a table, the JIT resolves by string. The naming
scheme, the contiguous prefix ranges, the `O(log n)` lookup, `TABLE_HASH`, the
AOT force-keep statics — all of that is machinery for *making a name resolve*.

In the new engine a built-in method is an `extern "C"` function pointer stored
beside a cell. It has no name a linker sees. `String.prototype.trim`,
`Array.prototype.map`, `RegExp.prototype.exec` — thirty-two of them exist today
and not one appears in a symbol table, an entry table, or the host's wiring.

So the question "how do we get 2 600 symbols into the new engine" has the wrong
noun in it. Most of those are not symbols here.

---

## 1. What a native actually is

Three facts, and every one of them is already load-bearing in shipped code.

**The convention is the compiled one.**

```rust
type Native = extern "C" fn(env, this, a0, a1, a2, a3) -> u64;
```

A Rust function can have the shape a compiled JavaScript function has, which is
why `entry::functions::call` never learns the difference. The alternative —
teaching `call` about a second kind of callee — puts a branch on every call in
the program to serve the few that are built in.

**Installation is a property write on an ordinary object.**
`native::install(context, prototype_cell, &[(&str, Native)])` makes a callable
per entry and puts it under its name. That is why `String.prototype.mine = f`
works with nothing special-cased: the prototype is an object, and so is the
constructor.

**State lives beside the cell.** A compiled pattern, an accessor pair, an array's
elements — `Aside<T>` keyed by region index. Never a reserved layout: that was
tried for arrays and callables and made `a.tag = 9` a silent no-op, because a
cell with no shape cannot hold a property.

### The rule that is a hang if you forget it

A native that calls user code — `map`, `replace`, a getter, a setter — **must
release the context borrow before calling**. `with_current` holds a `RefCell`
borrow for the length of its body, and the callee's first act may be to call the
runtime. Collect inside a borrow, drop it, call, re-borrow to store.

Getting this wrong is not a wrong answer. It is a deadlock, and it reproduces
only when the callback happens to touch the runtime. It is the single most
valuable thing the attribute can take away from a human.

---

## 2. What the attribute must emit

`rts-macro-rwk` today expresses one thing: a free function with scalar parameters
and at most one return, `self` refused. That is not a limit to design around — it
is what has been written so far. The macro is ours; if it must emit a member
table instead of a symbol, it emits a member table.

For `#[rtse::class("Error")]` over a Rust `impl`, the expansion is:

1. **A wrapper per member.** Unpacks `this`, coerces each argument from the Rust
   signature, packs the return. This is the boilerplate every method in
   `entry/string/basic.rs` and `entry/array_proto/` writes by hand today, and it
   is identical every time.
2. **`NATIVES`** for the prototype and **`STATICS`** for the constructor, in the
   shape `native::install` already takes.
3. **`register(context) -> u64`** — makes the prototype, makes the constructor,
   links `prototype`, installs both. `regex::constructor` and
   `string::constructor` are the two worked examples.
4. **Collector visibility** for whatever `Aside<T>` the class keeps. This is
   `#[rtse::trace]`'s job translated: today `Regexp` and the accessor pairs are
   beside cells and the collector cannot see them. That is a live gap, not a
   future one.
5. **The two-stage shape**, from a marker on members that call back. See §1.

Numbers 4 and 5 are the ones worth the machinery. 1–3 are volume.

### What the attribute cannot do, by construction

A proc macro sees one item. It cannot know its neighbours, and no amount of
cleverness changes that — a distributed slice (`linkme`, `inventory`) collects at
link time in an order the linker picks, which is neither deterministic across
platforms nor reviewable in a diff. That is the baker's half, and it is why the
pair exists rather than one tool.

---

## 3. What the baker assembles

Three sets are hand-written today and are exactly the "closed set assembled in
one place" that `runtime/mod.rs` says should be addressed by index rather than by
string:

| set | where it is by hand now |
|---|---|
| which global names the runtime can supply | `entry/global.rs`, a `match` on text |
| the numbered entry table | `entry/table.rs`, `CORE_ENTRY_COUNT = 51` |
| the compiler↔runtime agreement | `rts-host-rwk/src/entries.rs`, `resolve()` |

`entry/table.rs` states its own expiry: an explicitly numbered list in source is
the right mechanism at this size *and would not be at several hundred*. It is not
near that yet — twelve entries were added in the session that built classes,
iteration, accessors and the argument vector — because only operations **the
compiler emits** need a number. Methods do not.

So the baker's job for this engine is smaller and different from its job for the
old one: not a table of names to resolve, but the three assemblies above.

---

## 4. What stays hand-written, and why that is a decision

**The `PROVIDED` list in `rts-codegen`.** Which names the global object has is a
fact about **JavaScript** — ECMA-262 §19 enumerates them — and `rts-codegen`
must be able to target *a* runtime rather than *the* one in this workspace. If
the baker generated that list, the compiler would be asking permission from
whichever runtime it happened to be built against, which is the boundary rule 1
of that crate draws.

The asymmetry is deliberate: a name the compiler lists and the runtime lacks
reads as `undefined`, which a program can see. That is the failure mode chosen
over the alternative.

---

## 5. What the ABI still cannot express

Read before designing a member that wants one of these.

- **A string return.** `rts_cranelift::abi::AbiType::Slice` is a parameter shape.
  Returning text means returning a handle to an interned string, which is what
  every native does today and what the attribute should keep doing.
- **More than one return.** `Signature.returns` is a `Vec` and the type permits
  any number, but `max_direct_returns` is 1 on all three targets in
  `abi/target.rs` — rule 12 of that crate: unproven behaviour fails safely, and
  raising it is explicit and per target.
- **Intent above representation.** The old `Handle` / `PolyValue` distinction is
  gone; everything tagged is `Repr::Tagged`. If a class wants its handles typed,
  that is a new capability with a reason, not a restoration.

And one that was just found and fixed, worth carrying as a warning: a `bool`
returned across a foreign boundary is **one byte**, and declaring it a word reads
the callee's leftover bits as part of the value. It made `===` answer true for
two different strings in release and false in debug. The fix is at
`lower::abi_return_type`; anything the attribute generates for a boolean-returning
member inherits it.

---

## 6. Design against what exists

Four classes are built and running:

- `entry/string/` — prototype substituted by the chain walk, methods over UTF-16
  code units, and `pattern.rs` as the worked example of calling user code
- `entry/array_proto/` — the folder split on exactly the borrow seam of §1
- `entry/regex/` — state beside the cell, a constructor with a `prototype`
- `entry/object_global.rs` — statics on a constructor

The attribute should be designed so that **rewriting one of these through it
produces the same installed surface**. That is the acceptance test, and it is
available now rather than after a redesign.

---

## 7. Where a native belongs

`rts-core-rwk` is what **every target has, including wasm**. Availability is the
only membership rule, and it decides this split.

**Core:** `Error` and its subclasses, `Math`, `Number`, `Boolean`, `JSON`,
`Symbol`, `Function.prototype` (`call`/`apply`/`bind`), `Map`/`Set`,
`WeakMap`/`WeakSet`, `Promise`, `Iterator` helpers, `Reflect`, `Proxy`, `Date`,
`BigInt`, `ArrayBuffer`/`DataView`/the typed arrays.

**Host, not core:** `console`, timers, `fetch`, `URL`, `Headers`, `Blob`,
`FormData`, `Event`/`EventTarget`, `AbortController`, `TextEncoder`/`Decoder`,
streams.

### Written in Rust, not in TypeScript

The old engine implements `Error`, `Map`, `Set`, `JSON`, `Iterator`, `Reflect`
and more as `.ts` prelude — `error.ts` is a class, `map_set.ts` is an
open-addressing hash table in TypeScript. Porting them as `.ts` would carry into
the new engine the thing that was already measured as expensive: moving a stdlib
body from `.ts` to a native symbol won **124× at run time and 3.4× at compile
time**. The prelude form is not the target.

---

## 8. The queue

Ordered by what a real program stops on first, not by size.

1. **`Error` + the subclass family.** `throw new Error(...)` is how every program
   raises, and it does not compile today.
2. **`Math`, `Number`, `Boolean`.** `Math` in the old engine is not a class at
   all — it is compile-time lowering over a namespace of 34 callables and 8
   constants. Whether it stays a lowering here is an open question (§9).
3. **`Function.prototype`: `call` / `apply` / `bind`.** `apply` wants the argument
   vector, which now exists.
4. **`JSON`**, **`Symbol`** — including `Symbol.iterator`, and `Symbol.dispose`,
   which does not exist in either engine and is what `using` is waiting for.
5. **`Map` / `Set` / `WeakMap` / `WeakSet`.**
6. **`Promise`.** `schedule/` in the core and `sched/` in the machine are built;
   `async`/`await` in the compiler is the separate half.
7. Everything else in §7's core list.

---

## 9. Open questions, named rather than assumed

- **Does `Math` stay a compile-time lowering?** The old engine folds
  `Math.floor(x)` into an instruction. That is a real win and it is also the
  language layer knowing about a built-in, which is what `emit/globals.rs`
  deliberately avoids. Decide before writing it, not after.
- **Who owns the collector's view of `Aside<T>`?** Every native holding state has
  the same problem and there is one answer to write.
- **Does the attribute emit into `rts-core-rwk` only, or into `rts-host` too?**
  The split in §7 says both, which means the attribute cannot assume the
  installation point.
- **Decorators are not on this list at all.** `@decorator` does not parse in the
  new engine — `decorators: false` in `parse/mod.rs` — and the old engine's
  support is `decoratorExpr(0);` emitted as a discarded statement with the target
  passed as literal zero. There is nothing to port. It is a language feature with
  a tree, field/method/accessor semantics and `addInitializer`, and it belongs in
  `crates/rts-codegen/PLAN.md` rather than here.
