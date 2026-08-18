# Measuring the DOM engine

> `crates/rts-dom/src/metrics/` (counters, phases, samples, audit — feature
> `metrics`) + `crates/rts-dom/examples/dom_metrics/` (the harness).
> First measurement: **2026-08-17**.

## Why not just a clock

A clock says something is slow; it does not say what. Every open question this
crate had was a question of **count**, and each has a cache on the path — and a
cache is judged by a ratio, which is a pair of counters, never a stopwatch:

- how many times does the full cascade run per element in one layout?
- how many blocks are measured twice (pre-pass, then paint)?
- how many nodes does a *local* mutation invalidate?

And one question no performance number answers at all: was what we measured
**correct**? A `.class` index pointing at a detached node does not get slow, it
gets wrong — so the system has four parts, not one.

| part | answers | cost when off |
|---|---|---|
| `counters` | how many times each operation happened | nothing — `bump!` expands to nothing |
| `footprint` | how much memory the tree holds, and in which area | always available (it is a scan) |
| `phases` | how long each named phase took, across crates | nothing — `scope()` is an empty struct |
| `samples` | *which* cases were behind a counter (which selectors were dropped, which properties are missing) | nothing — the `format!` never runs |
| `audit` | whether the tree is consistent with itself | always available (it is a scan, not instrumentation) |

This is what the `perf-claim` skill asks for **before** an optimization: the
falsifier. "The cascade is the bottleneck" dies the moment `cascade_runs` equals
the element count and the time has not moved.

## Running it

```bash
# what happened inside — counters/phases/samples valid, time NOT comparable
cargo run --release -q -p rts-dom --features metrics --example dom_metrics -- page.html

# how much it cost — time valid, counters all zero (and the harness says so)
cargo run --release -q -p rts-dom --example dom_metrics -- page.html

# record a baseline, then compare later
… --features metrics … -- page.html --json base.json
… --features metrics … -- page.html --baseline base.json
```

Options: `--viewport 1280x800`, `--iters 20`, `--build 4000`, `--json <file>`,
`--baseline <file>`, `--tolerance 5`.

Never a debug build: a debug number is not a number. The baseline diff compares
**counters, per iteration** — a counter is deterministic, so a difference in one
is always a change in behaviour, while a difference in time can be the machine.

### Phases across crates

`phases` lives in `rts-dom` because it is the crate everything else depends on,
and the question "what eats the frame" only exists if every slice is in the
*same* account. `rts-egui` opts in with its own `metrics` feature (which just
forwards `rts-dom/metrics`) and contributes `render-dom` and `paint`; any host
or loop can add its own with `rts_dom::metrics::phases::scope("name")`.

Phases nest on purpose — `cascade` inside `layout` inside `render-dom` — so the
percentages sum to more than 100%. That is what lets you read "layout is 97% of
the frame **and** cascade is 71% of it".

Currently instrumented: `load-html`, `tokenize-html`, `parse-css`, `cascade`,
`layout`, `animate`, `set-inner-html` (rts-dom); `render-dom`, `paint`
(rts-egui). **Not** instrumented: the time inside a page's own JavaScript. The
hook exists (any crate can open a phase), but `rts:dom` has not been ported to
the new engine yet, so there is no TS-side surface to call it from — when that
lands, a `script` phase joins the same table.

## The scenarios

| scenario | isolates |
|---|---|
| `parse` | tokenizer + arena + indices + the CSS in `<style>` |
| `layout frio` | opening the page: every cache empty, the only run where the cascade really executes |
| `relayout parado` | the ceiling of the caches: nothing changed, so what remains is work no memo covers |
| `texto + relayout` | a text leaf changes — what the invalidation preserves |
| `classe + relayout` | a class changes — the price of restyling instead of just re-laying-out |
| `hover + relayout` | the backend reporting the node under the cursor, every frame |
| `querySelectorAll` / `querySelector #id` | the generic walk vs. the indexed path — together they say whether the index is used |
| `clique + bubbling` | the event walk up the tree |
| `innerHTML + relayout` | re-parsing a subtree, the shape of every script-rendered list |
| `append × N` / `append + layout /100` / `remove × N` | building and tearing down programmatically; the audit at the end is the metric, not the time |

