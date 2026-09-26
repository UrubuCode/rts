// Writes the `.esperado.json` of NEW fixtures from a measurement taken by
// `css_fixtures_medir_edge.mjs`, after validating the INSTRUMENT: every
// fixture that already has an expected file is re-compared against the fresh
// measurement, and the worst deviation is printed. A deviation above zero
// means the browser changed under the ruler — stop and look before trusting
// any new number (`tests/css/README.md`).
//
//   bun scripts/css_fixtures_medir_edge.mjs medidas.json
//   bun scripts/css_fixtures_escrever_esperado.mjs medidas.json claude-x.html [claude-y.html …]
//
// It NEVER rewrites an expected file that exists: a number that fails stays
// failing until the engine is fixed, which is the rule of this corpus.
import { readFileSync, writeFileSync, existsSync } from "node:fs";

const [medidasPath, ...novas] = process.argv.slice(2);
if (!medidasPath) { console.error("usage: css_fixtures_escrever_esperado.mjs medidas.json [fixture.html …]"); process.exit(2); }
const m = JSON.parse(readFileSync(medidasPath, "utf8"));
const medidas = m.medidas ?? m;
const esperadoDe = (nome) => `tests/css/${nome.replace(/\.html$/, "")}.esperado.json`;

let numeros = 0, pior = 0, piorOnde = "";
for (const [nome, med] of Object.entries(medidas)) {
  const p = esperadoDe(nome);
  if (!existsSync(p) || novas.includes(nome)) continue;
  const esp = JSON.parse(readFileSync(p, "utf8"));
  for (const [id, e] of Object.entries(esp.elementos ?? {})) {
    const n = med.elementos?.[id];
    if (!n) { console.log(`missing in the measurement: ${nome} #${id}`); continue; }
    e.rect.forEach((v, i) => { numeros++; const d = Math.abs(v - n.rect[i]); if (d > pior) { pior = d; piorOnde = `${nome}#${id}`; } });
  }
}
console.log(`instrument: ${numeros} numbers re-measured, worst deviation ${pior} ${piorOnde}`);
if (pior > 0.011) { console.error("the instrument moved — not writing anything"); process.exit(1); }

const versao = (m.ua ?? "").match(/Edg\/(\d+)/)?.[1] ?? (m.ua ?? "").match(/Chrome\/(\d+)/)?.[1] ?? "?";
for (const nome of novas) {
  const p = esperadoDe(nome);
  if (existsSync(p)) { console.log(`EXISTS, not rewritten: ${p}`); continue; }
  const med = medidas[nome];
  if (!med) { console.log(`not measured: ${nome}`); continue; }
  const out = {
    fixture: nome,
    regua: `Edge ${versao} headless (Blink) por CDP, num iframe de 1280x800`,
    viewport: [1280, 800],
    medido_em: new Date().toISOString().slice(0, 10),
    elementos: med.elementos,
  };
  writeFileSync(p, JSON.stringify(out, null, 1) + "\n");
  console.log(`written ${p}`);
  for (const [id, e] of Object.entries(med.elementos)) console.log(`   ${id} ${JSON.stringify(e.rect)}`);
}
