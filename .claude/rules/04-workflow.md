# Workflow — conventions, tests, debug, benchmarks

## Conventions

- Code language: Rust (English identifiers). Documentation language: English
  (all docs/specs/README — owner decision 2026-07-05)
- Communication language: Portuguese
- Commits follow conventional commits: `feat:`, `fix:`, `perf:`, `refactor:`,
  `docs:`, `chore:`
- A new namespace must be registered in `abi::SPECS` (and `rts.d.ts` is generated
  from there)
- `rts.d.ts` contains only `declare module "rts"` — generated from `abi::SPECS`,
  CI lints the committed file against the generator
- Build is via `cargo` directly — `xtask` was removed. The project is a crate
  workspace in `crates/` (the engine is `rts-codegen-new` + the `rts-adapters`
  value model); `cargo test --workspace` covers all crates in one run

## General design rules

- Don't implement high-level APIs in Rust — Rust only exposes raw primitives via
  `"rts"`
- TS packages in `builtin/*` build ergonomic APIs over `"rts"` (in this branch:
  `console/`, `globals/`, `rts-types/`)
- `rts.d.ts` contains only `declare module "rts"` — do not add other modules
- Numeric handles (u64) for runtime resources (buffers, sockets, dynamic
  strings, etc)
- Standalone distribution: runtime support resolved by precompiled `.o/.obj`
  objects (via `RTS_RUNTIME_OBJECTS_DIR` or a `runtime-objects` folder next to
  `rts`); we do not depend on external download at build time

## Progress bar for long tasks

When the user asks for multi-step work (e.g. new namespace, feat:js/feat:ts
feature, multi-file fix) show an ASCII progress bar on each significant change,
anchoring the user's perception of how much is left.

Format:

```
[▰▰▰▱▱▱▱▱▱▱] 30% — short current-step description
```

Rules:
- 10 segments: `▰` filled, `▱` empty. The percentage is the real value, not the
  segment count (e.g. 25% = 2 full segments + 50% of the 3rd rounded to full).
- Update on each concrete change: file created, build passed, test ran, commit
  made.
- On error: prefix `❌ erro:` and roll the percentage back to where confidence
  dropped. Continue from there.
- Final milestone: `[▰▰▰▰▰▰▰▰▰▰] 100% ✅ — summary (PR #N, X/Y tests)`.

Typical steps (new namespace):
- 10% mod.rs created
- 25% abi.rs defined
- 45% ops.rs implemented
- 55% rt.rs created
- 70% registered in SPECS + mod.rs + rt_all
- 80% JIT registered + build.rs updated
- 90% build passed + basic fixture ok
- 100% PR opened/merged

## Taking GitHub issues

When you start working on an issue (e.g. the user says "let's do #97"), before
coding mark the issue as taken via `gh issue edit` or by commenting — so other
contributors know someone is already on it.

Minimum form: comment on the issue indicating start of work.

```bash
gh issue comment <num> --body "Assumindo essa issue. Trabalho em andamento."
```

When possible, assign yourself via `gh issue edit <num> --add-assignee @me`
(works if the authenticated account is a collaborator of the repo).

On finishing (PR merged), comment again with the PR link and close when
appropriate.

## Testing creativity

When adding/modifying features, a happy-path test is not enough. Be creative and
cover several code variations in `tests/`:

- Normal path **and** atypical paths (empty, conditional, nested, inside a loop,
  inside try/catch, in a member call, etc).
- Combine the feature with adjacent features (e.g. arrow + class, arrow +
  generics, arrow + spread).
- TS/JS edge cases — undefined, null, recursion, tail call, common identifiers
  (`__rts*`, `this`, reserved words).
- When a variation fails and is out of the current PR's scope, open an issue with
  the minimal repro and remove it from the test until the follow-up.

Tests live in `tests/*.test.ts` (`rts:test` format). Reuse the standard
template: `__rtsCapturedOutput`, a `print()` shim, `describe()` with one or more
`test()`/`expect().toBe()`. Multiple `test()` per file are welcome to cover
variations without inflating the file count.

## Pre-commit gate — read_before_commit.sh (MANDATORY for engine commits)

Before **every** commit touching `crates/rts-codegen-new/`, run the gate at the
repo root and read its full output:

```bash
bash scripts/read_before_commit.sh            # full gate (static checks + cargo build)
bash scripts/read_before_commit.sh --no-build # fast static-only pass while iterating
```

