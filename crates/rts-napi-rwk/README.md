# rts-napi-rwk — N-API, on this engine

**Read this file in full before changing anything in this crate.**

A native addon compiled for Node — a `.node` file — resolves a fixed set of
`napi_*` symbols out of the host process and calls them. This crate is those
symbols, implemented over `rts-core`.

## Why it is a rewrite and not a port

`crates/rts-napi` implements the same ABI over the deleted engine, and it is
kept beside this one to be READ. It is not the starting point of a diff:

- Its handle model is the old runtime's. Every one of its 6422 lines reaches
  `Entry`/`alloc_entry`/`with_entry` — a tagged-enum heap. This engine has
  NaN-boxed words in a region, shapes, and a conservative collector. The two are
  different answers to the same question, so a line-by-line translation would
  carry the old answer's shape into a place where it is wrong.
- Its documentation is in Portuguese and its files are grouped by nothing in
  particular (`phase2.rs`, `phase2b.rs`, `phase2c.rs`, `phase2d.rs`). This
  repository's convention is English in code and files named for what they hold.

When this crate answers what that one answered, that one is deleted and the
suffix here comes off. That is the whole plan; `PLAN.md` has the phases.

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

## What is absent now

`napi_define_class` and the property-descriptor surface, `napi_new_instance`,
BigInt and Date conversions, buffers and typed arrays, `napi_throw` and the
error surface, and the module registration a real `.node` enters through (P8).

None of them is waiting on the engine. They are volume, and `PLAN.md` says
which phase each belongs to.