## First measurement — 2026-08-17

`cargo run --release`, one process, `--iters 20`, viewport 1280×800, on this
tree. Corpus: `examples/claude-ai-site.html` (hand-written, small inline CSS),
the Bootstrap `cover` example with `bootstrap.min.css` **inlined** into a
`<style>` (232 KB of CSS, 21 144 rules — what the mini-browser really builds
after fetching the `<link>`), and a synthetic page of 3005 elements with a
trivial stylesheet.

| scenario | claude-ai-site (177 nodes) | cover + bootstrap (182 nodes) | synthetic (5007 nodes) |
|---|---|---|---|
| parse | 0.45 ms | 19.0 ms | 4.9 ms |
| layout cold | 1.19 ms | 21.4 ms | 22.9 ms |
| idle relayout | 0.17 ms | 0.08 ms | 7.6 ms |
| text + relayout | 0.33 ms | 0.37 ms | 6.7 ms |
| class + relayout | 0.21 ms | 0.09 ms | 6.7 ms |
| **hover + relayout** | 0.17 ms | **1.60 ms** | 6.6 ms |
| querySelectorAll | 0.011 ms | 0.015 ms | 0.40 ms |
| querySelector `#id` | 0.001 ms | 0.001 ms | 0.001 ms |

Programmatic, 4000 elements: append only **4.1 ms**, append with a layout read
every 100 insertions **261 ms**, remove all **6.8 ms**.

### What the counters found

**The cascade is not the suspect.** `cascade_runs` came out at exactly one per
element on a cold layout, and 0 on an idle relayout (100% style-memo hits). The
memoization by structural revision does what it claims.

**Hovering a Bootstrap page re-runs the cascade every frame.** 1.60 ms/frame
against 0.08 ms idle — 20×. Per frame: one global `touch()`, 98 memo entries
dropped, **49 full cascades**, 7310 candidate rules tested, and 71% of the frame
inside the `cascade` phase. The guard in `set_hovered` (only invalidate when the
hovered node changes *and* the sheet has a `:hover` rule) holds on a page
without `:hover` — the synthetic page costs the same as idle — but a real
stylesheet has `:hover`, and then the invalidation is the whole tree rather than
the two nodes whose state changed.

**Insertion invalidates globally, and it is quadratic.** `append_child`,
`insert_before`, `prepend_child` and `remove_node` call the global `touch()`,
which clears `computed_memo`, `base_memo`, `layout_measure_cache` and
`intrinsic_width_cache` *whole*. The granularity exists — `set_attr`/`set_text`
use `touch_subtree` — it just is not on the insertion path. Measured: building
4000 elements while reading layout every 100 gives **82 120 full cascades** and
**156 234 memo entries thrown away**; doubling the items quadrupled the time
(2000 → 71 ms, 4000 → 261 ms).

**An idle frame re-runs the whole layout.** On the 3005-element page, an idle
relayout costs 7.6 ms with 15 015 `layout_block` calls and 55 000 `text_width`
calls, with nothing changed and 100% style-memo hits. The caches cover
*measurement*, not the paint walk — there is no per-node display-list reuse
inside `rts-dom`. The real app does not pay this today because `rts-egui` caches
the whole `DisplayList` by `render_revision`; the headless path (`rts:dom` from
TS) pays it in full, once per call.

**The stylesheet, not the tree, is what makes Bootstrap expensive.** Same page
size (182 vs 177 nodes), 42× the parse and 18× the cold layout: 118 µs per node
against 6.7 µs. `parse-css` alone is **86% of the parse** phase. Per cascade:
149 candidate rules tested, 3.2 matched.

