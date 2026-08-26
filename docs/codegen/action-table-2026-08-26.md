# A pass over the whole action table: two shipped, one refuted, two bugs

*2026-08-26, over `652738bb`. Every number here was produced by
`target/release/rts.exe` against `bench/analytic.ts` or a dedicated micro-bench,
on one Windows machine, with node v25.9.0 and bun 1.4.0 run on the same file.*

**Read the machine caveat first.** This machine was building for most of the
session, and `bench/analytic.ts` could not resolve any of it: three alternating
runs of two binaries put **all ninety rows in overlapping ranges**, with one
baseline row moving 212 → 468 ns between its own runs. Every claim below that
survives comes from a dedicated micro-bench that takes a minimum of five inside
one process, alternated between binaries, and is stated as a pair. The analytic
table is used here only to **rank** actions, never to score a change.

---

## 1. What the table says, once the eliminated rows are separated out

The ratio column of `analytic.ts` against node and bun is misleading in a way
worth stating once: **a node or bun row under about 1.2 ns is a loop the JIT
deleted**, not a cost. `string slice 16` at 0.36 ns on node and `regex replace`
at 0.23 on bun are not measurements of slicing and replacing.

So the rows split in two, and only the first half supports a ratio.

### Comparable — node and bun both spend more than 1.2 ns

| action | rts | node | bun | ×  |
|---|---:|---:|---:|---:|
| `regex exec+group` | 1440.8 | 33.2 | 7.3 | 197 |
| `string split 16` | 1210.0 | 34.4 | 8.8 | 137 |
| `regex test` | 105.6 | 18.2 | 1.4 | 78 |
| `binary alloc Uint8Array 64` | 603.7 | 27.3 | 9.8 | 62 |
| `array map 16` | 86.6 | 1.9 | 2.0 | 47 |
| `call closure make+call` | 261.1 | 7.8 | 6.0 | 44 |
| `array filter 16` | 87.0 | 2.1 | 2.7 | 42 |
| `flow generator next` | 415.5 | 15.2 | 14.0 | 30 |
| `string template literal` | 410.0 | 15.1 | 27.2 | 27 |
| `binary TextEncoder 16` | 717.0 | 390.7 | 27.4 | 26 |
| `json stringify small` | 1831.8 | 138.2 | 92.5 | 20 |
| `prop computed key` | 35.8 | 6.8 | 2.1 | 17 |
| `json parse small` | 2221.8 | 405.2 | 188.3 | 12 |
| `binary subarray 64` | 225.6 | 22.5 | 30.3 | 10 |
| `array join 16` | 100.5 | 11.9 | 13.8 | 8 |
| `coll Map.set existing` | 58.8 | 9.9 | 7.5 | 8 |
| `flow throw+catch` | 1095.8 | 7773.1 | 442.7 | 0.14 |

The last row is not a typo and is the one to keep in mind when reading the rest:
this engine throws and catches **seven times faster than node**.

### Not comparable — ranked by absolute cost here

`regex replace` 1150.6 · `string toUpperCase 16` 186.2 · `string slice 16`
182.6 · `string indexOf 256` 181.7 · `coll Object.keys 4` 148.4 ·
`call varargs 3` 138.8 · `alloc array literal 4` 134.9 · `string parseInt`
121.3 · `string concat 2` 120.9 · `array push+pop` 117.0 · `prop instanceof`
116.5 · `alloc class instance` 76.1 · `coll Map.get` 50.9 · `coll Set.has` 37.4

---

## 2. Where a call's time goes, measured

Four numbers, one micro-bench, five million iterations each:

| | ns |
|---|---:|
| `f(a)`, `f` statically known | **3.1** |
| `c.m(a)`, a method | **25.5** |
| `s.has(7)`, a built-in | **39** |

A static call emits `FuncAddr` and a direct call and costs almost nothing. A
method call emits `CachedGetIndirect`, then `__rts_set_call_name`, then
`__rts_call_counted`. Priced by ablation, in one binary, with the static call as
the control (unchanged at 3.15 → 3.20):

- **`set_call_name` is ~2 ns** — `c.m(a)` 25.5 → 23.6, `s.has(7)` 39.4 → 36.8.
  A whole runtime crossing per call site to record a name used only in a failure
  message. `crates/rts-codegen/src/emit/call.rs:489` already names the fix in a
  comment: an **operand** of `call_counted`, the way the argument count became
  one, rather than a call before the jump. Not done here.
- The `CachedGetIndirect` is ~4 ns, from `prop read own`.
- **~16 ns is the generic call protocol**, which is the same figure
  `docs/codegen/plan.md:110-116` already carries as "sixteen nanoseconds are
  unattributed, and every method row in the table pays them".

Section 4 is what happened when that last one was attacked.

