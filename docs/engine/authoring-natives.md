# Authoring natives for the new engine

**What this is.** How a built-in class is declared for `rts-core`, what
`#[rtse::class]` derives from that declaration, and which of the surrounding
decisions were made deliberately rather than fallen into.

It was `RTS_RWK_IMPLEMENT.md` at the repository root, which carried the condition
that ends it: *split or deleted when the first three classes land through the
attribute.* Seven landed — `Error` and its family, `Math`, `Number`, `Boolean`,
`Function.prototype` — so the decisions are here and the queue is in
`crates/rts-core/PLAN.md`, which is where a plan lives.

---

## 1. A native is not a linker symbol

In the old engine every built-in is one. `#[rtse::abi]` derives a name, the baker
renders 2 047 of them into a table, the JIT resolves by string. The naming
scheme, the contiguous prefix ranges, the `O(log n)` lookup, `TABLE_HASH`, the
AOT force-keep statics — all of that is machinery for *making a name resolve*.

Here a built-in method is an `extern "C"` function pointer stored beside a cell.
It has no name a linker sees. `String.prototype.trim`, `Array.prototype.map`,
`Math.floor` — none of them appears in a symbol table, an entry table, or the
host's wiring.

So the question "how do we get 2 600 symbols into the new engine" has the wrong
noun in it. Most of those are not symbols here.

`generated/entries.rs` was once described as the new engine's index-addressed
table. Nothing reads it, `TABLE_HASH` and `ENTRY_COUNT` appear nowhere outside
`rts-symbol-baker`, and the baker does not scan `rts-core`,
`rts-cranelift` or `rts-codegen`. That row was the intent, written where a reader
takes it for the state.

---

## 2. What a native actually is

Three facts, every one load-bearing in shipped code.

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
works with nothing special-cased, and why `Math.floor = f` replaces the built-in
exactly as it does in any other engine.

**State lives beside the cell.** A compiled pattern, an accessor pair, an array's
elements — `Aside<T>` keyed by region index. Never a reserved layout: that was
tried for arrays and callables and made `a.tag = 9` a silent no-op, because a
cell with no shape cannot hold a property.

### The rule that is a hang if you forget it

A native that calls user code — `map`, `replace`, `apply`, a getter — **must
release the context borrow before calling**. `with_current` holds a `RefCell`
borrow for the length of its body, and the callee's first act may be to call the
runtime.

Getting this wrong is not a wrong answer. In an `extern "C"` frame the panic
cannot unwind, so it aborts the process — which is how it was found while
building `Number.parseInt`, where reading a radix inside the borrow that then
coerced it took a second one. Collect inside a borrow, drop it, call, re-borrow
to store.

This is the single most valuable thing the attribute takes away from a human, and
it does it structurally: the generated wrapper is **coerce, drop, call**. Each
argument converts through its own short borrow, and the author's body runs with
none held.

---

## 3. What the attribute emits

`#[rtse::class]` is applied to an `impl` block. The block is grouping, not a
type: nothing named `Math` is emitted, and the type ident exists only to name
what is generated — which is why it is the ident and not the JavaScript name that
the prefix comes from. `URIError` snake-cases to `u_r_i_error`, while
`impl UriError` gives the `register_uri_error` a caller wants to write.

```rust
#[rtse::class("Math", namespace)]
impl Math {
    const PI: f64 = std::f64::consts::PI;

    /// `Math.floor(x)`.
    fn floor(x: f64) -> f64 { x.floor() }
}

#[rtse::class("TypeError", extends = register_error)]
impl TypeError {
    const name: &str = "TypeError";

    #[construct]
    fn build(this: u64, message: u64) -> u64 { … }
}
```

| written | means |
|---|---|
| a first parameter named `this` | the receiver, untouched |
| a parameter typed `u64` | the value as it arrived |
| a parameter typed `f64` | `ToNumber` of it, once, in the wrapper |
| a parameter typed `bool` | `ToBoolean` of it |
| `#[stat]` | on the constructor rather than the prototype |
| `#[construct]` | the code `new C()` runs |
| `#[js("…")]` | the JavaScript name, when the derived camel-case is wrong |
| `const N: f64` / `const N: &str` | an ordinary property, the Rust name unchanged |
| `namespace` | a plain object, no constructor, no prototype |
| `extends = path` | the parent's prototype and statics are linked in |

It emits `NAME_NATIVES`, `NAME_STATICS`, `NAME_CONSTANTS`,
`NAME_STATIC_CONSTANTS` and `register_name(context) -> u64`, in the shape
`native::install` already takes.

### What the parameter type states

`Math.abs` takes `f64` and `Number.isNaN` takes `u64`, and the difference is the
language's rather than a preference. `Number.isNaN("abc")` is **false** — it
exists precisely to be the one that does not convert — while `isNaN("abc")` is
true because it does. The type is where that is said.

### What it refuses

A member with no return type. Every JavaScript function answers something, so a
member returning nothing would be one whose answer the attribute chose, and
`undefined` is a value a program branches on rather than an absence.

A fifth argument. A call carries four slots and a receiver; a member wanting more
reads the argument vector.

A parameter or constant type with no decided crossing. Guessing produces a call
that compiles and passes the wrong number of registers.

### What it cannot do, by construction