**`candidate_indices` runs twice per element** — once in `custom_for_node` (the
`var()` pass) and once in `computed_for_node`, for the same node in the same
cascade. Half of that 149 is a repeat.

**`query_all` ignores the indices it maintains.** It walks all nodes in
pre-order and runs `matches_complex` on each: 182 nodes visited per query on the
Bootstrap page, 5007 on the synthetic one (0.40 ms). `querySelector('#id')`, which
does consult the index, is 400× faster on the same tree.

### What the samples found (fidelity, not speed)

Of Bootstrap's 21 144 rules, **2632 (12.4%) are dropped at parse** because the
selector is refused, and 4336 of 44 712 declarations (9.7%) name a property no
branch handles. The samples name them, which turns a number into a work list:

- pseudo-elements at all: `::before`, `::after`, `::file-selector-button`
- `:not()` with a compound or chained argument: `a:not([href]):not([class])`,
  `.table > :not(caption) > * > *`
- vendor pseudo-elements: `::-webkit-*`, `::-moz-focus-inner` (safe to ignore)
- ignored properties: `list-style`, `content`, `filter`, `clear`,
  `border-top-width`/`border-bottom-width` (longhands), `outline-offset`

### What the audit found

The tree is structurally sound: no broken parent↔child links, no cycles, no
stale index entries on a live node, no ids missing from the index — on every
page in the corpus.

What it does report is a **leak**, and the severity is its own for a reason:
removing a node leaves its `#id` and `.class` entries in the indices. Nothing
answers wrong (queries filter by `is_attached`, and the code says so), but
nothing recollects them either — removing 2000 nodes leaves 4000 dead entries.
The same holds for per-node derived state (listeners, input values, transitions).

### What this measurement does NOT say

One process, one machine, three pages, and `ApproxMeasurer` — not the egui text
measurer. It says nothing about how these numbers move under a real backend,
where `text_width` hits a font atlas instead of multiplying a character count;
the 55 000 calls per idle frame are a *count*, and their cost under the real
measurer is unmeasured. Nothing here has been optimized yet: these are the
falsifiers, recorded before any change.

---

## What the measurement paid for — the optimizations it justified (2026-08-18)

Three changes, each with its falsifier recorded above, each measured per
scenario against the previous binary. Numbers on this machine vary ±20% between
runs of the *same* build (measured: 36.7 / 39.2 / 39.2 ms for one scenario), so
the claims below lean on counters where the time difference is smaller than that.

### 1. The style memo returns `Rc`

`footprint::type_sizes()` said `ComputedStyle` is **1000 B** and `Rule` is
**2120 B**. `computed_style_idx` returned it by value, so every memo *hit* copied
1 KB — 12 016 hits per frame on an idle relayout of the 3005-element page, about
12 MB of memcpy per frame with a 100% cache hit rate. The cache was working and
costing.

Both memos now hold `Rc<ComputedStyle>`, and without animation the computed
value *is* the base, sharing one `Rc` instead of materializing a second copy.
`computed_style` (the public API) still returns a value: callers from outside
want their own data and call once.

Cold layout −23% to −41%, idle relayout −22% to −35%, text/class mutations
−20% to −35%. The one number that went up (idle relayout on the 182-node page,
0.075 → 0.091 ms) is `unwrap_or_default()` allocating an `Rc` where a stack
default used to do — stated rather than hidden in the net.

### 2. Hover invalidates the set that can change

`set_hovered` scanned every rule to ask "is there a `:hover`?" (2643 of them)
and then called the global `touch()`. `Stylesheet::hover_reach()` now answers
that once, cached, and classifies the reach: `None`, `SelfOnly`, `Subtree`
(`.card:hover .title`) or `Siblings` (`.a:hover + .b` — the one case that
escapes the subtree, and the declared fallback to the global). The dirty roots
are the nodes on the `hovered→root` chain that *could* match a `:hover` compound
— without that filter the `<body>` joins the chain and its subtree is the page,
which is where we started.

