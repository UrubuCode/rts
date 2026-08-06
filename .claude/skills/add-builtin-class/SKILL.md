---
name: add-builtin-class
description: Add a built-in class, namespace, or prototype method to the new engine's runtime (rts-core-rwk) with `#[rtse::class]` — Error family, Math, Number, JSON, Map/Set, Promise, Date, typed arrays, and anything else a program reaches by name. Use when a JavaScript global or a prototype method is missing at run time, or when a native needs state beside a cell.
---

# Adding a built-in class

A native here is **not a linker symbol**. It is an `extern "C"` function pointer
stored beside a cell, installed as an ordinary property. So `Math.floor = f`
replaces it, and `String.prototype.mine = f` works with nothing special-cased.

Full reasoning: `docs/engine/authoring-natives.md`. Read it once; this is the
procedure.

Run `reuse-check` first.

## 0. Does it belong here?

`rts-core-rwk` is what **every target has, including wasm**. Availability is the
only membership rule.

- **Core:** `Error` family, `Math`, `Number`, `Boolean`, `JSON`, `Symbol`,
  `Function.prototype`, `Map`/`Set`, `WeakMap`/`WeakSet`, `Promise`, `Iterator`
  helpers, `Reflect`, `Proxy`, `Date`, `BigInt`, `ArrayBuffer`/`DataView`/typed
  arrays.
- **Host, not core:** `console`, timers, `fetch`, `URL`, `Headers`, `Blob`,
  `FormData`, `Event`/`EventTarget`, `AbortController`, `TextEncoder`/`Decoder`,
  streams.

Written in Rust, never as a `.ts` prelude: moving a stdlib body from `.ts` to a
native was measured at **124× at run time and 3.4× at compile time**.

## 1. Declare it

```rust
#[rtse::class("Math", namespace)]
impl Math {
    const PI: f64 = std::f64::consts::PI;

    /// `Math.floor(x)`.
    fn floor(x: f64) -> f64 { x.floor() }
}
```

The `impl` block is grouping, not a type — nothing named `Math` is emitted. The
**ident** decides the generated names, so write `impl UriError` to get
`register_uri_error` rather than `u_r_i_error`.

| written | means |
|---|---|
| first parameter named `this` | the receiver, untouched |
| a `u64` parameter | the value as it arrived |
| an `f64` parameter | `ToNumber` of it, once, in the wrapper |
| a `bool` parameter | `ToBoolean` of it |
| `#[stat]` | on the constructor rather than the prototype |
| `#[construct]` | what `new C()` runs |
| `#[js("…")]` | the JavaScript name, when the derived camel-case is wrong |
| `const N: f64` / `const N: &str` | an ordinary property, Rust name unchanged |
| `namespace` | a plain object: no constructor, no prototype |
| `extends = path` | the parent's prototype and statics are linked in |

**The parameter type is a language decision, not a preference.** `Number.isNaN`
takes `u64` because `Number.isNaN("abc")` is `false`; `isNaN` takes `f64` because
`isNaN("abc")` is `true`.

It refuses: a member with no return type, a fifth argument (a call carries four
slots and a receiver — read the argument vector instead), and a type whose
crossing is undecided.

## 2. Register it

The attribute **cannot** register itself — a proc macro cannot see its
neighbours, and a distributed slice would collect in linker order. Call
`register_<name>(context)` from `crates/rts-core-rwk/src/entry/global.rs`.

**Record before installing.** A registration must record itself on
`class_support` before installing anything: installing interns names, interning
allocates, and an allocation can reach back into the same registration. Doing it
afterwards is what made the first `string::prototype_of` recurse until the region
ran out.

## 3. State beside the cell, never a reserved layout

State goes in an `Aside<T>` keyed by region index, declared on `Context` in
`crates/rts-core-rwk/src/entry/mod.rs` and built in **both** `new()` and `over()`
— `over` rebuilds every `Aside` at the region's width, and one missed there
indexes by a number that is not a cell.

A reserved layout was tried for arrays and callables and made `a.tag = 9` a
silent no-op, because a cell with no shape cannot hold a property.

Anything holding **values** in an `Aside` is invisible to the collector. That is
the open question `Function.prototype.bind` is still waiting on — `call` and
`apply` landed because they keep nothing.

## 4. The rule that is a process abort if you forget it

A native that calls user code — `map`, `replace`, `apply`, a getter — **must
release the context borrow before calling**. `with_current` holds a `RefCell`
borrow for the length of its body, and the callee's first act may be to call the
runtime. In an `extern "C"` frame the panic cannot unwind: it aborts the process.

**Collect inside the borrow, drop it, call, re-borrow to store.**

The generated wrapper already does this for arguments (coerce, drop, call). Your
body must do it for anything it calls.

## 5. Verify

```bash
cargo check -p rts-core-rwk
cargo test -p rts-core-rwk <area>
cargo test -p rts-host-rwk running    # a program that uses it
```

Add cases to `crates/rts-host-rwk/tests/suite/<area>.js` — those run the program.

## Never

- fold a built-in into a compile-time lowering. `Math.floor` is a writable
  property of a mutable object; folded code answers the original after a program
  replaces it. Decided, with the test that pins it in `running.rs`.
- return a Rust `bool` declared as a word across the boundary: a `bool` is **one
  byte**, and reading it as a word takes the callee's leftover bits. It made
  `===` answer true for two different strings in release and false in debug.
- return a string: return a handle to an interned string.
