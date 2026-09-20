# Import path aliases, from `tsconfig.json`

```ts
import { Scene } from "@/engine/core/scene";   // instead of ../../../engine/core/scene
```

A specifier that is neither relative (`./`, `../`) nor host-provided (`node:`,
`rts:`, a bare package name) may name a file, when a `tsconfig.json` says
which one. `tsconfig.json` is the format on purpose: an editor already reads
`compilerOptions.paths` and `baseUrl` to resolve the same import with no
plugin, so the engine and the editor agree without either being taught about
the other.

## Why `tsconfig.json`, and why one function

`is_relative` used to be the predicate the loader branched on: a specifier
either walked the filesystem from `./` or `../`, or it was left as written for
the host to provide by name. An alias needed a third case, and the six call
sites that asked `is_relative` (`graph/mod.rs` and the runtime resolver hook)
each needed to ask the third case the same way, or one could answer a `paths`
pattern and another could not. `is_relative` was therefore replaced everywhere
at once by `resolve_written(from, specifier) -> Option<PathBuf>` — one
function, one answer to "does this name a file, and which" — rather than
teaching a second site what a path is.

A second site had already been taught, in another crate, and the cost came due
in this work. `rts-cli`'s `imports_a_file` chose between compiling a program as
a graph and compiling it as one file by scanning the source for `from "./"` and
its three spellings. That predicate was written when a relative specifier was
the only way to name a file, and it was correct then; the moment a `paths`
pattern became a second way, it was silently wrong, because an aliased
specifier contains none of the four. A program whose imports are ALL aliases
was therefore compiled alone, on the path that deliberately forgets the alias
map, and died at run time with `cannot resolve module "@/…" — nothing
registered that specifier`. One relative import anywhere beside the alias hid
it completely, which is why nothing caught it: every fixture and every test in
this work had one, and `rts-game`'s own entry has dozens, so the real-project
proof passed while a two-file program did not run at all. The fix was not to
extend the scan but to delete it: `names_any_file(source, entry)` exports the
loader's own resolution, and the CLI and `examples/suite_run.rs` — which held a
third copy, with the same defect — both call it. The lesson is narrower than
"do not duplicate": a copy of this rule does not fail when it is written, it
fails when the rule next grows a case, and by then nobody remembers the copy
is there.

## The precedence table

| # | Written form | Resolves to |
|---|---|---|
| 1 | `./…`, `../…` | a file — unchanged from before this work |
| 2 | `rts:*`, `node:*` | **always** the host, never a file |
| 3 | matches a `paths` pattern | a file |
| 4 | any other bare name, with `baseUrl` set | a file, if a probe finds one; otherwise fall through to row 5 |
| 5 | anything else | left as written — host by name, or an installed package |

**Row 2 is not negotiable**, and its place above `paths` and `baseUrl` is the
whole reason it is a row of its own rather than a special case of row 4: with
`baseUrl` set, every bare name becomes a file candidate, and a project that
happens to contain a directory named `node` must not let `node/fs.ts` shadow
`node:fs`. That failure would compile and lie — the import would silently
resolve to the wrong module instead of raising — which is the specific
temptation `rts-host`'s rule 1 exists to refuse. A specifier containing `:`
before any `/` is a scheme, checked first, and is never treated as a path.

**Row 4 falls through rather than failing.** If `baseUrl` resolution raised on
a miss, `baseUrl: "."` would break every import of an installed package in the
same project — packages are bare names too. A miss instead falls through to
row 5, unchanged from what happens without a `tsconfig.json` at all.

## The no-config guarantee

No `tsconfig.json` found, walking upward from the entry file to the
filesystem root: there is no map, and resolution is byte for byte what it was
before this feature existed. This is what makes the change testable without a
new fixture for every existing program — every program that already compiles
is its own regression test, and any difference in behaviour for a project with
no `tsconfig.json` is a defect by definition, not a trade-off to weigh.

## One map per program

`tsc` resolves each file against the `tsconfig.json` nearest to IT. This
engine builds one map for the whole program — the entry's — a deliberate
subset of what `tsc` does, not an oversight.

