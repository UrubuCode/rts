// Join three views of the same 74 actions: what the bench reports inside the
// whole file, what the same case costs on its own, and what IR it emitted.
//
// The two timings are kept apart on purpose. A case that is cheap alone and
// expensive in the bench is not an expensive action — it is an interaction with
// the rest of the file (a cache pushed past its slot, a shape that stopped
// being monomorphic), and only the pair says which of the two is being read.

import { readFileSync, writeFileSync } from "node:fs";

const IR = process.argv[2] ?? "target/bench-ir";
const slugOf = (s) => s.replace(/[^a-z0-9]+/gi, "_").replace(/^_|_$/g, "").toLowerCase();

const inBench = new Map();
for (const l of readFileSync(`${IR}/inbench.txt`, "utf8").split(/\r?\n/)) {
  const m = /^(\S+)\s+(.*?)\s{2,}([\d.]+)\s+(?:([\d.]+)|~0)\s*$/.exec(l);
  if (!m) continue;
  inBench.set(slugOf(`${m[1]}_${m[2].trim()}`), { ns: +m[3], minusFloor: m[4] === undefined ? 0 : +m[4] });
}

const alone = new Map();
for (const l of readFileSync(`${IR}/isolated.txt`, "utf8").split(/\r?\n/)) {
  const m = /^(\S+)\s+([\d.]+) ns\/op/.exec(l);
  if (m) alone.set(m[1], +m[2]);
}

const ir = new Map(JSON.parse(readFileSync(`${IR}/summary.json`, "utf8")).map((r) => [r.slug, r]));

const rows = [];
for (const [slug, r] of ir) {
  rows.push({
    slug,
    inBench: inBench.get(slug)?.minusFloor ?? null,
    alone: alone.get(slug) ?? null,
    instrs: r.instrs,
    calls: r.calls,
    guards: r.guards,
    cache: r.cachedGets,
    callees: r.callees,
  });
}
rows.sort((a, b) => (b.inBench ?? -1) - (a.inBench ?? -1));

const w = (s, n) => String(s).padEnd(n);
const r = (s, n) => String(s).padStart(n);
const out = [];
out.push(`${w("case", 30)}${r("in-bench", 9)}${r("alone", 9)}${r("ratio", 7)}${r("instr", 7)}${r("calls", 6)}${r("guard", 6)}${r("cache", 6)}  hot callees`);
out.push("-".repeat(140));
for (const x of rows) {
  const ratio = x.inBench && x.alone ? (x.inBench / x.alone).toFixed(1) + "x" : "-";
  out.push(
    w(x.slug, 30) + r(x.inBench ?? "-", 9) + r(x.alone ?? "-", 9) + r(ratio, 7) +
    r(x.instrs, 7) + r(x.calls, 6) + r(x.guards, 6) + r(x.cache, 6) + "  " +
    x.callees.filter((c) => !/take_thrown|thrown_address/.test(c)).slice(0, 5).join(" "),
  );
}
const text = out.join("\n") + "\n";
writeFileSync(`${IR}/report.txt`, text);
writeFileSync(`${IR}/report.json`, JSON.stringify(rows, null, 2) + "\n");
console.log(text);
