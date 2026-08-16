// Split bench/analytic.ts into one standalone file per case, so `rts ir` can be
// read for one action at a time. The whole-file dump is 27k lines with the case
// bodies anonymous and interleaved with their own callbacks — unreadable as an
// attribution instrument, which is the only thing the bench is for.
//
// Every emitted file keeps the ORIGINAL preamble (helper functions, class
// declarations, the shared fixtures) because a case measured without them is a
// different program. Only the other `bench(...)` calls are removed.

import { readFileSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";

const SRC = "bench/analytic.ts";
const OUT = process.argv[2] ?? "target/bench-cases";

const text = readFileSync(SRC, "utf8");
const lines = text.split(/\r?\n/);

// The harness reads CASES and prints a table; a per-case file drives one case
// directly, so everything from its banner onward is dropped.
const harnessAt = lines.findIndex((l) => l.startsWith("// ------") && l.includes("harness"));
if (harnessAt < 0) throw new Error("harness banner not found");

// --- find every top-level `bench(` call, by paren balance from its line start.
const calls = [];
for (let i = 0; i < harnessAt; i++) {
  if (!lines[i].startsWith("bench(")) continue;
  const m = /^bench\("([^"]+)",\s*"([^"]+)"/.exec(lines[i]);
  if (!m) throw new Error(`unparsed bench at ${i + 1}: ${lines[i]}`);
  let depth = 0, end = -1;
  for (let j = i; j < harnessAt && end < 0; j++) {
    for (const ch of lines[j]) {
      if (ch === "(") depth++;
      else if (ch === ")") { depth--; if (depth === 0) { end = j; break; } }
    }
  }
  if (end < 0) throw new Error(`unterminated bench at ${i + 1}`);
  calls.push({ group: m[1], name: m[2], start: i, end });
}

const slug = (g, n) => `${g}_${n}`.replace(/[^a-z0-9]+/gi, "_").replace(/^_|_$/g, "").toLowerCase();

rmSync(OUT, { recursive: true, force: true });
mkdirSync(OUT, { recursive: true });

const index = [];
for (const c of calls) {
  const keep = [];
  for (let i = 0; i < harnessAt; i++) {
    const other = calls.find((o) => o !== c && i >= o.start && i <= o.end);
    if (other) continue;
    keep.push(lines[i]);
  }
  // Fixed iteration count: the point is the IR, and a fixed n keeps the emitted
  // code identical between runs so two dumps are diffable.
  keep.push("");
  keep.push("const N = Number(globalThis.process?.env?.BENCH_N ?? 1000000);");
  keep.push("const t0 = Date.now();");
  keep.push("const r = CASES[0].run(N);");
  keep.push("const dt = Date.now() - t0;");
  keep.push(`console.log(${JSON.stringify(slug(c.group, c.name))}, ((dt * 1e6) / N).toFixed(2) + " ns/op", "sink=" + r);`);
  const file = join(OUT, `${slug(c.group, c.name)}.ts`);
  writeFileSync(file, keep.join("\n") + "\n");
  index.push({ slug: slug(c.group, c.name), group: c.group, name: c.name, file, lines: c.end - c.start + 1 });
}

writeFileSync(join(OUT, "index.json"), JSON.stringify(index, null, 2) + "\n");
console.log(`${index.length} cases -> ${OUT}`);
