---
name: add-entry-point
description: Add a runtime entry point to the new engine — an operation compiled code performs by calling rather than by emitting. Use when a lowering needs something the machine cannot express as instructions (it touches the heap, the OS, or global mutable state), when a `RuntimeOp` or `CoreEntry` is missing, or when the host refuses a call with a symbol or shape disagreement.
---

# Adding an entry point

There are **two kinds** and they are wired differently. Decide first.

| kind | who states it | resolved in | example |
|---|---|---|---|
| `RuntimeOp` — the **language** calls it | `rts-codegen` | `entries::resolve` | `__rts_add`, `__rts_get_property` |
| `RtEntry` — the **machine** emits it | `rts-cranelift` | `entries::machine_entry` | alloc, cache resolve, write barrier, throw |

Membership rule, same for both: *an entry point exists if and only if the
operation touches the heap, the operating system, or global mutable state. Pure
computation is instructions.* `to_int32` is not one. `add` is, because two
strings joined allocates.

Run `reuse-check` first.

---

## A. `RuntimeOp` — the language calls it

Six touch points, in this order. Skipping any of the first four is a build error;
skipping the fifth ships a dead entry.

**1. `crates/rts-codegen/src/runtime/mod.rs`**
- a variant on `enum RuntimeOp`, with a doc comment saying why it is a call and
  not an instruction
- append to `RuntimeOp::ALL`
- an arm in `symbol()` → `"__rts_<snake_name>"`, no scope prefix
- an arm in `signature()` → `UNPROVEN` where nothing is proved, `Repr::Bool` /
  `Repr::I64` where the runtime establishes something

**2. `crates/rts-core-rwk/src/entry/<module>.rs`** — the implementation:

```rust
/// What it guarantees, and what was rejected.
#[rtse::entry]
pub fn my_op(target: u64, key: i64) -> u64 {
    with_current(|context| { … })
}
```

- no attribute argument: the symbol derives as `__rts_my_op`, and the descriptor
  const as `MY_OP_ENTRY`. Only pass a name for a spelling that predates the
  convention.
- `u64` is a **tagged value**, not an integer. A genuine integer is `i64`.
- a string parameter is `&[u16]` — free, and what a JavaScript string is. `&str`
  re-encodes on every call and aborts on a lone surrogate.
- if it can call back into user code, **drop the borrow before calling**:
  collect inside `with_current`, drop, call, re-borrow to store. A second borrow
  in an `extern "C"` frame aborts the process — it cannot unwind.

**3. `crates/rts-core-rwk/src/entry/mod.rs`** — `pub use <module>::my_op;`

**4. `crates/rts-core-rwk/src/entry/table.rs`**
- import `MY_OP_ENTRY`
- a `CoreEntry` variant with its number **written out**, appended
- append to `CoreEntry::ALL`, an arm in `describe()` → `MY_OP_ENTRY`
- raise `CORE_ENTRY_COUNT`. Removing an entry leaves its number as a gap; never
  renumber.

**5. `crates/rts-host-rwk/src/entries.rs`** — an arm in `resolve()`:

```rust
RuntimeOp::MyOp => (CoreEntry::MyOp, {
    rts_core_rwk::entry::my_op as extern "C" fn(u64, i64) -> u64 as *const u8
}),
```

The cast **is** the shape check. Write it out.

**6. Emit the call** in `crates/rts-codegen/src/emit/`, then a test in
`crates/rts-host-rwk/tests/running.rs` that **runs** a program reaching it.

---

## B. `RtEntry` — the machine emits it

The set is closed and small. Adding one is a change to
`crates/rts-cranelift/src/symbols/mod.rs` (`RtEntry`, `ALL`, `COUNT`) plus an arm
in `machine_entry` in `crates/rts-host-rwk/src/entries.rs`. A missing arm there is
a crash inside compiled code, not a refusal — so it lands in the same change.

---

## Verify

```bash
cargo check -p rts-codegen -p rts-core-rwk -p rts-host-rwk
cargo test -p rts-host-rwk entries          # the agreement tests
cargo test -p rts-core-rwk entry            # numbering and uniqueness
cargo test -p rts-host-rwk running          # a program that runs it
```

`entries.rs` tests fail loudly on a symbol skew and on a shape skew — do not
change what they assert to make them pass.

## Never

- hand-write an ABI signature row: it is derived from the Rust signature
- renumber `CoreEntry`
- add a `RuntimeOp` without the runtime side in the same change: the compiler
  would name an operation nothing implements
