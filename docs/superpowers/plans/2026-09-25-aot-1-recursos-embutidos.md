# AOT-1: every resource of a compiled page travels inside the binary

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `rts compile pagina.html` (and `rts compile app.ts --html pagina.html`) produces an executable that paints the same page on a machine that does not have the page's folder: the `<link rel=stylesheet>` sheets, the `@import`s they pull, the `<script src>` files and the local `<img src>` files are embedded at build time and read from the image at run time, before any disk lookup.

**Why now:** issue #2731, comment of 2026-09-25 (the AOT plan, lot AOT-1): today `html_entry.rs` bakes the HTML in as a literal but resolves `<link>`/`<img>` against the build machine's absolute folder (`html_entry.rs:34-39`); a copied `.exe` loses its CSS. That is a defect, not a choice, and it is named in `--help` and in `docs/engine/aot-page-scripts.md` as such. This lot does not touch layout and runs in parallel with BT-2.

**Written 2026-09-25** from the tree at `ee36cda2a`, after reading every site named below.

---

## The FORM, fixed first

**One loader.** The resource graph of a page is decided by ONE piece of code, the TypeScript loader that already runs at every start: `loadResources(doc, resourceBase)` in `crates/rts-dom/src/dom.ts:1674` — `__expandInlineStyleImports` (`window.ts:565`), `__loadLinkAt` → `__readResource` → `__inlineImports` (recursive `@import`, depth 16, `seen` list), `__loadScriptAt` → `__readResource`, `__loadImageAt` → `dom.setImageFile`. Every local read ends in exactly two bridge natives: `readTextFile(path)` (`crates/rts-dom-bridge/src/recursos.rs:23`) and `setImageFile(doc, node, path)` (`crates/rts-dom-bridge/src/imagem.rs:57`). **The build side does not re-implement any of that in Rust.** It RUNS the same loader once, in a throwaway JIT — the pattern `rts_host::object::html_scripts::window_base` already uses for the window's member list ("the only version of the list that cannot go stale") — with the bridge in RECORDING mode, and keeps what the two natives were asked for.

**The key of a resource is the string the loader hands the bridge.** `__resolveUrl(base, href)` produces an absolute path from `resourceBase` (the build-time absolute path of the HTML, `html_entry::for_compile`); the compiled binary embeds the SAME html and the SAME `resourceBase`, so at run time the loader produces the SAME strings. The table is `path → bytes`, keyed by that exact string (after the same `strip_prefix("file://")` both natives already apply). No normalisation beyond that: a second spelling of one path is a second entry, which is cheap and never wrong.

**Where the table lives: the manifest, then the bridge.** Build: `rts_host::object::manifest::encode` gains a `resources` section (last, after `page_scripts`, so every existing reader test stays byte-compatible up to it): `u32 count`, then per entry a `string` path and `u32` byte length + bytes. Run: `rts-runtime-boot::manifest::read` reads it and `run` hands it to a new `rts_dom_bridge::recursos::tabela::declare(Vec<(String, Vec<u8>)>)` before the entry runs — the dependency already points that way (`rts-runtime-boot` depends on `rts-dom-bridge`; the bridge depends on nothing above `rts-core`/`rts-dom`). The two natives ask `tabela::lookup(path)` first and fall back to `std::fs` on a miss — so a JIT run, and an AOT binary built before this lot, behave exactly as today.

**Recording is the same table's other side.** `tabela::record_into(Vec<..>)` puts the bridge in recording mode for the current thread; the natives push `(path, bytes)` after every successful disk read. `rts compile` (in `rts_host::object`, next to `html_scripts`) runs the throwaway JIT with recording on and takes the vector out. One table type, two modes, one module, one test that records a page and reads it back.

**http(s) stays out**, exactly as today (`__fetchText` answers `""`); an `<img src=data:>` is already inline and needs nothing. **Fonts** are not loaded by anything today (no `@font-face` loading in the engine) — nothing to embed, and this plan says so instead of pretending.

What this plan does NOT do: AOT-2 (`ua.css` pre-parsed), AOT-3 (binary snapshot of the DOM), a second resolver in Rust, a change to how a JIT `rts run pagina.html` reads files.

---

## The state this plan starts from (read, 2026-09-25)

- `crates/rts-cli/src/cli/html_entry.rs` (163 lines): `for_compile` embeds the HTML as a JSON literal and `resource_base = absolute(entry)`; `CASCA_FN` calls `loadDocumentFrom(html, scriptUrl, resourceBase)`. `crates/rts-cli/src/cli/compile.rs:104-161`: a `.html` entry is pushed onto the `--html` list; `rts_host::object::html_scripts::extract_files` extracts the `<script>`s; `compile_to_object_with_html` / `compile_graph_to_object_with_html` build the object; `embed_manifest` places `manifest::encode(program)` bytes under `__rts_manifest`.
- `crates/rts-host/src/object/manifest.rs`: hand-rolled little-endian format, documented in its header in the exact order of sections; the reader is `crates/rts-runtime-boot/src/manifest.rs::read` (`Manifest` struct with `resolutions`, `page_scripts`, …), "kept honest by a test that writes a manifest here and reads it back with the reader's own rules". `rts-runtime-boot/src/lib.rs::run` reads the embedded manifest (`:315`), then the `.rtsdata` sidecar (`:333`), then declares resolver (`:458`) and page scripts (`:501`).
- `crates/rts-host/src/object/html_scripts.rs::window_base` (`:167`): the throwaway JIT — a `BOOTSTRAP` TS source run in-process through the engine's own compiler, answering a string. Read its whole body and its "NOT `crate::run::compile`" note before writing the recorder's run: it explains which entry point does not prepend the DOM facade and why the answer must be copied out as a string while the heap exists.
- `crates/rts-dom-bridge/src/recursos.rs` (`readTextFile`, 1 native, `MEMBERS` table) and `imagem.rs:57` (`setImageFile`): both do `std::fs` directly. `rts-dom-bridge` depends on `rts-core` and `rts-dom` only.
- Tests of the manifest: `crates/rts-host/src/object/manifest.rs::tests` (round trip), `crates/rts-host/tests/aot_manifest_embedded.rs`. AOT fixtures: `tests/aot/claude-pagina-*.{html,ts}`; how they are exercised in CI is in `.github/workflows/build-artifacts.yml` (grep `tests/aot`) — read it before adding one.

