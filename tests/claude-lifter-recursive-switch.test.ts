import { describe, test, expect } from "rts:test";

// Rename do self-nome de função recursiva LEVANTADA que se chama dentro de um
// `switch` (e de um `labeled`).
//
// Quando uma `function z` aninhada CAPTURA uma variável externa, o lifter a
// levanta para o topo e renomeia `z` → `__rtsn_arrow_N`; as chamadas recursivas
// no corpo precisam ser renomeadas junto, senão viram nome não-ligado. O
// `rename_ident_stmt` traversava if/while/for/try/block mas tinha um `_ => {}`
// que engolia `HirStmt::Switch` e `HirStmt::Labeled` — a mesma armadilha de
// match-em-vez-de-visitor que já escondeu um gap do detector de generator.
//
// Sintoma real: o `mapIntoArray` do React (uma `function z(t,n,r,o,a)` recursiva
// com `case h: return z(...)` num switch) balava "call to unknown function `z`"
// ao ser levantada, derrubando o módulo React inteiro na carga de uma página.
//
// Valores conferidos contra o Node.

// z captura `cap` (força o lift) e recorre DENTRO de um switch case.
function comSwitch(cap: number): number {
  function z(t: number): number {
    switch (t) {
      case 0: return cap;
      case 1: return z(0);
      case 2: return z(1) + z(0);
    }
    return t + cap;
  }
  return z(2);
}

// recursão dentro de um switch ANINHADO em outro switch (como no React:
// switch(typeof t){ case "object": switch(t.$$typeof){ case h: return z(..) } })
function switchAninhado(cap: number): number {
  function z(t: number, k: number): number {
    switch (t) {
      case 0:
        switch (k) {
          case 9: return cap;
          case 8: return z(0, 9);
        }
        return k;
    }
    return t;
  }
  return z(0, 8);
}

// recursão dentro de um laço ROTULADO.
function comLabeled(cap: number): number {
  function z(n: number): number {
    let acc = 0;
    L: for (let i = 0; i < n; i = i + 1) {
      if (i === 0 && n > 1) { acc = acc + z(1); continue L; }
      acc = acc + cap;
    }
    return acc;
  }
  return z(3);
}

const vSwitch = comSwitch(10);          // z(2)=z(1)+z(0)=(cap)+(cap)=20
const vAninhado = switchAninhado(7);    // z(0,8)->z(0,9)->cap=7
const vLabeled = comLabeled(10);        // n=3: i0 -> z(1)=10 ; i1,i2 -> +10 +10 = 30

describe("lifter: self-nome recursivo em switch/labeled", () => {
  test("recursão dentro de switch case", () => {
    expect(vSwitch).toBe(20);
  });
  test("recursão dentro de switch aninhado", () => {
    expect(vAninhado).toBe(7);
  });
  test("recursão dentro de laço rotulado", () => {
    expect(vLabeled).toBe(30);
  });
});