It enforces the binding rules as a commit gate:

- **HARD (exit non-zero — never commit):** forbidden crate dep / direct `use` of
  `rts-shared`/`rts-std`; broken `cargo build`. `rts-shared` and `rts-std` are
  **NOT** native/primitive — the engine reaches the runtime ONLY through the
  `rts-runtime` facade and names ONLY primordials.
- **REVIEW (read every entry; the list must shrink, never grow):** a
  non-primordial class named in codegen (`Map`/`Set`/`Date`/`URL`/… — must
  resolve via the Registry, never a hardcoded per-class path; `Symbol` is
  PRIMORDIAL since 2026-06-26; since 2026-07-03 also BigInt/Proxy/Reflect/
  ArrayBuffer/SharedArrayBuffer/DataView/TypedArrays/Atomics/WeakRef/
  FinalizationRegistry/Math — they define/intercept the value model, engine
  MAY name them (see CLAUDE.md doctrine); current draining
  targets `dateclass.rs`, `globalclass.rs`); any source file over its layer
  ceiling **(codegen ≤1000 / engine ≤700 / rest ≤500)** (split into a
  folder/subfolder of cohesive submodules — never append to an already-oversized
  file).
- **INFO:** `todo!()`/`unimplemented!()` markers (fine as WIP, never as a shipped
  "pass").

When the feature you picked is blocked by a missing engine capability, shift
focus and implement the blocker first (modest, incremental), then return — state
the shift explicitly in the commit/PR.

## How to test

```bash
cargo test                                        # unit tests + fixtures
cargo build --release                             # release build
$env:RUST_BACKTRACE="full"; target/release/rts.exe run file.ts                # run via in-memory JIT
$env:RUST_BACKTRACE="full"; target/release/rts.exe compile -p file.ts output  # native compile (AOT)
target/release/rts.exe apis                       # list available APIs
```

### Debugging individual test failures

**Rule:** when investigating failures, ALWAYS run the individual file before
running the full suite — avoids timeout and noise from other tests.

```bash
# run only the file that failed
target/release/rts.exe test tests/foo.test.ts

# inspect generated Cranelift IR (BEFORE executing)
target/release/rts.exe ir tests/foo.test.ts 2>&1 | head -60
```

`rts ir` shows the IR of each compiled function. Use it to diagnose:

- **"unknown namespace member `X.Y`"** — X.Y has no handler in codegen
  (`calls/mod.rs`) nor an ABI entry. Add a handler or register it in the ABI.
- **"illegal instruction" / SIGILL** — invalid IR (wrong type, iconst out of
  range, brif without branch). See which block precedes the trap in the IR.
- **"access violation"** — load/store with a null ptr. Check that handles were
  initialized before use.
- **Wrong result (no crash)** — compare IR with expected behavior. Look for
  iconst 0 where it shouldn't be (a placeholder), or a wrong cast.

**Typical workflow:**

1. `target/release/rts.exe test tests/failing.test.ts` — see the error
2. `target/release/rts.exe ir tests/failing.test.ts 2>&1` — see whether it
   compiles and what IR it generates
3. If the IR doesn't compile (codegen error): fix in `calls/mod.rs` or the
   relevant crate
4. If the IR compiles but the result is wrong: analyze the IR of the problem fn,
   compare types/conversions
5. Rebuild (`cargo build --release`) and repeat

**Note:** the `target/release/rts.exe` binary may be stale if commits were merged
without a rebuild. Always `cargo build --release` before debugging suspected
failures (especially "unknown namespace member", which may be a feature already
implemented in recent commits).

**Mandatory:** always set `RUST_BACKTRACE=full` before running `rts.exe`.
Without it crashes show a shallow stack trace with no useful context. The crash
handler (`src/crash.rs`) uses this variable to show full frames.

```powershell
# PowerShell — set for the session:
$env:RUST_BACKTRACE = "full"
```

Codegen fixtures live in `tests/fixtures/*.{ts,out}`. The `codegen_fixtures`
test compiles the `.ts` and compares stdout with the `.out` byte-for-byte. To add
a new fixture:

1. `tests/fixtures/<name>.ts` — program
2. `tests/fixtures/<name>.out` — expected output (LF, no CRLF)
3. `#[test] fn fixture_<name>() { run_fixture("<name>") }` in
   `tests/codegen_fixtures.rs`

## Codegen debug — `rts ir`

To inspect the Cranelift IR generated for any program before define+compile, use
`rts ir`:

