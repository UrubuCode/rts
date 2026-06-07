// Cross-runtime: funções aninhadas com o MESMO nome em pais diferentes.
// Bug RTS #1357 — o hoisting de funções aninhadas (hoist_fn.rs) sobe cada
// `function helper()` ao top-level mantendo o nome original, colidindo:
// "Duplicate definition of identifier __RTS_USER_helper". Bun/Node escopam
// cada helper ao seu pai. Esperado: a=1 / b=2.
function a(): void {
  function helper(): number {
    return 1;
  }
  console.log("a=" + helper());
}
function b(): void {
  function helper(): number {
    return 2;
  }
  console.log("b=" + helper());
}
a();
b();
