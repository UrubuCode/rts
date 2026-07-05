# Cross-Runtime Parity Testing

Test system that validates RTS's JS spec compatibility by comparing
outputs against **Bun** and **Node** on standalone TypeScript fixtures.

## Components

- **`tests/cross-runtime/*.ts`** — TS fixtures runnable on any of the 3
  runtimes. No `import "rts"`, no `JSON5`/`Bun`/`Deno`/`process`.
- **`scripts/cross_runtime_check.sh`** — runs each fixture on the 3 runtimes
  (parallelized via `xargs -P`), compares stdouts, generates JSON.
- **`.github/workflows/cross-runtime.yml`** — CI that runs on PR + schedule.
- **`docs/specs/cross-runtime-roadmap.md`** — living list of planned
  fixtures (checklist marked as new batches are added).

## How to run locally

```bash
cargo build --release
bash scripts/cross_runtime_check.sh
```

Prerequisites: `bun` and `node` on PATH.

## Externally consumable JSON report

The CI commits `.github/cross_runtime_report.json` on every update
(push to `main` + weekly schedule). Any external dashboard can
consume it via raw URL:

```
https://raw.githubusercontent.com/UrubuCode/rts/main/.github/cross_runtime_report.json
```

Structure:

```json
{
  "results": [
    { "name": "01_logical_truthy", "status": "pass",
      "bun": "...", "node": "...", "rts": "..." },
    ...
  ],
  "summary": {
    "total": 107, "pass": 40, "rts_diverge": 22,
    "bun_node_diverge": 0, "errors": 45, "rejected": 0
  }
}
```

External sites/dashboards can fetch the JSON and render a chart of
parity progress over time (comparing different releases).

## Output categories

Each fixture falls into one of 5 categories:

| Status | Meaning | Action |
|---|---|---|
| `pass` | RTS = Bun = Node | ✅ parity ok |
| `rts_diverge` | RTS ≠ Bun = Node | ❌ RTS bug — open an issue |
| `bun_node_diverge` | Bun ≠ Node | ⚠️ engine difference — skip |
| `rts_error` | RTS crashed or panicked | ❌ RTS bug |
| `rejected` | Fixture uses an RTS-only API | 🚫 move to `tests/*.test.ts` |

## CI policies

- **On PR**: runs automatically, comments on the PR with a divergence table,
  **does not block** merge (opt-in for now, to evolve into required once
  we have broad coverage).
- **On the weekly schedule** (Monday 6h UTC): if a new regression shows up,
  automatically opens an issue with the `cross-runtime` label.
- **On push to `main`**: runs as a sanity check (JSON artifact saved).

## Issue dedup by hash

The scheduled issue auto-create uses **two levels of hashing** to avoid
duplicates:

1. **Per-divergence hash** (`sig`): truncated SHA-1 of
   `name|status|bun_output|node_output|rts_output`. Same signature =
   same bug. Inserted in the body as `<!-- cross-runtime-sig: <12 chars> -->`
   before the outputs block.

2. **Aggregate hash** (`aggregateHash`): SHA-1 of the set of sorted
   sigs. Identical set of divergences = same hash. Inserted
   in the body footer as `<!-- cross-runtime-hash: <12 chars> -->`.

### Decision logic on each schedule run

```
divergencias_atuais = run | filter(rts_diverge ou rts_error)
sigs_atuais = map(divergencias_atuais, sig)
hash_atual = sha1(sort(sigs_atuais).join(","))

issues_abertas = labels:cross-runtime

if exists(issue with hash_atual no body):
  # Identical set already has an issue — just comment "still present"
  comment(issue, "🔁 Schedule run YYYY-MM-DD — persistem")
elif all(sigs_atuais já estão em alguma issue aberta):
  # Subset already covered, spread across multiple issues
  comment(em cada issue afetada)
else:
  # There are brand-new sig(s) — create a new issue with only the new ones
  create_issue(divergencias_novas, hash_atual no footer)
```

This guarantees:
- **Same persistent bug** → no duplicate issue, a "still present" comment
  marks the timeline.