The reason is the loader's own indexing: modules are keyed by resolved path,
so if two importers under different configs could give the same module two
different alias maps, one file could resolve its own imports two different
ways depending on who reached it first — which the loader would then have to
treat as two separate modules to stay consistent, i.e. two copies of one
module with two namespaces. One map per program removes the question instead
of answering it per file.

## `paths` semantics

Full `tsc` semantics, not a subset, because a subset invites the question of
which part was left out:

1. A pattern holds at most one `*`; more than one is an error, named.
2. Patterns with no `*` are matched first, exactly.
3. Among wildcard patterns, the one with the **longest literal prefix** before
   the `*` wins — not the order they appear in `tsconfig.json`.
4. A pattern's value is a list, tried in order; the first substitution that
   resolves to a real file wins. If every entry misses, resolution falls
   through to row 4 of the precedence table.
5. `baseUrl` is the base a relative `paths` target resolves against; with
   `paths` set and no `baseUrl`, a target resolves against the directory of
   the `tsconfig.json` that WROTE it. This matters with `extends`: the file
   that wrote a `paths` entry and the file that ends up applying it can
   differ, and the target resolves against the writer, not the reader.

## The one-file-one-spelling property, and what it cost to learn

Every module the loader reads is keyed by a canonical path, and that key has
to be the same no matter which spelling of the specifier reached it — `./lib/counter`
and `@/counter`, if both name the same file, must produce ONE entry, not two.
This sounds obvious until a `paths` target is allowed to contain `..`, which
`tsc` permits and this engine does not refuse (`"@lib/*": ["../../shared/*"]`
is a legitimate, supported target that walks outside the project).

A joined path keeps its `..` in it; a canonicalised path collapses it. The
alias branch used to join and stop there, matching what the relative branch
had always done — except the relative branch's paths never happened to need
collapsing to compare equal to a plain traversal that had already normalized
along the way, and the alias branch's did. The two spellings of one file
arrived at the loader as two different strings, became two different keys,
and the loader read one of them as an entirely separate module. A test built
around exactly this shape (`a_paths_target_that_walks_out_and_back_is_still_one_module`,
`crates/rts-host/tests/import_alias.rs`) mutates the file's module state
through one written name and reads it back through the other; it caught the
bug by answering `0` where a single shared module must answer `1`. The fix is
that `settled()` — the function both the relative and the alias branch now
call on their way to a key — canonicalises unconditionally, on both branches,
so a `..` never survives into a module key. This is the property that makes
"one file, one module" true regardless of which of its names got there first,
and it is worth restating: the bug was not in matching `paths` patterns, it
was in the last step everyone assumed was already shared.

## What it does not do

- **No `node_modules` resolution.** A bare specifier that is not `paths` and
  not found under `baseUrl` is left exactly as written, the same as it always
  was; the engine does not walk `node_modules` looking for a package.
- **No type checking.** Only two fields of `tsconfig.json` are read —
  `compilerOptions.paths` and `compilerOptions.baseUrl` (plus `extends` to
  find them) — and every other field is ignored. Ignoring a field is not a
  promise to honour it later; `strict`, `target`, and the rest of
  `compilerOptions` mean nothing here and never will just because this reader
  exists.

## Cost

One `is_file` probe per bare specifier per load, and only in projects that set
`baseUrl` — row 4's fall-through has to know whether a candidate exists before
falling through. It is paid once, at load time, never in a hot path: nothing
about a compiled program's running behaviour reads `tsconfig.json` again.

## Reproducing the AOT smoke locally

The plan for this feature omitted a step that cost a wasted build cycle to
discover: reproducing the AOT smoke (`tests/aot/graph.ts`, run by
`.github/workflows/build-artifacts.yml`) locally needs BOTH of:

```
cargo build --release -p rts
cargo build --release -p rts-runtime-jit
```

The binary is the workspace ROOT package (`rts`), not `rts-cli` — `rts-cli` is
a library, and building only it produces nothing to run. Without the second
build, `rts compile` fails with `no rts_runtime_jit.lib`, because the default
AOT binary links the compiler crate (see `crates/rts-host/README.md`, "What it
does not do yet") and that crate has to exist as a build artifact before
anything can link against it.
