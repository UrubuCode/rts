# AOT page scripts

A page's `<script>` bodies are ordinary JavaScript a JIT run compiles at run
time, through `context.eval_compiler_with_receiver` — installed by
`rts-host::live` there, and nowhere in an AOT binary until this batch. `rts
compile --html <file>` (repeatable) closes that: it extracts every `<script>`
from the given HTML, exactly by `rts-dom`'s own tree
(`rts_host::object::html_scripts`, calling `rts_dom::parse_html_to_dom`
directly rather than through the JS bridge), compiles each one at BUILD time,
and installs a run-time hook — `crates/rts-runtime/src/aot/page_scripts.rs`
— that finds the right one by the hash of its exact source.

## Why one object, not two

`rts-runtime`'s facade is generic and prebuilt: it reads `__rts_functions`,
`__rts_frames` and `__rts_modules` by those fixed names, so a second,
differently-named table for page scripts would have to exist
unconditionally. Worse, and decisive on its own: `rts-core`'s README rule 3
forbids a SECOND, unseeded `KeyRegistry` for the scripts — a property numbered
independently by two compilations reads the wrong slot in one of them. So
`rts_host::object::page` appends each script into the SAME `Ctx` the main
program's own `FrontEnd` already has: a fresh `KeyRegistry`, advanced (via
`declare`) past however many keys `front.names` already handed out.

That is NOT `crate::live`'s own `Seed`-based joining, and using it was this
batch's own bug: `Names::key` answers an already-keyed name from its own map
without touching the registry it is handed, so replaying `Seed` over a
NON-empty `front.names` leaves the fresh registry at `issued: 0` — the first
new key page-script emission mints then collides with the main program's own.
`Seed` is right for `live.rs`, which always keys against an EMPTY `Names`; it
is wrong for a `front.names` that already has some. Pinned by
`two_page_scripts_and_the_main_program_share_one_key_numbering` in
`aot_object.rs`. Literals ARE re-seeded before every script, through
`ctx.literal_units`, since `emit_page_program`'s `finish()` resets
`ctx.literals` each call.

## The `enclosing` chain, without running anything

A JIT run learns what a page's `window` carries by reading
`rts_core::entry::environment_names` off the object the PREVIOUS `<script>`
actually wrote to. Nothing runs at `rts compile` time, so this batch builds
the same list two other ways: `html_scripts::window_base` measures
`WindowImpl`'s own static surface through one throwaway JIT bootstrap (build a
`window`, walk `Object.getOwnPropertyNames` up its prototype chain — the same
enumeration `environment_names` itself uses), and `emit_page_program`'s
existing `published` return (a script's own top-level `var`/`function` names)
is chained into the next script's `enclosing`. Publishing is unconditional per
ECMA-262 §16.1.7, so this equals what a JIT run would see after script N ran —
except a property a script creates DYNAMICALLY, named below.

That bootstrap cannot use `crate::run::compile` as written: a DOM prelude
comment contains the substring "import " (Portuguese "import é…"), so every
program using the facade is parsed as a MODULE by `front_end_agreeing`'s
text-substring guess — harmless with no top-level `return`, fatal here, since
a module body is never wrapped in a function and refuses one outright.
`Scoped::Eval { enclosing: &[], .. }` sidesteps it: that guess only fires for
`Scoped::Nothing`, so asking for `Eval` scoping with nothing enclosing takes
the script door unconditionally — function-wrapped, completion value included.

**The hash**: `rts_core::entry::source_hash`, 64-bit FNV-1a — deterministic
across processes because it has no seed to disagree about, unlike
`DefaultHasher`'s per-process SipHash key. One function, called by `rts
compile` writing the manifest's `page_scripts` table and by the installed
hook looking a requested source up — never restated.

## What is cut, named rather than discovered

- **`<script src="http…">` never enters** — fetched by the page loader, never
  by `rts compile`, which touches no network.
- **A DYNAMICALLY-created global is invisible to a later script's
  `enclosing`.** `window.foo = bar()` in script 1 resolves for script 2 at JIT
  time (already ran), not at AOT compile time (`enclosing` is `published`, a
  static fact) — script 2 then fails to COMPILE (`UnboundName`), loudly.
- **`eval`, `new Function`, `node:vm`** inside a precompiled script still raise
  (`rts-host/README.md`'s gap list) — only `eval_compiler_with_receiver` lands.
- **Tagged templates across page scripts may read the wrong site** — `Seed`
  carries literals, never templates; a `live.rs` gap this inherits, not closes.
- **An unknown source raises a `TypeError` naming why** ("not pre-compiled;
  pass `--html`"), not the generic "a fonte não compilou" a JIT syntax error
  gives.
