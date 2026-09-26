// Compares two sweeps of `wpt_css_todas.mjs` PER FILE — the only form the
// claim "no regression" takes in this repository (CLAUDE.md, "regress
// explicitly"): a net `+3` is equally three gained and five gained against two
// lost.
//
//   bun scripts/wpt_comparar.mjs <base-out-dir> <new-out-dir>
//
// Prints, per folder, passes before -> after; then LOST (passed, no longer
// passes), GAINED, and every other change of state or of more than half a
// point of differing pixels. `passa-vazio` is NOT a pass (wpt_reftests.md): a
// test that goes from pass to pass-empty is listed as lost, and one that goes
// from pass-empty to pass as gained.
import { readFileSync, readdirSync, existsSync, statSync } from "node:fs";
import { join } from "node:path";

const [base, novo] = process.argv.slice(2);
if (!base || !novo) { console.error("usage: wpt_comparar.mjs <base-out-dir> <new-out-dir>"); process.exit(2); }

function relatorios(dir, pre = "") {
  const out = {};
  for (const n of readdirSync(dir)) {
    const p = join(dir, n);
    if (!statSync(p).isDirectory()) continue;
    const r = join(p, "relatorio.json");
    if (existsSync(r)) out[pre + n] = JSON.parse(readFileSync(r, "utf8"));
    Object.assign(out, relatorios(p, pre + n + "/"));
  }
  return out;
}

const A = relatorios(base), B = relatorios(novo);
const total = [0, 0], perdidos = [], ganhos = [], outros = [];
let incomparavel = false;
for (const pasta of Object.keys(A).sort()) {
  const a = A[pasta], b = B[pasta];
  if (!b) { console.log(`MISSING in the new sweep: ${pasta}`); incomparavel = true; continue; }
  // A corpus that changed size is not the same ruler — say so rather than
  // report a difference that is the denominator's.
  if (a.total !== b.total) { console.log(`!! total differs in ${pasta}: ${a.total} vs ${b.total}`); incomparavel = true; }
  const estados = (r) => {
    const m = new Map(r.resultados.map((x) => [x.nome, x]));
    for (const n of r.nao_rasterizaram ?? []) m.set(n, { estado: "nao-rasterizou" });
    return m;
  };
  const ea = estados(a), eb = estados(b);
  total[0] += a.passam; total[1] += b.passam;
  console.log(`${pasta.padEnd(30)} ${String(a.passam).padStart(5)} -> ${String(b.passam).padStart(5)}  (total ${a.total})`);
  for (const [n, ra] of ea) {
    const rb = eb.get(n) ?? { estado: "ausente" };
    if (ra.estado === rb.estado) {
      if (ra.estado !== "passa" && ra.pct != null && rb.pct != null && Math.abs(ra.pct - rb.pct) > 0.5)
        outros.push(`${pasta}/${n}: ${ra.estado} ${ra.pct.toFixed(2)}% -> ${rb.pct.toFixed(2)}%`);
      continue;
    }
    const linha = `${pasta}/${n}: ${ra.estado} -> ${rb.estado}` + (rb.pct != null ? ` (${rb.pct.toFixed(2)}%)` : "");
    if (ra.estado === "passa") perdidos.push(linha);
    else if (rb.estado === "passa") ganhos.push(linha);
    else outros.push(linha);
  }
}
console.log(`\nTOTAL pass: ${total[0]} -> ${total[1]}`);
for (const [titulo, lista] of [["LOST", perdidos], ["GAINED", ganhos], ["OTHER CHANGES", outros]]) {
  console.log(`\n${titulo} (${lista.length}):`);
  lista.forEach((l) => console.log("  " + l));
}
process.exit(incomparavel ? 3 : perdidos.length ? 1 : 0);
