// Cross-runtime: função recursiva LEVANTADA que captura uma variável externa e
// se chama recursivamente dentro de switch / switch aninhado / laço rotulado /
// do-while. Foi o bail "call to unknown function `z`" e o subsequente
// "ReferenceError: cap is not defined" do React mapIntoArray (issue #2071):
// o lifter não traversava switch/labeled/do-while ao renomear o self-nome nem
// ao coletar capturas.

// recursão dentro de switch case, capturando `cap`
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

// switch ANINHADO, `cap` no case interno
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

// laço ROTULADO com recursão e captura
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

// do-while com recursão e captura (o gap que só apareceu ao tornar o match exaustivo)
function comDoWhile(cap: number): number {
  function z(n: number): number {
    let s = 0;
    let i = 0;
    do {
      if (i === 0 && n > 1) { s = s + z(1); }
      else { s = s + cap; }
      i = i + 1;
    } while (i < n);
    return s;
  }
  return z(2);
}

console.log("switch=" + comSwitch(10));
console.log("aninhado=" + switchAninhado(7));
console.log("labeled=" + comLabeled(10));
console.log("dowhile=" + comDoWhile(10));

// caso combinado: switch que retorna uma closure recursiva capturando
function build(base: number): (n: number) => number {
  function fib(n: number): number {
    switch (n) {
      case 0: return base;
      case 1: return base + 1;
    }
    return fib(n - 1) + fib(n - 2) - base;
  }
  return fib;
}
const f = build(0);
console.log("fib5=" + f(5));