```bash
target/release/rts.exe ir file.ts 2>&1 | head -100
```

Prints the full IR of each `user fn` plus `__RTS_MAIN` (top-level). Output goes
to stderr. Does not execute the program.

**Use `-e`/`eval` for snippets** — avoids leaving temp files around the project.
Relative imports (`./mod`) don't work in eval (only builtins `import { x } from
"rts"`).

**When Claude should use this:** whenever debugging performance or suspecting
inefficient codegen. Reading the IR shows immediately:

- loops with redundant `load`/`store` (vars not promoted to Cranelift Variables,
  sites without `gv` cache);
- duplicated lowered subexpressions (try_operator_overload / try_bin_imm calling
  lower_expr before checking whether they'll use it);
- unneeded `uextend` in comparisons that go straight to `brif`;
- f64↔i32 conversions in a hot loop (literals like `1.0` misclassified);
- repeated `global_value` for the same symbol;
- extern calls that could be inline intrinsics.

**Usage pattern:**

1. Run a bench (RTS slow? check the gap with Bun/Node).
2. `rts ir file.ts 2>&1 | sed -n '/<fn-of-interest>/,/^---/p'` — isolate the
   problem fn.
3. Look at the `block` that is the hot loop's header/body. Look for:
   - how many `load`/`store` per iteration (ideally 0 for local vars);
   - how many `call` (each extern call is expensive);
   - duplicated subexpressions (same `fmul`/`fadd` repeated).
4. Identify the cause in codegen (`src/codegen/lower/`) and fix.
5. Re-dump to confirm; run `cargo test --release --lib` + `target/release/rts.exe
   test` to ensure no unexpected regression (intentional regression must be
   explicit and justified).

**Real example (commit 4a418d1):** `x*x + y*y <= 1.0` in a loop had 6× `fmul x x`
+ 3× `fmul y y` + 3× `fadd` in the IR — `try_operator_overload` and `try_bin_imm`
lowered subexprs twice before knowing whether they'd use them. The fix reduced it
to 1× each (~6% faster in Monte Carlo).

## Benchmarks

Canonical benches in `bench/`:

- `monte_carlo_pi.ts` — pi estimation by Monte Carlo 10M (inline xorshift64)
- `pi_bigfloat.ts` — pi via Machin 30 digits using `bigfloat`
- `pi_machin.ts` — pi via Machin in f64 (16 digits)

Current scoreboard (medians, updated 2026-05-01):

| Bench                       | RTS JIT | RTS AOT | Bun    | Node    |
|-----------------------------|---------|---------|--------|---------|
| Monte Carlo 10M             | 26.8 ms | 16.9 ms | 91.8 ms| 113.9 ms|
| Monte Carlo 10M (8 workers) | 30.3 ms | —       | 147.6 ms (Workers) | — |

RTS AOT vs Bun: **5.14× faster**. RTS multi-thread vs Bun Workers: **4.66×
faster**.

HTTP server (issue #399 + actix-web): peak **29k req/s** (78% of pure-Rust actix
on the same workload, 2× more than `Bun.serve`).

Full suite:

```bash
powershell.exe -ExecutionPolicy Bypass -File bench/benchmark.ps1
```

## Status

The cutover happened: `rts-codegen-new` (value model in `rts-adapters`) is the
only engine; the old engine + `rts-mir` are deleted. Honest cross-runtime parity
is **~76.5%** as of 2026-07-05 (auto-updated badge; re-measure before quoting) —
the engine has the sound value model and is re-filling coverage. The deleted old
engine's 100% (372/372, tag `v0.0-202606072107`) is the bar to re-clear. Always
re-measure (`scripts/measure_new.sh` / cross-runtime report); never quote a remembered
number or the old 94.3%/100%/70.7% framings. See `00-meta.md` "HONEST CURRENT
STATUS".

Heavy issues still open:

- **#195** mutable closures — env-record refactor; blocked by #90 (block params).
- **#207** real async/await event loop — Promise refactor.
- **#213** module exports — resolver refactor.
- **#216** Symbol as computed key — side-channel HashMap.
- **#217** real weak WeakMap/WeakSet semantics + FinalizationRegistry.
- **#218** Proxy — interception in codegen.
- **#222** real Map/Set Symbol.iterator (today only a stub).
- **#223** dynamic import.
- **#211** generators / **#219** BigInt / **#225** Intl — candidate-discard.
