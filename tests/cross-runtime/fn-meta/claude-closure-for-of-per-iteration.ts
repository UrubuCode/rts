// Cross-runtime: UMA coisa — `let`/`const` em `for...of` cria um binding NOVO
// por iteração, então closures criadas dentro do corpo capturam o valor DAQUELA
// volta. Distinto de claude-let-vs-var-loop-closures (aquele é `for(;;)` clássico,
// onde o binding-por-iteração vem da cópia do loop; aqui a fonte do binding é o
// protocolo de iteração). Variações: const, let mutado no corpo, destructuring
// no cabeçalho, for-of aninhado, var (um único binding compartilhado).

// 1) const no cabeçalho: um binding por elemento
const a: Array<() => string> = [];
for (const ch of ["x", "y", "z"]) {
  a.push(() => ch);
}
console.log("const_head=" + a[0]() + a[1]() + a[2]());

// 2) let no cabeçalho, MUTADO dentro do corpo: a mutação afeta só o binding da volta
const b: Array<() => number> = [];
for (let n of [1, 2, 3]) {
  n = n * 10;
  b.push(() => n);
}
console.log("let_head_mutated=" + b[0]() + "," + b[1]() + "," + b[2]());

// 3) var no cabeçalho: UM binding só, function-scoped — todas veem o último
const c: Array<() => number> = [];
for (var v of [1, 2, 3]) {
  c.push(() => v);
}
console.log("var_head=" + c[0]() + "," + c[1]() + "," + c[2]());

// 4) destructuring no cabeçalho: cada parte é um binding por-iteração
const d: Array<() => string> = [];
for (const [k, val] of [["a", 1], ["b", 2]] as Array<[string, number]>) {
  d.push(() => k + val);
}
console.log("destructure_head=" + d[0]() + "," + d[1]());

// 5) let extra declarado no corpo: binding por-iteração independente do cabeçalho
const e: Array<() => string> = [];
for (const p of [1, 2]) {
  let tag = "p" + p;
  tag = tag + "!";
  e.push(() => tag);
}
console.log("body_let=" + e[0]() + "," + e[1]());

// 6) for-of aninhado: closure captura os bindings dos DOIS níveis
const f: Array<() => string> = [];
for (const i of [1, 2]) {
  for (const j of ["a", "b"]) {
    f.push(() => i + j);
  }
}
console.log("nested=" + f[0]() + "," + f[1]() + "," + f[2]() + "," + f[3]());

// 7) closure criada numa volta observa mutação feita em volta POSTERIOR? Não —
// bindings distintos. Guardamos a closure e o setter da mesma volta.
const getters: Array<() => number> = [];
const setters: Array<(x: number) => void> = [];
for (const seed of [10, 20]) {
  let cell = seed;
  getters.push(() => cell);
  setters.push((x: number) => {
    cell = x;
  });
}
setters[0](111);
console.log("independent_cells=" + getters[0]() + "," + getters[1]());