**Hover on the Bootstrap page: 1.618 → 0.146 ms per frame (11×.)**

### 3. Insertion and removal invalidate the subtree that moved

Same shape, different guard: `Stylesheet::position_sensitive()` —
`:first-child`, `:nth-child()`, `:empty`, `+`, `~`. Without any of them,
appending a node changes no other node's style.

**Building 2000 elements while reading layout: 21 060 → 2 003 full cascades**
(one per element created; the quadratic is gone), removal −34%.

Two mistakes the harness caught before the commit, both recorded in the commit
message: reusing `touch_subtrees(parent)` on removal was **118× slower** (it
walks the parent's subtree per removed node), and the `HashSet` inherited from
`touch_subtrees` cost an allocation per call.

### Still open, measured and not yet done

- **`parse-css` is 86% of opening a Bootstrap page** (19 ms of 22), and the
  parsed sheet is 9.22 MiB for 232 KB of CSS — each `Rule` carries a `DeclBlock`
  with two whole `ComputedStyle`.
- **`candidate_indices` runs twice per element per cascade** (`custom_for_node`
  and `computed_for_node`).
- **`query_all` ignores the `#id`/`.class` indices** it maintains.
- **12.4% of Bootstrap's rules are dropped at parse** — pseudo-elements and
  compound `:not()` lead the list.
- **Removing a node leaves its index entries** (the audit reports it as a leak).

---

## The reference: Chrome on the same pages (2026-08-18)

The goal is a DOM fast enough to stand next to a real browser, so the only
honest way to know where we are is to run the same operations in one. Chrome on
the same machine, same two files opened over `file://`, timing with
`performance.now()` over batches (a single mutation is below that clock's ~0.1 ms
resolution, so each number is a batch of 300–2000 divided by the count).

Synthetic page, 3005 elements:

| operation | Chrome | rts-dom | ratio |
|---|---|---|---|
| class toggle on a leaf + layout | 0.0053 ms | 2.9 ms → **0.133 ms** after the early-out | 25× |
| text change on a leaf + layout | 0.369 ms | 2.9 ms | 8× |
| idle frame | 0.00045 ms | 0.000 ms (cached) | par |
| `querySelectorAll('.btn, div, a[href]')` | 0.097 ms | 0.207 ms | 2.1× |
| append 2000 nodes, layout every 100 | 32.5 ms | 30.2 ms | **we win** |
| full forced relayout (padding on root) | 8.5 ms | 2.6 ms | **we win** |

Bootstrap cover page (68 elements, 232 KB of CSS):

| operation | Chrome | rts-dom |
|---|---|---|
| parse + DOM interactive | 21.9 ms | 8.9 ms |
| class toggle + layout | 0.0006 ms | 0.038 ms |
| text change + layout | 0.0010 ms | 0.160 ms |
| `querySelectorAll` | 0.0028 ms | 0.009 ms |

### How to read this, honestly

**Where we "win" we are doing less work.** Our text measurement multiplies a
character count; Chrome shapes real fonts with kerning and ligatures, does
subpixel positioning, builds an accessibility tree, and handles a CSS surface
several times larger. A full relayout being 3× faster than Chrome's is a
statement about scope, not about quality.

**Where we lose, the comparison is fair, and it is the same cause twice.**
Chrome is 25× faster on a class toggle and 8× on a text change because it
relayouts *the subtree that changed*. We relayout the document: `layout_document`
walks the whole tree every time the revision moves. That is the one structural
gap left, and it is what the next work attacks.

`querySelectorAll` being 2× slower is a separate, smaller gap: we walk the tree
in document order and filter by target key, while Chrome starts from the
`.class` bucket when the selector's key allows it.
