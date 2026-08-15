# Generated obfuscated fixtures

An obfuscator emits **legal JavaScript nobody writes by hand**, which is exactly
the syntax a hand-written corpus never reaches. `tests/cross-runtime/obfuscated/`
is what came out of pointing one at seven seed programs.

What that found on the first run, on a tree that had just measured 674 of 708:

| what | how many | what it was |
|---|---|---|
| **hung** | 12 | a name assigned in a loop's TEST had no block parameter, so the counter never moved. `for (let i = 0, c; c = s.charAt(i++); )` is what every base64 decoder looks like |
| refused | 5 | `super[e]` — an obfuscator rewrites every `super.m` into one |
| wrong answer | 5 | a method with a COMPUTED key came out enumerable, so `for`-`in` over an instance listed it |

None of the three needed an obfuscator to be reachable. Each is ordinary
JavaScript that the corpus had simply never spelled that way.

## What the second batch found

Four more seeds — generators and `yield*`, the error hierarchy, accessors and
`Proxy`, array higher-order methods — gave 54 fixtures, of which **18 of 19 new
ones pass**. The one that does not is a genuine finding and is left in the
corpus as its own reproduction:

**`obf_arrays_higher_order_everything` exhausts the heap.** 65 536 cells, all in
use after a collection. It is not allocation VOLUME: a loop allocating twenty
thousand objects collects fine, and nine of the eleven `everything` fixtures
pass. Reduced as far as cheap probes go — the flattened state machine, a
captured counter, a closure over a loop variable, rc4 decoding — and every one
of those shapes answers correctly on its own. Not reduced further; the fixture
is the reproduction.

Two outputs were also **rejected as unfaithful**: `transformObjectKeys` rewrites
`{ get v() {} }` into a form that loses the accessor, so the obfuscated program
answered something its seed did not. `generate.mjs` deletes those rather than
reporting and leaving them, because `install.mjs` reads the directory rather
than the report.

## Running it

```bash
cd scripts/obfuscated
npm install javascript-obfuscator          # not vendored; see below
node generate.mjs                          # seeds -> out/, validated
node install.mjs                           # out/ -> tests/cross-runtime/obfuscated/
```

Then validate from the REPOSITORY ROOT, which is where the harness runs:

```bash
for f in tests/cross-runtime/obfuscated/*.ts; do
  diff <(bun "$f" 2>&1) <(node "$f" 2>&1) >/dev/null || echo "DIVERGE $f"
done
```

That last step is not optional and is not the same check `generate.mjs` makes.
Bun and Node infer **module-ness** differently, and module-ness decides strict
mode: a plain `frozen.v = 2` is silent in sloppy code and throws in strict, so a
seed containing one pins the runtime's guess rather than the freeze. It was
measured — five fixtures agreed under `generate.mjs` (which runs beside a
`package.json`) and disagreed once installed. A seed must therefore avoid every
construct whose behaviour depends on the mode; `Reflect.set` answers `false` in
both, where the plain write does not.

## Never re-emit a name that already exists

`install.mjs` skips a fixture whose file is already in the corpus, and that is
not tidiness. Obfuscation is randomised, so writing an existing name replaces
one program with a different one under that name — and the per-file comparison
then reads a CORPUS change as an engine regression. Measured: regenerating the
first batch produced exactly one LOST entry that no code change had caused.

## Why the output is committed rather than regenerated

Obfuscation is **randomised**. Running `generate.mjs` again produces different
programs from the same seeds — different names, a different dead-code shape, a
different string table. So the committed files are the corpus and the generator
is how to make MORE, not how to reproduce these. Re-running it and committing
the result is a corpus change like any other: measure per file, state both
halves of the denominator.

## What a seed must be

- **Deterministic.** No clock, no randomness, no iteration over anything whose
  order the engine is allowed to choose.
- **One `console.log`,** joined — the harness compares stdout line by line and a
  single line makes a diff say which value moved.
- **Mode-independent,** for the reason above.
- **No `import`, no `process`/`Bun`/`Deno`/`JSON5`,** which the harness rejects
  by name. An obfuscator can emit `process` sniffing on its own, so
  `generate.mjs` checks the OUTPUT rather than trusting the options.

`selfDefending`, `debugProtection` and `domainLock` are off, and they have to be:
the first two are written to break under exactly the kind of harness this is, and
the third reads a location that does not exist here. `rgf` is off because it
builds `new Function`, which `tests/cross-runtime/README.md` excludes by policy.