---

## 3. Shipped: an array's `length` attributes are no longer a record per cell

`e5058342`. `array::set_length` wrote `{writable, !enumerable, !configurable}`
against the cell and the key on every array built and every `push`. For a fresh
cell that is the one call in the function that allocates.

Ablated in one binary, the two halves separately — and the **first attempt at
this ablation was wrong in a way worth recording**: the gate read
`std::env::var_os` inside the hot path and made the "before" number 2.1× too
large. Re-done behind a `OnceLock`:

| `[]` | ns |
|---|---:|
| both | 136.6 |
| without `objects::put` | 112.3 |
| **without the attribute record** | **52.4** |
| without both | 54.7 |

Non-additive, and the reason is the finding: `put`'s apparent 24 ns was mostly
the attribute lookup it performed, which the same change removes. **So there is
no separate prize left in taking `length` off the by-name write path** — that
candidate was killed by this data before anyone wrote it.

Shipped result, minimum of four alternations per binary:

| | before | after |
|---|---:|---:|
| `[]` | 142.1 | **54.0** |
| `[i]` | 184.9 | 98.4 |
| `[i,i,i,i]` | 216.6 | 129.5 |
| `new C()` *(control)* | 81.0 | 78.7 |
| `{x,y}` *(control)* | 1.10 | 1.10 |

An empty array now costs less than `new C()`. `call varargs 3` fell 241 → 139
and `alloc array literal 4` 213 → 135 in the analytic table for the same reason.

---

## 4. REFUTED: collapsing the call path from five context borrows to two

**Do not do this.** It was implemented in full, measured, and reverted.

`called` → `invoke` took **five** `with_current` borrows around one jump: the
class-constructor check, the two argument-stack pushes, the callee resolution,
and two lots of pops. Each is a thread-local, a `RefCell` borrow and a `last_mut`
on the context stack. The rewrite reduced the hit path to two, with
`entering`/`left` gathering the four activation records in one place and `invoke`
becoming the single owner of the `transmute` — a strictly simpler shape.

It bought nothing. Minimum of six alternations:

| | baseline | collapsed |
|---|---:|---:|
| `c.m(a)` | 26.02 | 25.61 |
| `s.has(7)` | 40.85 | 41.95 |
| `f(a)` *(control)* | 3.15 | 3.12 |

**The premise came from misreading an ablation, and that is the lesson.** The
ablation had cut the whole path down to one borrow and no bookkeeping at all,
giving `c.m(a)` 25.5 → 15.9 and `s.has(7)` 39 → 28.6 — and that 10-12 ns was
read as "five borrows cost 10 ns". It is not: the ablation deleted **the borrows
and the three stacks together**, and it is the stacks that carry the cost.
`with_current` is close to free.

So the 10-12 ns is real and it is still there, but the thing that removes it is
not a runtime tidy-up. It is what `crates/rts-core/src/entry/functions.rs:370-373`
already names: **a calling convention with a stack slot**, so that a callee's
argument vector never has to be pushed at all. That is a machine-layer change.

A second lesson, cheaper: during this measurement a transient load made
`s.has(7)` read 90 ns — more than twice its real cost — and the regression was
briefly attributed to the refactor. What showed the attribution was wrong was
measuring the **reverted** binary and finding the difference still there. On a
loaded machine, a single pairing is not evidence.

---

## 5. Regular expressions: two bugs, and where the rest of the time is

### 5a. Making one never collected — a legal program died

`regex/mod.rs` called `context.region.alloc` **directly**, the only site in the
crate outside `entry/alloc.rs` that did so on a path a program can repeat. Every
other allocation goes through `alloc_or_die`, which runs a collection before it
gives up. This one did not, so the region filled with cells nothing had referred
to for a long time and the program died. Two lines reproduce it; node and bun
both run them:

```js
let a = 0;
for (let i = 0; i < 300000; i++) { const r = /[0-9]/; if (r === r) a++; }
```

`new RegExp("[0-9]")` died the same way, and **no match was needed** — merely
making one was enough. `"abc".replace(/[0-9]/g, "x")` in a loop died too, which
is how it was found. One line: `super::alloc::alloc_or_die`.

### 5b. Every literal evaluation recompiled the pattern — ~57.6 µs

The falsifier was one line of JavaScript: hoist the literal out of the loop.

| `"abc123def456".split(/[0-9]/)` | ns |
|---|---:|
| literal inside the loop | **60 612** |
| same pattern hoisted out | **2 994** |

Twenty times, and it is not the matching. `mod.rs`'s first page is right that
`/a+/g` must be **a new object every time it is evaluated** — two passes of a
loop each need their own `lastIndex` — but the *compiled program* is a pure
function of the source and the flags, and sharing it is invisible to a program.