Register itself. A proc macro sees one item and cannot know its neighbours — a
distributed slice (`linkme`, `inventory`) collects at link time in an order the
linker picks, which is neither deterministic across platforms nor reviewable in a
diff. So `register` is a function, and `entry::global` is where the set is
assembled.

---

## 4. Where a class is found at run time

`class_support` holds a list of `(name, made, prototype)` on the context, and
that is a list rather than a field per class for a reason worth stating: the
expansion has to find *its own* prototype, and a field per class would mean the
attribute could not add one without editing `Context` — the "a proc macro cannot
see its neighbours" limit showing up as a build error instead of a design.

**A registration records itself before installing anything.** Installing interns
names, interning allocates, and an allocation can reach back into the same
registration. Recording afterwards is what made the first `string::prototype_of`
recurse until the region ran out.

`Function.prototype` is reached the way `String.prototype` is: substituted by the
chain walk in `objects::inherited_from` rather than linked from each cell.
`closure_new` runs at every function definition, and writing a link there would
spend a word per function to record a fact all of them share.

---

## 5. What stays hand-written, and why that is a decision

**The `PROVIDED` list in `rts-codegen`.** Which names the global object has is a
fact about **JavaScript** — ECMA-262 §19 enumerates them — and `rts-codegen` must
be able to target *a* runtime rather than *the* one in this workspace. If the
baker generated that list, the compiler would be asking permission from whichever
runtime it happened to be built against.

The asymmetry is deliberate: a name the compiler lists and the runtime lacks
reads as `undefined`, which a program can see.

---

## 6. What the ABI still cannot express

- **A string return.** `AbiType::Slice` is a parameter shape. Returning text
  means returning a handle to an interned string, which is what every native
  does.
- **More than one return.** `max_direct_returns` is 1 on all three targets in
  `abi/target.rs`; raising it is explicit and per target.
- **Intent above representation.** The old `Handle` / `PolyValue` distinction is
  gone; everything tagged is `Repr::Tagged`.

And one worth carrying as a warning: a `bool` returned across a foreign boundary
is **one byte**, and declaring it a word reads the callee's leftover bits as part
of the value. It made `===` answer true for two different strings in release and
false in debug. The fix is at `lower::abi_return_type`.

---

## 7. Where a native belongs

`rts-core` is what **every target has, including wasm**. Availability is the
only membership rule.

**Core:** `Error` and its subclasses, `Math`, `Number`, `Boolean`, `JSON`,
`Symbol`, `Function.prototype`, `Map`/`Set`, `WeakMap`/`WeakSet`, `Promise`,
`Iterator` helpers, `Reflect`, `Proxy`, `Date`, `BigInt`,
`ArrayBuffer`/`DataView`/the typed arrays.

**Host, not core:** `console`, timers, `fetch`, `URL`, `Headers`, `Blob`,
`FormData`, `Event`/`EventTarget`, `AbortController`, `TextEncoder`/`Decoder`,
streams.

### Written in Rust, not in TypeScript

The old engine implements `Error`, `Map`, `Set`, `JSON`, `Iterator` and `Reflect`
as `.ts` prelude. Porting them that way would carry into the new engine the thing
already measured as expensive: moving a stdlib body from `.ts` to a native symbol
won **124× at run time and 3.4× at compile time**. The prelude form is not the
target.

---

## 8. The questions this answered, and the ones still open

### `Math` is an object, not a compile-time lowering — decided

The old engine folds `Math.floor(x)` into an instruction. The deciding argument
is neither "that is faster" nor "the language layer should not know about
built-ins":

> **A lowering is not observably equivalent.** `Math.floor` is a writable
> property of a mutable object, so a program may replace it, pass it, or read it,
> and folded code answers the original for all three.

An optimisation wrong for legal programs has to be *guarded*, and the guard —
proving nothing wrote to `Math` — is a whole-program fact this compiler cannot
establish today. The 124× measurement is also narrower than "lower it": it was
`.ts` body → native symbol, which is exactly what this is. Folding on top stays
available later, as a decision at the call site rather than a different runtime.
`running.rs` pins the property that would break: replacing `Math.floor` changes
what a later call answers.

### Still open

- **Who owns the collector's view of an `Aside<T>`?** Every native holding state
  has the same problem and there is one answer to write. It is what
  `Function.prototype.bind` is waiting for: a bound function remembers a receiver
  and a list of arguments, which is a table of **values** beside a cell, and the
  collector cannot see one. `call` and `apply` keep nothing, which is why they
  landed and `bind` did not.
- **Does the attribute emit into `rts-host` too?** It writes `crate::entry::…`
  paths today, so it works anywhere inside `rts-core` and nowhere else. §7's
  split says both eventually, which means the installation point has to become
  something the declaration states.
- **A property read on a primitive number.** `(5).toFixed(2)` has no cell to walk
  from, so `Number.prototype` methods would be unreachable and are not written.
  The fix belongs in `objects::inherited_from` beside the two substitutions
  already there.
- **Decorators are not on this list at all.** `@decorator` does not parse in the
  new engine, and the old engine's support is `decoratorExpr(0);` emitted as a
  discarded statement. There is nothing to port; it belongs in
  `crates/rts-codegen/PLAN.md`.