- **New set of bugs** → new issue with only the brand-new ones.
- **New bug + old bugs** → new issue with only the new ones; old ones get a
  comment on their existing issues.

### Why a 12-char truncated hash

Enough to avoid collision given the small number of fixtures (hundreds
in the worst case). Space of 16^12 = 2.8e14, birthday collision at
~16M distinct divergences — far beyond realistic.

## Adding a new fixture

1. Create `tests/cross-runtime/NN_<description>.ts` with `console.log` covering
   the JS behavior you want to validate.
2. Validate locally that the 3 runtimes match. If RTS diverges, that's a bug —
   open a fix before merging.
3. Normal commit. CI validates on the next push/PR.

## APIs forbidden in cross-runtime

These are RTS-only or runtime-specific. The script rejects fixtures that
use them (pre-execution regex check):

- `import { ... } from "rts"` — native RTS namespaces
- `JSON5` — RTS-only global
- `Bun` global — runtime-specific
- `Deno` global — runtime-specific
- `process` global — Node-specific

If a fixture needs any of these, it goes in `tests/<name>.test.ts`
(RTS suite via `rts:test`) instead of cross-runtime.

## Auto-created issue categories

Instead of creating one giant issue with all divergences, the workflow
groups by **thematic category** (one issue per area). Current mapping
in `.github/workflows/cross-runtime.yml` in the "Auto-create issues" step:

| Category | Covers |
|---|---|
| `regex` | regex methods, named groups, indices, unicode |
| `url` | URL, URLSearchParams |
| `json` | JSON.parse/stringify, replacer/reviver |
| `intl` | Intl.NumberFormat/DateTimeFormat/Segmenter |
| `streams` | ReadableStream, CompressionStream, TextDecoder stream |
| `typed-buffers` | ArrayBuffer, DataView, TypedArray, BigInt, Atomics |
| `web-api` | Blob, File, FormData, Headers, Request/Response |
| `events-async` | AbortController, EventTarget, MessageChannel, microtask |
| `classes-errors` | classes, instanceof, Error family, typeof |
| `promises` | Promise.all/race/withResolvers, async/await |
| `fn-closure-syntax` | closures, function meta, destructuring, templates |
| `array` | array methods, iter, sparse, groupBy, set ops |
| `object-meta` | Object methods, Proxy, Reflect, Symbol |
| `string` | advanced string methods |
| `numeric` | Math, Number format, coercion, bitwise, NaN |
| `date` | Date methods |
| `misc-platform` | WeakRef, structuredClone, dynamic import, etc. |
| `other` | fallback if the name doesn't match |

Each category with ≥1 brand-new divergence creates/updates its own issue
with labels `cat:<category>` + `cross-runtime` + `bug`.

## Weekly history

The CI commits a snapshot to `.github/cross_runtime_history/YYYY-MM-DD.json`
on each schedule run (1× per week). The `.github/cross_runtime_history/index.json`
file keeps a chronological list for dashboards to consume:

```json
{
  "entries": [
    { "date": "2026-05-10", "pct": 37.4, "pass": 40, "total_valid": 107, ... }
  ]
}
```

Detailed snapshots (`YYYY-MM-DD.json`) carry the names of the divergent
fixtures without full outputs — saving space long-term.

## GitHub Pages dashboard

The `parity.html` page on the project's GitHub Pages consumes
`.github/cross_runtime_report.json` + `.github/cross_runtime_history/index.json` and renders:

- **Current parity %** (big number)
- **Stats**: pass / diverge / error / total
- **SVG chart** of % evolution across weeks
- **Table** with pending fixtures

Final URL: `https://urubucode.github.io/rts/parity.html`

Updated automatically when the cross-runtime workflow finishes on
main or when `.github/cross_runtime_report.json` changes.

## Known cross-runtime bugs (track)

The list keeps living here as they show up:

- `Number(null)` → RTS NaN, JS 0
- `parseInt("abc")` → RTS i64::MIN, JS NaN
- `[null,1,null].join("/")` → RTS "0/1/0", JS "/1/"
- `var: number = 8080` in concat → RTS 8176 (fcvt bug)

(These haven't become fixtures yet because the fixes are pending.)
