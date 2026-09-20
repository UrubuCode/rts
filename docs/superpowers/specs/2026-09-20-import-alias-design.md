# Import path aliases, from `tsconfig.json`

**Status:** design, awaiting review. No code written.
**Crate:** `rts-host` (`src/graph/`). RULE 0 read: `crates/rts-host/README.md`
(6 rules) and `PLAN.md`. `reuse-check` run — §2 records what it found.

---

## 1. What this adds, in one sentence

A specifier that is neither relative nor host-provided may name a file, when a
`tsconfig.json` says which one.

```ts
import { Scene } from "@/engine/core/scene";   // instead of ../../../engine/core/scene
```

## 2. What already answers part of this — `reuse-check`

Three existing answers must be called rather than re-derived. Each of them is
the whole reason a corresponding piece of new code does NOT appear below.

- **`resolve::extended(base, specifier)`** already turns a base plus a written
  name into a real file: it tries `.ts`, `.js`, `.cjs`, `.mjs`, and `index.*`
  when the name is a directory. An alias target is exactly that question asked
  from a different base. It is called, not copied.
- **`resolve::resolve` and `resolve::plain`** own what a path IS, including
  Windows's verbatim `\\?\` prefix. The header of
  `rts_core::entry::dynamic_module` records what a second copy of this rule
  cost: `createRequire` reproduced it, wrote in its own comment that it had to
  match "exactly", and stopped matching the day the loader started stripping
  that prefix. Nothing below introduces a second place that knows.
- **`Loaded.resolutions` and `Graph.resolutions`** already carry every
  `(referrer, written, resolved)` triple to the AOT destination. An aliased
  specifier is recorded by the same walk in the same table — which is why §6
  has no new AOT mechanism.

What `reuse-check` found NO answer for, and is therefore genuinely new: reading
`tsconfig.json`, and matching a `paths` pattern.

## 3. The single question

`is_relative` stops being the predicate the loader branches on. One function
replaces it:

```rust
/// Whether this specifier names a FILE, and which.
fn resolve_written(from: &Path, specifier: &str) -> Option<PathBuf>
```

Every site that today asks `is_relative` and then resolves asks this instead:
`graph/mod.rs` lines 116, 143, 290, 299, 304, and `resolve_specifier` (the
runtime hook installed at `live.rs:95`). Six callers, one answer — which is the
property the loader's own header says it exists to preserve, and the property a
`paths` map is most likely to break, because a map is easy to consult twice.

`None` keeps today's meaning exactly: not a file, so the text is used as
written and the host provides it by name.

## 4. Precedence

`tsc` orders `paths` → `baseUrl` → `node_modules`. RTS has a fourth category
`tsc` does not: modules the host provides by name. The order:

| # | Written form | Resolves to |
|---|---|---|
| 1 | `./…`, `../…` | a file — unchanged from today |
| 2 | `rts:*`, `node:*` | **always** the host, never a file |
| 3 | matches a `paths` pattern | a file, via `extended()` |
| 4 | any other bare name, with `baseUrl` set | a file **if** `extended()` finds one; otherwise fall through to 5 |
| 5 | anything else | left as written — host by name, or an installed package |

**Row 2 is not negotiable.** `baseUrl` makes every bare name a file candidate,
and a project with `baseUrl: "."` that happens to contain `node/fs.ts` would
otherwise shadow `node:fs`. That failure compiles and lies, which
`rts-host`'s rule 1 names as the specific temptation this crate must refuse.
A specifier containing `:` before any `/` is a scheme and is never a path.

A Windows absolute specifier (`C:/x`) matches that shape. It is left as
written, which is what happens today — `is_relative` is false for it too — so
the rule introduces no change for it. Said here because the shape is shared and
the reader will ask.

**Row 4 falls through rather than failing.** If `baseUrl` resolution raised on
a miss, `baseUrl: "."` would break every import of an installed package in the
same project. Falling through costs one `is_file` probe per bare specifier per
load — paid only by projects that set `baseUrl`, and only at load time, never
in a hot path.

## 5. Discovery, and the no-config guarantee

From the entry file's directory, upward to the filesystem root, first
`tsconfig.json` wins. `extends` is followed.

**No `tsconfig.json` found → there is no map, and resolution is byte for byte
what it is today.** This is the guarantee that makes the change testable: every
program that exists is its own regression test, and a diff in behaviour for a
project without the file is a defect by definition, not a trade-off.

### One map per program — a deliberate subset

`tsc` uses the `tsconfig.json` nearest to EACH file. This uses one map for the
whole program: the entry's.

The reason is that per-file maps make resolution depend on where a file sits,
so one module reached through two importers under different configs could
resolve its own imports two ways — and the loader keys modules by resolved
path, so that is two copies of one module with two namespaces. Stated as a
subset rather than discovered later as a bug.

## 6. The two destinations — rule 4

`rts-host`'s rule 4: a program compiled to memory and to an object file must be
the same program, and any difference is stated and is about the destination.

- **Static `import "@/x"`** — `rewrite` (`graph/mod.rs:161`) replaces the
  specifier in the tree at build time. The alias does not survive into the
  object. Free.
- **Literal `import("@/x")` / `require("@/x")`** — the walk already resolves
  these and records them in `resolutions`, which the object carries. Free.
