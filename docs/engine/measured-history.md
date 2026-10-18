# The measurements, kept — and why they are not in CLAUDE.md any more

**2026-10-01.** Every number below was in `CLAUDE.md`, loaded into every session.
They are moved rather than deleted, because the honesty floor says a measured
number stays real and states what produced it — but a number from 08-09 about a
corpus that has since doubled, and a comparison against an engine that was
deleted on 08-10, are **not** things a session needs in front of it to work.

What stayed in `CLAUDE.md` is the current figure and the rules these measurements
produced: compare per file and never net, a share that falls because the ruler
grew is not a regression, and the generated block in `README.md` is the one source
for the cross-runtime number.

What is here is the trail. Read it when a number needs context, not before.

---

## The suite, measurement by measurement

**The share fell by eight between 08-15 and 08-22 and this line does not claim
to know why.** What it can say is what the drop is NOT: each of the 62 failing
files was re-run against a kept binary of the tree at `97f66385`, and **all 62
fail there too** — 13 on an assertion and 49 on an uncaught exception. So none
of them is a regression from the optimisation work of 08-21, and the comparison
is per file rather than net, which is the only form the claim takes here.

The corpus itself did not move (808 both times), so the eight are files that
stopped passing somewhere in the ninety commits between the two measurements —
or on 08-15's own machine state. `node_fs`, `node_dns`, `node_tls`, `node_dgram`,
`net_*`, `tls_*` and `gpu_compute` are 36 of the 62, which is where to look
first.

---

**The share fell between 08-10 and 08-14 and nothing here claims to know why.**
The corpus grew by nine files and there were commits between the two
measurements that neither was taken across, so the drop is not attributable to
anything by subtraction. What IS attributable was measured per file, against a
binary of the tree as it stood before the work: 748 → 750 → 754, six gained,
**none lost**. That is the only comparison a number of this shape supports —
and the file that used to HANG is gone from the column:
`for_await_break_return.test.ts` timed out on every run until `for`-`of`
stopped materialising its sequence.

---

This line used to carry a copy, and the copy went stale twice. It said 728 of
762 (95.5 %) for 2026-08-15, then 1 179 of 1 514 (77.9 %) for 2026-08-28, while
the generated block said something else — the second time by two and a half
points. A pointer cannot do that, which is the whole reason this paragraph is a
pointer now: the same "one source, generated views" this file demands of a
runtime symbol, applied to a number about itself.

---

It was 674 of 708 before an earlier growth of the corpus, and 666, 646, 630, 593
and 419 earlier in the same stretch. Every step between those figures was
measured PER FILE against a kept binary and cost **nothing**: the LOST list is
empty at each one, which is the only form the claim "no regression" takes here.
The net number never was.

**The denominator moved by 54, and both halves are stated because of it.** Those
are `tests/cross-runtime/obfuscated/` — real `javascript-obfuscator` output over
seeds that each exercise one area. An obfuscator emits legal JavaScript nobody
writes by hand, which is the syntax a hand-written corpus never reaches, and the
first run of it found three bugs on a tree that had just measured 674 of 708:
**twelve programs HUNG** because a name assigned in a loop's test carried nothing
across the back edge, five were refused for `super[e]`, and five answered wrongly
because a computed method key came out enumerable. None of the three needed an
obfuscator to be reachable. `scripts/obfuscated/README.md` is how to make more,
and says why a name already in the corpus is never re-emitted.

---

The 08-10 figure was measured by the same `suite_run`, one process per file, on
the same corpus plus the two files that day's own work added — which is why the
denominator moved by two and is stated rather than smoothed over. The line above
said 626 of 797 for a day in which the number had already moved; a measured
number that is not re-measured becomes a claim, which is the thing this
paragraph exists to refuse.

---

**The number that says how far this still is: the OLD engine passes 777 of the
same 797** — 779 until two fixtures asserting a `super` JavaScript does not have
were corrected, which the old engine passed by implementing `super` wrongly.
Both engines were measured over the same corpus by `scripts/measure_engines.sh`
— deleted with the second engine, since a script that runs two things can run
neither when one is gone — one process per file. **167 files passed only on the
old engine and 16 only on the new.**

Read that as a work list and not as a loss: `rts-codegen-new` was DELETED on
2026-08-10, and deleting it cost none of those 167. It had stopped running
anything at the cutover — `run`, `test` and `compile` had already moved — so
what the crate still held was `ir`, `eval` and `emit-types`, and each of those
was rebuilt on this engine first. The 167 are what this engine does not do yet,
which was true the day before as well; what changed is that there is no longer a
second engine that could be measured instead of fixed.
---

The DENOMINATOR changed that day and both halves are stated because of it: 21
files were removed for testing surfaces this engine will not have in that shape
(`gc`, `ptr`, `mem`, `alloc`, `ffi`, `trace`), and SIX of them were passing. So
the count fell by six while the share rose, and neither number alone says that.
`crates/rts-host/examples/suite_run.rs` produced it, one process per file,
because an uncaught exception and an endless loop each take the process with
them and a single-process harness would report whatever it reached first as the
score. It compiles a file with a relative import as a GRAPH, which it did not
until that day: measuring those on their own bound every import to nothing and
reported an instrument's limit as the engine's — 14 assertions in one file.

---

Read the columns together rather than the first alone, and read files that move
BETWEEN them as what they are: one that starts compiling and then fails an
assertion has moved a number in the direction that looks like regression.

---

**The gap has no single cause, and its shape is the work list.** As triaged at
194 files (08-09, before this round took 27 of them):

| n | the new engine answers | reading |
|---|---|---|
| 93 | compiles, runs, FAILS an assertion | a wrong answer, not a missing feature |
| 64 | `TypeError: undefined is not a function` | was un-triageable; the message now names the callee |
| 11 | `Unbound("x")`, `Unbound("v")`, `Unbound("R")` … | ordinary LOCAL names — scope, not a missing library |
| 22 | a missing global, `rts:`/DOM surface, a hang | mostly decisions already taken elsewhere |

The third row is worth listing apart because a missing `WeakRef` is a library gap
while a missing `x` is the emitter losing a binding — those eleven were five
causes, the largest being that `var` was never distinguished from `let`.

**Naming the callee is what made the second row workable, and its answer was
"there is no single cause".** Once `atomic.*` and the unnamed optional-chain
sites are set aside, those 64 files spread over ~50 distinct missing operations,
mostly one file each. Expect volume, not a switch.