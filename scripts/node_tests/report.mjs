// Agrega o TSV do `run.sh` numa tabela por modulo e num JSON.
//
// Corre em `node` e nao no `rts`, de proposito: o instrumento que mede o motor
// nao e o motor. Um relatorio produzido pela coisa medida pode falhar da mesma
// maneira que ela e reportar um numero que parece bom.
import { readFileSync, writeFileSync } from 'node:fs';

const [, , tsv, out] = process.argv;
const rows = readFileSync(tsv, 'utf8')
  .split('\n')
  .filter(Boolean)
  .map((line) => {
    const [mod, name, status, detail = ''] = line.split('\t');
    return { mod, name, status, detail };
  });

const KINDS = ['ok', 'fail', 'error', 'timeout'];
const zero = () => Object.fromEntries(KINDS.map((k) => [k, 0]));
const byMod = new Map();
for (const r of rows) {
  if (!byMod.has(r.mod)) byMod.set(r.mod, zero());
  if (r.status in byMod.get(r.mod)) byMod.get(r.mod)[r.status] += 1;
}

const ran = (c) => KINDS.reduce((n, k) => n + c[k], 0);
const pct = (c) => (ran(c) ? (100 * c.ok) / ran(c) : 0);

const table = [...byMod.entries()]
  .map(([mod, c]) => ({ mod, ...c, ran: ran(c), pct: pct(c) }))
  .sort((a, b) => b.ran - a.ran || a.mod.localeCompare(b.mod));

const total = zero();
for (const r of rows) if (r.status in total) total[r.status] += 1;

const pad = (s, n) => String(s).padEnd(n);
const num = (s, n) => String(s).padStart(n);
const line = (name, c) =>
  pad(name, 18) + num(c.pct.toFixed(1), 7) + num(c.ok, 7) +
  num(c.fail, 7) + num(c.error, 7) + num(c.timeout, 6);

console.log('');
console.log(`${pad('modulo', 18)}${num('%', 7)}${num('ok', 7)}${num('fail', 7)}${num('erro', 7)}${num('t/o', 6)}`);
console.log('-'.repeat(52));
for (const r of table) console.log(line(r.mod, r));
console.log('-'.repeat(52));
console.log(line('TOTAL', { ...total, pct: pct(total) }));
console.log('');
console.log(`${total.ok} de ${ran(total)} ficheiros saem com 0.`);

writeFileSync(
  out,
  JSON.stringify({ total: { ...total, ran: ran(total), pct: pct(total) }, modules: table, files: rows }, null, 2),
);
console.log(`relatorio: ${out}`);
