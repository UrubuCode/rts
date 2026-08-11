# rts-napi — N-API, on this engine

**Read this file in full before changing anything in this crate.**

A native addon compiled for Node — a `.node` file — resolves a fixed set of
`napi_*` symbols out of the host process and calls them. This crate is those
symbols, implemented over `rts-core`.

## Why it was a rewrite and not a port

There were two crates under this name for a while — this one carried an `-rwk`
suffix — because a version of it existed over the DELETED engine and cargo will
not have two crates of one name. That version was deleted on 2026-08-10 and the
suffix came off with it. The reasoning is kept because it is the standard the
next rewrite in this repository will be held to:

- Its handle model was the old runtime's. Every one of its 6422 lines reached
  `Entry`/`alloc_entry`/`with_entry` — a tagged-enum heap. This engine has
  NaN-boxed words in a region, shapes, and a conservative collector. The two are
  different answers to the same question, so a line-by-line translation would
  have carried the old answer's shape into a place where it is wrong.
- Its documentation was in Portuguese and its files were grouped by nothing in
  particular (`phase2.rs`, `phase2b.rs`, `phase2c.rs`, `phase2d.rs`). This
  repository's convention is English in code and files named for what they hold.

**What ended it was a measurement, not a feeling of completeness**: the old
crate exported 145 distinct `napi_*` names and this one exports 146, with an
empty diff in the direction that mattered. A phase is finished when the old
code is gone.

That is a claim about NAMES. Eight of the 146 answer a status rather than doing
the work, each saying why where it is defined, and `PLAN.md` lists them.

## The rules

### 1. The ABI is not ours to design

Every `#[repr(C)]` layout, every enum discriminant, every symbol name here is
dictated by `js_native_api_types.h`. A compiled addon knows them by heart, and
"improving" one produces a crash inside somebody else's binary with no
diagnostic on our side.

This is the one place in the repository where `CLAUDE.md`'s "never hand-write a
symbol name" does not apply, and it is stated there too: the names ARE the
interface, so an attribute deriving them would be deriving the wrong thing.

Snake-case, non-camel identifiers, and `#[allow(non_camel_case_types)]` come
from the same place. Do not rename them to fit Rust convention.

### 2. A `napi_value` is a slot, never a value

The ABI's `napi_value` is pointer-sized and the addon may keep it for as long as
its handle scope lives. A NaN-boxed word cannot be handed over directly: the
collector may move nothing today, but nothing on the addon's side is a root the
collector can see, so a word held over a call is a word that may name a reused
cell.

So `napi_value` points at a SLOT this crate owns, the slot holds the word, and
the slot is registered with `rts_core::entry::external` — which exists because
of this crate and closes exactly this gap (`entry/external.rs` says so). One
indirection, and the addon's handle is as live as it claims to be.

### 3. A failure is a status, never a panic

`napi_status` is how this ABI says no, and an addon checks it. Unwinding across
`extern "C"` into a `.node` is undefined behaviour, so every entry point here
returns a status and no entry point panics — including on an argument the addon
got wrong, which is the common case rather than the exotic one.

A `napi_generic_failure` from something not yet implemented is honest and is not
a stub in `CLAUDE.md`'s sense: the ABI has a way to say "this did not work", and
saying it is different from answering a wrong value.

### 4. Files stop at 500 lines, and are named for what they hold

`values.rs` holds value creation and reading, `objects.rs` properties,
`functions.rs` calls. Not `phase2b.rs`.

### 5. Nothing here decides JavaScript

Same rule the host has. `napi_get_property` asks `rts-core` what a property is;
it does not walk a prototype chain of its own. Where this crate seems to need a
semantic, the semantic belongs one layer down.

## What this table used to say, and what happened to it

Four rows, each naming an engine gap: a pointer beside a cell, somewhere to run
finalizers, a thread that can reach a `Context`, a queue the loop drains. All
four are now in — the first two as capabilities added to `rts-core`
(`entry::foreign`, `entry::finalize`), the last two by reading the ABI properly.

The threading row was the interesting one, because it was **wrong**. It said
threadsafe functions needed "a second thread that can reach a `Context`". They
do not: `napi_call_threadsafe_function` takes no `napi_value` and returns none,
and an `execute` callback may not call `napi_*` at all. The ABI already draws
its line where this engine needs one, so what crosses a thread is the addon's
own pointer and nothing else. `CLAUDE.md`'s `thread` entry — about running
JAVASCRIPT on two threads — is untouched and still true.

## An addon runs

```
 napi uuid.win32-x64-msvc.node
uuid.win32-x64-msvc.node loaded, exporting 1 names:
  v4() -> dd2e1115-ce69-41de-98c8-8056e54fbc41
```

`-rs/uuid-win32-x64-msvc`, off npm, built against Node's headers by
someone who has never heard of this engine. `PLAN.md` P8d has how the last mile
went and what one addon does NOT prove.

## How to load one

```rust
// SAFETY: mapping arbitrary native code, which is what `require` of a `.node`
// has always meant.
let addon = unsafe { rts_napi::loader::open(path) }?;
let env = rts_napi::Env::new().into_raw();
// SAFETY: the environment outlives the addon, which is never unloaded.
let exports = unsafe { addon.exports(env) };
```

Nothing calls this yet: `rts` has no `require("./x.node")`, and wiring one is a
module-resolution change in the host rather than anything here.

## What is absent now

BigInt and Date conversions, symbols, `napi_get_value_string_latin1`/`utf16`,
a typed array's ELEMENT TYPE and offset, the
property ATTRIBUTES
(`writable`/`enumerable`/`configurable`, which are read and ignored, and say so),
and the module registration a real `.node` enters through (P8).

None of them is waiting on the engine. They are volume, and `PLAN.md` says
which phase each belongs to.
