// Agrega o TSV do `run.sh` numa tabela por modulo, numa lista de trabalho, e
// num JSON.
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
// So os modulos com mais de dois ficheiros na tabela larga: um modulo de um
// ficheiro so responde 0% ou 100% e enche a tabela de linhas que nao dizem
// nada. Os outros continuam no JSON e no total.
for (const r of table) if (r.ran > 2) console.log(line(r.mod, r));
console.log('-'.repeat(52));
console.log(line('TOTAL', { ...total, pct: pct(total) }));
console.log('');
console.log(`${total.ok} de ${ran(total)} ficheiros saem com 0.`);

// A LISTA DE TRABALHO. O que interessa numa percentagem baixa nao e a
// percentagem, e quais sao as poucas causas por tras de muitos ficheiros: uma
// mensagem que aparece 200 vezes e um nome em falta, nao duzentos problemas.
//
// Agrupado pela mensagem com os numeros e as aspas retirados, porque
// `x is not a function` e `y is not a function` sao a mesma causa com dois
// nomes — e essa e a fatia que uma correcao apaga de uma vez.
const shape = (detail) =>
  detail
    .replace(/^rts: uncaught exception \(tag \d+\): /, '')
    .replace(/"[^"]*"/g, '"…"')
    .replace(/'[^']*'/g, "'…'")
    .replace(/\b\d+\b/g, 'N')
    .trim()
    .slice(0, 90);

const causes = new Map();
for (const r of rows) {
  if (r.status === 'ok' || !r.detail) continue;
  const key = shape(r.detail);
  if (!causes.has(key)) causes.set(key, { count: 0, example: r.name });
  causes.get(key).count += 1;
}
const ranked = [...causes.entries()].sort((a, b) => b[1].count - a[1].count);
if (ranked.length) {
  console.log('');
  console.log('as causas mais frequentes (ficheiros, mensagem, um exemplo):');
  for (const [message, { count, example }] of ranked.slice(0, 15)) {
    console.log(`  ${num(count, 5)}  ${message}`);
    console.log(`         ex: ${example}`);
  }
  console.log(`  (${ranked.length} formas distintas ao todo; todas no JSON)`);
}

writeFileSync(
  out,
  JSON.stringify(
    {
      total: { ...total, ran: ran(total), pct: pct(total) },
      modules: table,
      causes: ranked.map(([message, held]) => ({ message, ...held })),
      files: rows,
    },
    null,
    2,
  ),
);
console.log('');
console.log(`relatorio: ${out}`);
