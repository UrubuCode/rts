# AOT page scripts

A page's `<script>` bodies are ordinary JavaScript a JIT run compiles at run
time, through `context.eval_compiler_with_receiver` — installed by
`rts-host::live` there, and nowhere in an AOT binary until this batch. `rts
compile --html <file>` (repeatable) closes that: it extracts every `<script>`
from the given HTML, exactly by `rts-dom`'s own tree
(`rts_host::object::html_scripts`, calling `rts_dom::parse_html_to_dom`
directly rather than through the JS bridge), compiles each one at BUILD time,
and installs a run-time hook — `crates/rts-runtime-boot/src/page_scripts.rs`,
shared by both archives since the sequence itself moved there (see
`rts-runtime-boot`'s own module doc) — that finds the right one by the hash
of its exact source.

## The manifest travels inside the image now

Everything this document calls "the manifest" — singletons, kinds, property
keys, literals, templates, the `page_scripts` table above included — used to
reach a running program only as a `.rtsdata` file written beside the `.exe`,
which meant moving the binary without that file broke it: `rts: missing
program data … an AOT binary from rts compile is not standalone of this
file`, measured against a real user on 2026-09-05 when only the `.exe` was
shared. `rts_host::object::embed_manifest` now places the same bytes
[`manifest::encode`] produces as a plain [`rts_cranelift::target::DataBlob`]
inside the object itself, under `MANIFEST_SYMBOL` (`__rts_manifest`) —
alongside the three address tables this document's own "why one object, not
two" section explains, but needing none of THEIR machinery: every byte of the
manifest is known at compile time, so there is nothing for a linker to fill
in, unlike a table whose entries are relocations.

`rts-runtime-boot::run` reads that symbol straight out of the running image
first, and falls back to the `.rtsdata` sidecar — still written by `rts
compile`, and still what `rts_host::object::manifest`'s own tests exercise
directly — only when the image carries none. So a `.rtsdata` file is still
ACCEPTED, for a binary built before this note or moved apart from an image a
future backend cannot embed into, but no compiled program needs one any
more: `rts compile tests/aot/claude-pagina-eval.ts X`, delete `X.rtsdata`,
run `X.exe` — it still prints `3`. `crates/rts-host/src/object/mod.rs` and
`crates/rts-host/src/object/manifest.rs`'s own module docs have the exact
framing (an eight-byte little-endian length ahead of the same bytes the
sidecar carries unframed) and why it lives where it does rather than in
`rts_cranelift::target::DataBlob` itself.

## Why one object, not two

`rts-runtime`'s facade is generic and prebuilt: it reads `__rts_functions`,
`__rts_frames` and `__rts_modules` by those fixed names, so a second table for
page scripts would have to exist unconditionally. Worse, and decisive on its
own: `rts-core`'s README rule 3 forbids a SECOND, unseeded `KeyRegistry` for
the scripts — a property numbered independently by two compilations reads the
wrong slot in one of them. So `rts_host::object::page` appends each script
into the SAME `Ctx` the main program's own `FrontEnd` already has: a fresh
`KeyRegistry`, advanced (via `declare`) past however many keys `front.names`
already handed out.

That is NOT `crate::live`'s own `Seed`-based joining, and using it was this
batch's own bug: `Names::key` answers an already-keyed name from its own map
without touching the registry it is handed, so replaying `Seed` over a
NON-empty `front.names` leaves the fresh registry at `issued: 0`. `Seed` is
right for `live.rs` (always an EMPTY `Names`); wrong for a `front.names` that
already has some. Pinned by
`two_page_scripts_and_the_main_program_share_one_key_numbering`. Literals ARE
re-seeded before every script (`ctx.literal_units`), since
`emit_page_program`'s `finish()` resets `ctx.literals` each call.

## The `enclosing` chain, without running anything

A JIT run learns what a page's `window` carries from
`rts_core::entry::environment_names`, read off the object the PREVIOUS
`<script>` actually wrote to. Nothing runs at `rts compile` time, so this
batch builds the list two other ways: `html_scripts::window_base` measures
`WindowImpl`'s static surface through one throwaway JIT bootstrap, and
`emit_page_program`'s `published` return (a script's own top-level
`var`/`function` names) chains into the next script's `enclosing`.

That bootstrap needs `Scoped::Eval { enclosing: &[], .. }`, not
`crate::run::compile` as written: a DOM prelude comment contains "import "
(Portuguese "import é…"), so `front_end_agreeing`'s text-substring guess
parses every facade program as a MODULE — harmless with no top-level
`return`, fatal here, since a module body is never wrapped in a function and
refuses one outright. `Eval` scoping with nothing enclosing takes the script
door instead, since that guess only fires for `Scoped::Nothing`.

## A name no script's own text ever spells bare

`enclosing`/`published` are STATIC facts about source text, and a UMD bundle
does not read that way: `(function (global) { global.React = {}; })(this)`
never spells `React` bare anywhere in ITS OWN body, so no static scan places
it, and a SIBLING script's bare `React` read had nowhere to resolve.
`emit::binding`'s fallback now tries one more door before refusing: a
reserved chain entry (`emit::page::page_window_name`) recovers the window
value from whatever nesting depth emission is at — `Scope`'s EXISTING hop
bookkeeping, unchanged, is what makes that work at any depth, not a new
mechanism — then asks `RuntimeOp::PageGlobalGet`/`PageGlobalSet`, the WINDOW
itself, rather than `rts-core`'s one process-wide global object
(`GlobalGet`/`GlobalSet`/`UnboundGlobalGet`), for the name AT RUN TIME, when a
sibling may have created it since this one compiled. A miss still raises
`ReferenceError`; a hit answers what a JIT run would measure one execution
later. Proved live: `scripts/rts_vs_electron`'s React 18 bundle mounts and
its counter renders inside a real AOT `.exe` — see
`crates/rts-core/src/entry/page_scope.rs` and `aot_object.rs`'s
`a_name_a_sibling_script_writes_only_as_a_property_of_this_still_compiles`.
**One path serves both destinations, and the JIT does not regress**: the
sentinel is only ever pushed by `emit_page_program`, which `live.rs` (JIT) and
`object/page.rs` (AOT) both call, so the dynamic fallback fires solely for a
name absent from `enclosing` — a name the JIT was already resolving late
against the same window object, not a case its static path used to answer.
Confirmed by the unchanged 158/158 `rts-codegen --lib` and 43/43 `language`
suites.

## The hash

`rts_core::entry::source_hash`, 64-bit FNV-1a — deterministic across
processes because it has no seed to disagree about, unlike `DefaultHasher`'s
per-process SipHash key. One function, called by `rts compile` writing the
manifest's `page_scripts` table and by the installed hook looking a
requested source up — never restated.

## What is cut, named rather than discovered

- **`<script src="http…">` never enters** — fetched by the page loader, never
  by `rts compile`, which touches no network.
- **`eval`, `new Function`, `node:vm`** inside a precompiled script still
  raise (`rts-host/README.md`'s gap list) — only
  `eval_compiler_with_receiver` lands. A global an `eval`'d fragment creates
  dynamically therefore still lands on the PROCESS object, not the window.
- **Tagged templates across page scripts may read the wrong site** — `Seed`
  carries literals, never templates; a `live.rs` gap this inherits, not
  closes.
- **An unknown source raises a `TypeError` naming why** ("not pre-compiled;
  pass `--html`"), not the generic "a fonte não compilou" a JIT syntax error
  gives.
