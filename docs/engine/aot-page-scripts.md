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

## `.html` as an entry — no TypeScript to write

`rts compile pagina.html [out]` and `rts run pagina.html` need no `.ts` file
at all — "só mandar a página e ele compilar sozinho". `crates/rts-cli/src/cli/html_entry.rs`
recognises the extension and writes the shell PROGRAM a user would otherwise
write by hand — `scripts/rts_vs_electron/rts/app.ts`'s own loop — rather than
teaching `compile`/`run` a second front end: by the time
`rts_host::object`/`rts_host::compile` see anything, it is one more string of
ordinary TypeScript source, generated rather than typed.

The generated program is the same shape either way — `casca(html,
resourceBase, scriptUrl, title)` parses the page, loads its resources, runs
its `<script>`s, opens an `egui` window titled from the page's own `<title>`
(the file's stem when it has none), and loops `beginFrame`/`render(win,
doc._dom)`/`endFrame` plus `pumpInputEvents`/`pumpEventCallbacks`/
`pumpTimerCallbacks` per frame, exactly as `app.ts` does today by hand.
`casca`'s own leading comment marks the line to replace with a single
`loadDocument(html, url)` call once that function lands from the DOM lot
building it — this shell composes the identical result from
`parseDocument`+`loadResources`+`runScriptsAt` in the meantime, so nothing
about the CALLER changes the day it does.

What differs between the two commands is only where the HTML text comes
from, matching each destination's own constraint:

- **`rts compile`** embeds the page as a JSON-escaped literal
  (`html_entry::for_compile`) — the binary may run on a machine with no copy
  of the source tree, so it never reads the page from disk again. The page's
  OWN `<script>`s are precompiled exactly as if `--html <entry>` had been
  passed on the command line — the entry path is pushed onto the very same
  list `compile::command` already builds for an explicit `--html`, not a
  second mechanism. A relative `<link>`/`<img>` resolves against the HTML
  file's OWN folder as it exists on the machine that ran `compile`, baked in
  at build time (`std::path::absolute`, not `canonicalize` — the latter's
  Windows `\\?\` prefix would break the very same-drive check
  `__resolveUrl` does on the string). Moving the `.exe` to a machine without
  that exact path loses those resources; a `<script src="http…">` was never
  going to travel anyway (see the cut list below).
- **`rts run`** reads the page from disk at run time (`html_entry::for_run`),
  the same way `examples/view.ts` already does — editing the page and
  re-running costs no rebuild. Nothing is precompiled: the JIT binary already
  carries a compiler by default, so `runScriptsAt` reaches it through the
  ordinary `eval` path, not the hash lookup below.

**Why `rts run`'s generated shell is handed to `run_path` on a mirrored file
and not to `run_source` on the text directly.** `run_source` compiles and
runs on a freshly spawned thread (`new_engine::on_a_deep_thread`) — the right
choice for `-e`/`eval`, which never opens a window — but winit panics
building an event loop off the process's main thread, which is the exact
reason `run_path` itself no longer spawns one (see that function's own
comment). `html_entry::write_shell` mirrors the generated program into the
system temp dir, the same way `url_entry::fetch_program` mirrors a URL entry,
so `run_path` runs it on the CALLING thread and a `.html` entry's window opens
exactly like an ordinary `.ts` one's does.

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
