# rts-macro-rwk — declare an entry point, derive its shape

**Read this file in full before changing anything in this crate.**

One attribute. `#[rtse::entry]` turns a plain Rust function into a runtime entry
point: `extern "C"` under a derived linker name, plus a `const` describing its
ABI shape **derived from the Rust signature**.

---

## Why it exists

`rts-macro` does the same derivation and emits an `rts_abi::SymbolDesc`.
`rts-abi` is the interface `rts_cranelift::abi` was built to replace — its own
module says why it was rebuilt rather than extended: *"entirely scalar: no
aggregate, no structure, a return position holding zero or one machine slot, and
a string that cannot be returned at all… It is not a foundation."*

The first attempt added the new attribute to `rts-macro` anyway. The emission
was right and the dependency was not:

```
rts-core-rwk → rts-macro → rts-abi
```

Source that looks independent and a build graph that is not is worse than an
honest coupling, because nothing shows it until the day `rts-abi` is deleted and
the new runtime stops building alongside it.

---

## The rules

### 1. Depend on nothing but `syn`, `quote` and `proc-macro2`

Not `rts-abi` — that is the coupling this crate exists to remove. Not
`rts-cranelift` either: a proc macro emits **tokens**, so this writes
`::rts_cranelift::abi::EntryDesc` into the expansion without knowing the type,
and nothing using the macro drags a compiler backend into its build.

If an addition here seems to need a fourth dependency, the thing it wants is
almost certainly a decision that belongs where the descriptor is *read* rather
than where it is written.

### 2. Derive; never accept a shape as an argument

The point is that the declaration and the definition cannot disagree. An
attribute argument saying "two tagged parameters" would be a second place to say
what the signature already says, which is the drift being removed.

The one thing taken as an argument is the linker name, and only as an escape
hatch for a symbol whose spelling predates the convention.

### 3. Refuse a type whose crossing has not been decided

The list is short and the error names it. A guess produces a call that compiles
and passes the wrong number of registers, found at run time or not at all.

A `&str` is the exception to "one parameter, one argument": it cannot cross
`extern "C"` as itself, so a function taking one keeps its ordinary Rust
signature and gains a trampoline that takes the pointer and the length. A
function taking none is rewritten in place and pays nothing — a trampoline for
every entry would add a call to the common case for no reason.

`u64` is a **tagged value**, not an integer — that is what a `Value` is, and
`Repr::Tagged` is the machine's word for "nothing has been proved about this". An
entry wanting a genuine integer takes `i64`.

### 4. Decide nothing about the set

Which number an entry has, which entries exist, whether two collide: none of it
is visible to a proc macro, because an expansion sees only its own item. This
emits one descriptor. Assembling them is somebody else's job, and pretending
otherwise is how a macro grows a global it cannot have.

### 5. Copy nothing from `rts-macro` without a reason to

Registry members, class declarations, the `__rtsm_`/`__rtsn_` scope convention,
constants exposed as globals — all absent, and their absence is the point. That
convention exists to organise a table of thousands; the new engine's set is small
and closed by a membership rule. A rework that copied the old surface across
would be a rename.

---

## How a crate picks which macro it gets

Cargo renames a dependency, so both are spelled `rtse` at the use site:

```toml
rtse = { package = "rts-macro-rwk", path = "../rts-macro-rwk" }   # new engine
rtse = { package = "rts-macro",     path = "../rts-macro" }       # old engine
```

No feature flag, no conditional compilation, no shared body carrying two shapes.
A crate belongs to one world and says so in one line.

---

## The `-rwk` suffix

Temporary, like the others. It goes when `rts-macro` does, and `rts-macro` goes
when `rts-abi` does.