---

## Rulers

- **A fixture that FAILS today, written first** (`PLAN.md` §1, "a régua antes do código"): `tests/aot/claude-pagina-recursos.html` with a `<link rel=stylesheet href="claude-pagina-recursos/estilo.css">` whose sheet `@import`s a second file, a `<script src="claude-pagina-recursos/probe.js">` and a local `<img src="claude-pagina-recursos/px.png">` (any small PNG the repo already has, copied); the sheet sets an id'd element's `width` to a value the UA sheet never would. A `.ts` driver beside it (the shape of `tests/aot/claude-pagina-eval.ts`) loads the page HEADLESS through `loadDocumentFrom(html, url, base)`, and prints the element's computed width, the number of stylesheets, whether `probe.js` ran (it sets a global) and the image's natural size. The test compiles it (`rts compile driver.ts --html page.html`), runs the `.exe` **from another working directory with the fixture folder renamed away**, and asserts the output equals the run WITH the folder present. Today the two differ; after this lot they are equal.
- **Unit:** manifest round trip with a `resources` section, read back by `rts-runtime-boot`'s reader (extend the existing cross-crate test); the bridge table's `lookup` before `fs`, and recording, pinned in `rts-dom-bridge`.
- **Nothing else moves:** `cargo test -p rts-host --no-fail-fast` and `-p rts-dom-bridge`, same counts; `tests/aot/claude-pagina-eval` and `claude-pagina-com-script` still pass; the CSS corpus (`examples/claude-css-runner.ts`) and the paint parity dump are unaffected by construction (JIT path unchanged) — the coordinator runs them anyway.

---

## Tasks

- [ ] **Task 1 — the fixture, red.** Write `tests/aot/claude-pagina-recursos.{html,ts}` and the folder; add it wherever `.github/workflows/build-artifacts.yml` exercises `tests/aot`. Run the driver under JIT (`rts run`) to check the probe itself prints the four values. Do NOT make it pass yet.
- [ ] **Task 2 — the table in the bridge.** New `crates/rts-dom-bridge/src/recursos/tabela.rs` (move `recursos.rs` to `recursos/mod.rs`): a thread-local `Option<Vec<(String, Vec<u8>)>>` for lookup, a thread-local recorder; `declare`, `lookup(&str) -> Option<&[u8]>`, `record_into`, `take_recorded`. `read_text_file` and `set_image_file` consult `lookup` first (UTF-8 for text, bytes for the image) and record on a disk hit. Tests: lookup wins over disk; a miss falls back; recording captures exactly the paths asked, in order.
- [ ] **Task 3 — the manifest section.** `manifest.rs` (`rts-host`): `resources: Vec<(String, Vec<u8>)>` on `ObjectProgram` (or wherever the sibling tables live — follow `page_scripts`), encoded LAST; header doc updated with the new line of the format. `rts-runtime-boot/src/manifest.rs::read`: the matching read; `Manifest.resources`. A manifest WITHOUT the section (built before this lot) must still read — the reader treats end-of-bytes at that point as an empty table, and a test pins it. `run`: `rts_dom_bridge::recursos::tabela::declare(manifest.resources)` before the entry.
- [ ] **Task 4 — the recorder at `rts compile`.** In `rts_host::object` (a new `page_resources.rs` beside `html_scripts.rs`): for each `--html` page and the `.html` entry, run a throwaway JIT (the `window_base` pattern) whose bootstrap is `parseDocument(html)` + `loadResources(doc, resourceBase)` with recording on; collect the vector into `ObjectProgram.resources`. `resourceBase` is what `html_entry::for_compile` computes — take it from ONE place (a function both call), never two spellings. `compile.rs`: thread the pages' resource bases through.
- [ ] **Task 5 — the fixture, green**, and the words: `html_entry.rs` module doc "Embedded vs read from disk" and "What compiling pays…", `compile.rs`'s `--help` text and `docs/engine/aot-page-scripts.md` stop saying the copied `.exe` loses its resources; they say what is embedded, what is not (http(s)), and where the table lives.
- [ ] **Task 6 — rulers** (coordinator): `cargo test -p rts-host -p rts-dom-bridge -p rts-runtime-boot --no-fail-fast`, the AOT fixtures, corpus + parity; `PLAN.md` §0 gains an `AOT-1` row (vaga 12, "a página compilada") updated in the same commit.

---

## Constraints

- File ceiling 500 (`html_entry.rs` 163, `recursos.rs` small; `compile.rs` and `manifest.rs` — check their length before adding, split if they would pass).
- No second parser, no second resolver: if a task seems to need Rust code that decides which files a page references, STOP — that is the loader's job, run it.
- `rts-dom` (the crate) stays dependency-free and untouched except for `dom.ts`/`window.ts` if the loader needs a seam (it should not).
- Comments say WHY; English identifiers and comments; tests name the behaviour.
- The agent compiles only `cargo check -p rts-dom-bridge -p rts-runtime-boot -p rts-host` and `cargo test -p rts-dom-bridge --lib` / `-p rts-host <manifest filter>` in its own worktree and target dir; the end-to-end fixture needs the runtime archive and is the coordinator's run.