- **Computed `import("@/" + name)`** — in no table, on either destination,
  because the static walk pre-registers only LITERAL specifiers and
  `rts_core::entry::module_import` reads only what is already registered
  rather than loading anything new (`rts-host/README.md`'s own "What it does
  not do yet"). Under JIT the runtime resolver hook — `rts_host::graph`'s
  `resolve_specifier` — still answers `None` for it, exactly as it does a
  bare or `node:` specifier — the PATH can be resolved, but the IMPORT was
  never told to compile that module, so it fails with the same ordinary
  message (`rts_core::entry::common_js`'s `require`, "cannot find module")
  on both destinations. **Both destinations refuse it, identically.** This
  makes rule 4 STRONGER for aliases than for a plain relative specifier, not
  weaker: an alias inherits a behaviour that is the SAME everywhere rather
  than one that differs.

No new table, no new relocation, no map carried into the binary.

## 7. Two edges that must not be discovered later

**Remote programs get an empty map.** `relative_imports` is public because
`rts run https://…` mirrors a program and its imports into a temp directory
before compiling. A remote program has no local project, and a remote `@/…`
resolving against the local machine's `tsconfig.json` would read local files on
a remote program's behalf. That path is handed an empty map, stated at the call
site.

**A `paths` target that escapes the project is allowed but recorded.** `tsc`
permits `"@lib/*": ["../../shared/*"]`. Refusing it would diverge from the
semantics chosen for this work; it is resolved like any other target, and the
resulting absolute path is what `resolutions` carries.

## 8. Where the code goes — rule 6

Files stop at 500 lines. `resolve.rs` is 162 and `graph/mod.rs` is 429, so
neither absorbs a `tsconfig` reader.

- `graph/tsconfig.rs` — **new.** Finding the file, following `extends`, parsing
  the two fields, and matching a pattern. Nothing here touches the loader.
- `graph/resolve.rs` — gains `resolve_written`, the single question of §3.
- `graph/mod.rs` — its six `is_relative` sites become `resolve_written`. No
  other change.

Rule 1 (this crate holds no semantics) is satisfied: which file a name means is
not what JavaScript means by anything. It is the question `resolve.rs` already
owns.

## 9. `paths` semantics — full `tsc`, and what "full" is

Chosen deliberately over a subset, so this states what the whole is:

1. A pattern holds **at most one** `*`. More than one is an error in `tsc` and
   is an error here, named.
2. Patterns with no `*` are matched first, exactly.
3. Among wildcard patterns, the one with the **longest literal prefix** before
   the `*` wins. Not source order.
4. A pattern's value is a **list**, tried in order; the first substitution that
   `extended()` resolves to a real file wins. A list whose every entry misses
   falls through to row 4 of §4.
5. `baseUrl` is the base that a relative `paths` target resolves against. With
   `paths` set and `baseUrl` absent, targets resolve against the directory of
   the `tsconfig.json` that wrote them — which matters with `extends`, where
   the writing file and the final file differ.

Point 5 is the one most likely to be got wrong, and point 3 is the one most
likely to be got wrong silently.

## 10. Testing — rule 5

`rts-host`'s rule 5: *a test here runs the program.* Not a unit test of pattern
matching; a program that imports through an alias, runs, and answers.

| # | What it proves |
|---|---|
| 1 | Static `@/…` import runs and answers, JIT |
| 2 | The object CARRIES the aliased module (its module table counts two), asserted in `cargo test`; and the alias goes into `tests/aot/graph.ts`, so the **diff** of the two destinations is made by the blocking smoke that already runs it |
| 3 | A project with **no** `tsconfig.json` resolves identically to today |
| 4 | `node:fs` and `rts:egui` still reach the host with `baseUrl: "."` set and files that would shadow them present — §4 row 2 |
| 5 | A bare package import still works with `baseUrl` set — §4 row 4's fall-through |
| 6 | Longest-prefix wins over source order — §9 point 3 |
| 7 | A `paths` list falls through its entries to the first that exists — §9 point 4 |
| 8 | `extends` resolves targets against the writing file — §9 point 5 |
| 9 | Computed `import("@/" + n)` runs under JIT and is refused **by name** under AOT — §6 |
| 10 | A cycle through an alias is refused by name, as a relative cycle is |

### Where a test may be written, and where it may not

`crates/rts-host/tests/aot_object.rs`'s own header states the constraint that
shapes the row above: running an object file needs a linker and the
`rts-runtime` staticlib, and `cargo test` builds neither. So a crate test may
claim what an object CONTAINS, and only the CI smoke may claim what it
ANSWERS. The first draft of this section asked for a diff in a crate test,
which cannot be written; it was corrected while planning, not discovered while
implementing.

## 11. What this does not do

- **No `node_modules` resolution.** The loader does not do it today and this
  does not add it. `baseUrl` making a bare name a file is not the same
  capability and must not be read as a step toward it.
- **No type checking.** `tsconfig.json` is read for two fields. Everything else
  in it is ignored, and ignoring it is not a promise to honour it later.
- **No IDE contract.** That `tsconfig.json` is the format means an editor
  resolves the same aliases with no plugin. That is the reason for the choice,
  not a guarantee this work maintains.

## 12. Open, and deliberately not decided here

Whether `rts emit-types` should learn to WRITE a `tsconfig.json` stanza, and
whether `rts init` should scaffold one. Both are `rts-cli` questions about a
different crate's surface, and neither blocks this.
