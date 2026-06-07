// Cross-runtime: array de closures capturando `let` de loop (binding por-iteração).
// Bug RTS — cada closure deveria capturar o `i` da SUA iteração (ES6 let cria um
// binding novo por volta do loop). O RTS captura por referência compartilhada,
// devolvendo o valor final (3,3,3) em vez de 0,1,2. Relaciona-se a #195
// (mutable closures / env-record). Bun/Node: 0,1,2.
const fns: (() => number)[] = [];
for (let i = 0; i < 3; i++) {
  fns.push(() => i);
}
console.log(fns[0]() + "," + fns[1]() + "," + fns[2]());

// variante com const dentro do corpo (snapshot explícito) — deve dar 0,1,2 nos 3
const gns: (() => number)[] = [];
for (let j = 0; j < 3; j++) {
  const k = j;
  gns.push(() => k);
}
console.log(gns[0]() + "," + gns[1]() + "," + gns[2]());