`Context::compiled_patterns` is a bounded `HashMap<(String, Flags), Engine>`,
64 entries, cleared wholesale when full. Cloning an `Engine` is an `Arc` bump in
both matchers. It holds no heap references, so the collector never has to know
about it. `Context::well_known` argues against a `HashMap` for its five names
because hashing to avoid hashing is silly; here the alternative to hashing a
short string is fifty-seven microseconds, which settles it the other way.

| | before | after |
|---|---:|---:|
| `split(/[0-9]/)`, literal in the loop | 60 612 | **5 136** |
| `match(/[0-9]/g)`, literal in the loop | 60 409 | 5 094 |
| `replace(/[0-9]/g, "x")` | *heap exhausted* | 3 934 |
| `/[0-9]/.test(s)`, literal in the loop | *heap exhausted* | 1 288 |
| making the literal, nothing else | *heap exhausted* | 1 442 |
| `split(hoisted)` *(control)* | 2 994 | 3 426 |

And the shape of the cost changed with it: `split` now grows with the subject
(5.0 / 11.3 / 18.8 µs at 12 / 48 / 96 characters) where it used to be flat at
~57 µs, because a fixed compilation dominated everything.

### 5c. What is left, with the patterns hoisted so nothing is compiled

| | rts | node | bun |
|---|---:|---:|---:|
| `test` | 101 | 18.5 | 1.4 |
| `exec`, no group | 1157 | 33.4 | 7.4 |
| `exec`, 1 group | 1443 | 41.7 | 7.6 |
| `exec`, 3 groups | 1829 | 58.5 | 8.9 |
| `exec`, named groups | 2341 | 75.7 | 65.4 |
| `exec`, `d` flag | 2487 | 201.0 | 103.3 |

**`test` costs 101 ns and `exec` of the same match costs 1157.** The extra
~1050 ns is not matching — it is building the result: an array, one Rust
`String` per group and then a `Str` per group from it, three by-name property
writes (`index`, `input`, `groups`) each with a `well_known` lookup, a `groups`
object, and an O(n) UTF-16 recount for `index`. Named groups add ~900 more and
the `d` flag ~1000. **That is the next regex item**, and it is result
construction, not the matcher.

Subject length is nearly free by comparison: `test` grows 101 → 242 ns from 6 to
4096 characters, so the UTF-16 → UTF-8 transcode costs about 0.03 ns per
character.

**One duplication found and deliberately left unpriced.** `exec` transcodes the
subject **twice** — `search` calls `text_of` and then `exec` calls it again for
the same value — so every `exec` does two full UTF-16 → UTF-8 conversions and
two `String` allocations of one text. It is named here rather than fixed
because it belongs with the rest of §6 item 1, and because the transcode rate
above says it is worth about 0.2 ns on a short subject and 140 on a long one:
real, and not where the 1050 ns is.

**A process note, because it nearly became a false number.** A first attempt at
removing it appeared to measure "no change", and the change had in fact never
reached the binary — a scripted edit matched on `\n` against a CRLF file and
silently did nothing. `git diff --stat` is what caught it. A measurement of a
change that was not applied looks exactly like a measurement of a change that
did not matter.

---

## 6. What is worth doing next, ranked

1. **`exec`'s result construction** — ~1050 ns per `exec`, and `exec` is the
   worst comparable row in the table at 197×. Not the matcher. The double
   transcode named at the end of §5c goes with it.
2. **`string split 16`** at 1210 ns and 137× is the second-worst comparable row
   and was never triaged here; it shares `string::pattern` with the regex work
   above.
3. **`set_call_name` as an operand** — ~2 ns on every method and built-in call,
   the fix already written in a comment at `emit/call.rs:489`.
4. **A calling convention with a stack slot** — the real owner of the ~16 ns
   section 4 failed to remove, and a machine-layer change.
5. `binary alloc Uint8Array 64` at 604 ns and 62×, and `closure make+call` at
   261 ns and 44×, are untriaged.

What is **not** worth doing, with the evidence in this document: taking
`length` off the by-name write path (§3), and collapsing the call path's context
borrows (§4). And `docs/codegen/object-model.md` §9 already holds the longer
list.

---

## 7. A compiler refusal found on the way, not fixed here

Legal TypeScript, refused at compile time. Node runs it; it reproduces on the
tree before any change in this document.

```ts
let sink: any = null;
function t(run: (n: number) => number): void { console.log(run(3)); }
t((n) => { let a = 0; for (let i = 0; i < n; i++) { const c = () => i; sink = c; a += i & 1; } return a; });
```

```
error: Emit(Build(ImplicitNarrowing { target: block37, position: 0, expected: F64, found: Tagged }))
```

A closure made inside a loop and assigned to an `any` binding from an enclosing
scope.
