// Reduce a whole-file `rts ir` dump to the ONE function the bench case is, and
// say what that function calls out to. A case's cost is either instructions it
// emits or entry points it calls; the summary below separates the two, which is
// the question "why is this row expensive" reduces to.

import { readFileSync, writeFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const IR = process.argv[2] ?? "target/bench-ir";

function parse(text) {
  const lines = text.split(/\r?\n/);
  const names = new Map(); // FuncId -> name
  let i = 0;
  for (; i < lines.length; i++) {
    if (lines[i].startsWith("; FuncId(")) break; // a body header, not the legend
    const m = /^;\s{2,}FuncId\((\d+)\)\s?(.*)$/.exec(lines[i]);
    if (m) names.set(+m[1], m[2].trim());
  }
  const funcs = [];
  let cur = null;
  for (; i < lines.length; i++) {
    const m = /^; FuncId\((\d+)\)\s?(.*)$/.exec(lines[i]);
    if (m) {
      cur = { id: +m[1], name: m[2].trim(), body: [] };
      funcs.push(cur);
      continue;
    }
    if (cur) cur.body.push(lines[i]);
  }
  return { names, funcs };
}

function summarise(fn, names) {
  const callees = new Map();
  const ops = new Map();
  for (const l of fn.body) {
    for (const m of l.matchAll(/callee: FuncId\((\d+)\)/g)) {
      const id = +m[1];
      callees.set(id, (callees.get(id) ?? 0) + 1);
    }
    // instruction kind: the token after `= ` or the statement head
    const k = /^\s+(?:v\d+: [^=]+ = )?([A-Z][A-Za-z]*)/.exec(l);
    if (k) ops.set(k[1], (ops.get(k[1]) ?? 0) + 1);
  }
  return { callees, ops };
}

const rows = [];
for (const f of readdirSync(IR).filter((f) => f.endsWith(".ir.txt"))) {
  const slug = f.replace(/\.ir\.txt$/, "");
  const { names, funcs } = parse(readFileSync(join(IR, f), "utf8"));
  // The case body is the LAST anonymous function: the preamble's helpers are all
  // named, and a callback written inside the case is emitted before the case
  // that closes over it only when it is hoisted, which none of them are.
  const anon = funcs.filter((x) => x.name === "" || x.name === "<anonymous>");
  const cse = anon[anon.length - 1];
  if (!cse) { rows.push({ slug, error: "no anonymous function" }); continue; }

  const { callees, ops } = summarise(cse, names);
  const callList = [...callees.entries()]
    .map(([id, n]) => ({ name: names.get(id) || `FuncId(${id})`, n }))
    .sort((a, b) => b.n - a.n);

  const out = [];
  out.push(`; case ${slug}`);
  out.push(`; ${cse.body.length} IR lines, ${[...ops.values()].reduce((a, b) => a + b, 0)} instructions`);
  out.push(";");
  out.push("; calls out to:");
  for (const c of callList) out.push(`;   ${String(c.n).padStart(3)}x ${c.name}`);
  out.push(";");
  out.push("; instruction mix:");
  for (const [k, n] of [...ops.entries()].sort((a, b) => b[1] - a[1])) out.push(`;   ${String(n).padStart(3)}x ${k}`);
  out.push("");
  out.push(...cse.body);
  writeFileSync(join(IR, `${slug}.case.txt`), out.join("\n"));

  rows.push({
    slug,
    lines: cse.body.length,
    instrs: [...ops.values()].reduce((a, b) => a + b, 0),
    guards: ops.get("Guard") ?? 0,
    cachedGets: (ops.get("CachedGet") ?? 0) + (ops.get("CachedGetIndirect") ?? 0),
    calls: [...callees.values()].reduce((a, b) => a + b, 0),
    callees: callList.map((c) => `${c.n}x${c.name}`),
  });
}

rows.sort((a, b) => (b.instrs ?? 0) - (a.instrs ?? 0));
writeFileSync(join(IR, "summary.json"), JSON.stringify(rows, null, 2) + "\n");
console.log("slug".padEnd(34), "instr".padStart(6), "calls".padStart(6), "guard".padStart(6), "cache".padStart(6), " top callees");
for (const r of rows) {
  if (r.error) { console.log(r.slug.padEnd(34), " ERROR " + r.error); continue; }
  console.log(
    r.slug.padEnd(34),
    String(r.instrs).padStart(6),
    String(r.calls).padStart(6),
    String(r.guards).padStart(6),
    String(r.cachedGets).padStart(6),
    " " + r.callees.slice(0, 4).join(" "),
  );
}
