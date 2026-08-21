// O gemeo em JavaScript de `property_access.ts`, para os runtimes que a matriz
// do `benchmark.ps1` compara contra o RTS.
//
// # Porque existe, em vez de os runners lerem o `.ts` direto
//
// O Bun e o Deno leem TypeScript; o Node so' a partir da v22, e o workflow
// fixa `node-version: "20"`. Um `.ts` entregue ao Node 20 falha em
// milissegundos — e `benchmark.ps1` redireciona a saida dos runners para
// `$null`, entao esse fracasso nao aparece como fracasso: aparece como um
// tempo minusculo, e o Node seria publicado na tabela do README como o
// runtime mais rapido por nao ter corrido o programa.
//
// Nenhuma logica muda daqui para o `.ts` — o que sai sao as anotacoes de tipo,
// e nada mais. Os dois tem de responder o mesmo checksum, que e' o que faz
// deles o mesmo programa.

const N = 3000000;

function bench_local() {
  let s = 1;
  let i = 0;
  while (i < N) {
    s = (s * 1664525 + 1013904223) % 4294967296;
    i = i + 1;
  }
  return s;
}

let captured_state = 1;
function peek() {
  return captured_state;
}
function bench_captured() {
  captured_state = 1;
  let i = 0;
  while (i < N) {
    captured_state = (captured_state * 1664525 + 1013904223) % 4294967296;
    i = i + 1;
  }
  return peek();
}

function bench_property() {
  const o = { s: 1 };
  let i = 0;
  while (i < N) {
    o.s = (o.s * 1664525 + 1013904223) % 4294967296;
    i = i + 1;
  }
  return o.s;
}

function bench_two_hops() {
  const outer = { inner: { s: 1 } };
  let i = 0;
  while (i < N) {
    outer.inner.s = (outer.inner.s * 1664525 + 1013904223) % 4294967296;
    i = i + 1;
  }
  return outer.inner.s;
}

function timed(label, run) {
  const started = performance.now();
  const answer = run();
  const elapsed = performance.now() - started;
  console.log(label + " " + elapsed.toFixed(1) + " ms  (" + answer + ")");
}

const expected = bench_local();
for (const [label, run] of [
  ["local      ", bench_local],
  ["capturada  ", bench_captured],
  ["propriedade", bench_property],
  ["duas hops  ", bench_two_hops],
]) {
  timed(label, run);
}
console.log("checksum " + expected);
