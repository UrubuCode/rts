// Compare two per-case IR dumps instruction for instruction.
//
// A count that fell is the claim being made; a count that ROSE is the one worth
// finding, and a net total would hide it. So every kind is reported in both
// directions, per case, and the summary lists cases that grew separately from
// cases that shrank rather than subtracting one from the other.

import { readFileSync, readdirSync, existsSync } from "node:fs";
import { join } from "node:path";

const [BASE, NOW] = [process.argv[2], process.argv[3]];
if (!BASE || !NOW) throw new Error("usage: compare_ir.mjs <base-dir> <now-dir>");

// The case body is the last anonymous function, same rule as the extractor.
function caseInsts(path) {
  const lines = readFileSync(path, "utf8").split(/\r?\n/);
  const bodies = [];
  let cur = null;
  for (const l of lines) {
    const m = /^; FuncId\((\d+)\)\s?(.*)$/.exec(l);
    if (m) {
      cur = { name: m[2].trim(), body: [] };
      bodies.push(cur);
      continue;
    }
    if (cur) cur.body.push(l);
  }
  const anon = bodies.filter((b) => b.name === "" || b.name === "<anonymous>");
  const body = anon[anon.length - 1]?.body ?? [];
  const kinds = new Map();
  for (const l of body) {
    const k = /^\s+(?:v\d+: [^=]+ = )?([A-Z][A-Za-z]*)/.exec(l);
    if (k) kinds.set(k[1], (kinds.get(k[1]) ?? 0) + 1);
  }
  return { kinds, total: [...kinds.values()].reduce((a, b) => a + b, 0) };
}

const rows = [];
for (const f of readdirSync(BASE).filter((f) => f.endsWith(".ir.txt"))) {
  if (!existsSync(join(NOW, f))) {
    rows.push({ slug: f.replace(/\.ir\.txt$/, ""), missing: true });
    continue;
  }
  const a = caseInsts(join(BASE, f));
  const b = caseInsts(join(NOW, f));
  const kinds = new Set([...a.kinds.keys(), ...b.kinds.keys()]);
  const moved = [];
  for (const k of [...kinds].sort()) {
    const d = (b.kinds.get(k) ?? 0) - (a.kinds.get(k) ?? 0);
    if (d !== 0) moved.push(`${k} ${d > 0 ? "+" : ""}${d}`);
  }
  rows.push({ slug: f.replace(/\.ir\.txt$/, ""), before: a.total, after: b.total, moved });
}

const grew = rows.filter((r) => !r.missing && r.after > r.before);
const shrank = rows.filter((r) => !r.missing && r.after < r.before);
const same = rows.filter((r) => !r.missing && r.after === r.before);

rows.sort((a, b) => (a.after - a.before) - (b.after - b.before));
console.log("case".padEnd(30), "before".padStart(7), "after".padStart(7), "delta".padStart(7), " what moved");
console.log("-".repeat(120));
for (const r of rows) {
  if (r.missing) { console.log(r.slug.padEnd(30), " MISSING in the second dump"); continue; }
  const d = r.after - r.before;
  console.log(
    r.slug.padEnd(30),
    String(r.before).padStart(7),
    String(r.after).padStart(7),
    String(d > 0 ? "+" + d : d).padStart(7),
    " " + r.moved.join(", "),
  );
}
console.log("-".repeat(120));
console.log(`${shrank.length} cases emit less, ${grew.length} emit MORE, ${same.length} unchanged`);
if (grew.length) {
  console.log("GREW (read these first — a fold that adds instructions is a defect, not a win):");
  for (const r of grew) console.log(`  ${r.slug}: +${r.after - r.before} (${r.moved.join(", ")})`);
}
