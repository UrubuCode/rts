import { describe, test, expect } from "rts:test";

// O walker que reescreve `this` (`Raw("This(…)") → Ident("this")`) e o irmão que
// DETECTA `this` num corpo ainda-não-reescrito percorriam só um subconjunto dos
// statements: faltavam o INIT de um `for`, o `try`/`catch`/`finally`, o `switch`
// (discriminante e corpos de case) e o `labeled`. `this` em qualquer um deles
// sobrevivia como nó cru e a função INTEIRA falhava a compilar com
// `expression raw/unrecognized: This(ThisExpr)`.
//
// Medido num bundle real do WhatsApp Web. Forma mínima (do `g53`, um generator
// de coleta de samples):
//
//   const g = function*(e){ for(var r=this.f(e), i=0; i<1; ++i){} return e; };
//
// O rótulo inicial ("`this` em função levantada / generator") era inferência: o
// mesmo corpo SEM o `for` compilava, e a mesma falha reproduz numa function
// expression comum — não tem relação com o lifter nem com generators. O que
// falta é só a travessia de statements.
//
// O mesmo buraco atingia `collect_this_assign_fields`: um `this.x = …` dentro de
// try/switch/labeled no CONSTRUTOR não virava slot da instância.
//
// Valores conferidos contra o Node (v22). Pré-computado no top-level.

// ── `this` em cada posição de statement que faltava ─────────────────────────
const forInit = function (this: any) {
  for (var r = this.v, i = 0; i < 1; ++i) {}
  return r;
};
const tryBody = function (this: any) {
  try {
    var r = this.v;
  } catch (e) {}
  return r;
};
const catchBody = function (this: any) {
  try {
    throw 1;
  } catch (e) {
    var r = this.v;
  }
  return r;
};
const finallyBody = function (this: any) {
  try {
  } finally {
    var r = this.v;
  }
  return r;
};
const switchDisc = function (this: any) {
  switch (this.v) {
    case 1:
      return "um";
  }
  return "outro";
};
const switchCase = function (this: any, n: number) {
  switch (n) {
    case 1:
      var r = this.v;
      break;
  }
  return r;
};
const labeled = function (this: any) {
  lab: {
    var r = this.v;
  }
  return r;
};

const r_for = forInit.call({ v: 7 });
const r_try = tryBody.call({ v: 8 });
const r_catch = catchBody.call({ v: 9 });
const r_finally = finallyBody.call({ v: 10 });
const r_switchD = switchDisc.call({ v: 1 });
const r_switchC = switchCase.call({ v: 11 }, 1);
const r_labeled = labeled.call({ v: 12 });

// ── campos de instância atribuídos dentro de try/switch/labeled ─────────────
class Caixa {
  x: any;
  y: any;
  z: any;
  w: any;
  constructor() {
    try {
      this.x = 1;
    } catch (e) {}
    lab: {
      this.y = 2;
    }
    switch (0) {
      case 0:
        this.z = 3;
    }
    for (var i = 0; i < 1; i++) {
      this.w = 4;
    }
  }
}
const k = new Caixa();

// ── a forma do bundle: generator + `this` no init do `for` ──────────────────
// (via método de classe; `generatorExpr.call(recv)` não devolve iterador hoje —
// falha com `next is not a function`, defeito à parte, não tocado aqui.)
class Gerador {
  m: number = 3;
  *gen(e: number) {
    for (var r = this.m * e, i = 0; i < 1; ++i) {}
    yield r;
  }
}
const r_gen = new Gerador().gen(5).next().value;

describe("this em statements compostos", () => {
  test("init de for", () => {
    expect(r_for).toBe(7);
  });
  test("corpo de try", () => {
    expect(r_try).toBe(8);
  });
  test("corpo de catch", () => {
    expect(r_catch).toBe(9);
  });
  test("corpo de finally", () => {
    expect(r_finally).toBe(10);
  });
  test("discriminante de switch", () => {
    expect(r_switchD).toBe("um");
  });
  test("corpo de case", () => {
    expect(r_switchC).toBe(11);
  });
  test("bloco rotulado", () => {
    expect(r_labeled).toBe(12);
  });

  test("campo em try vira slot", () => {
    expect(k.x).toBe(1);
  });
  test("campo em bloco rotulado vira slot", () => {
    expect(k.y).toBe(2);
  });
  test("campo em case vira slot", () => {
    expect(k.z).toBe(3);
  });
  test("campo em for vira slot", () => {
    expect(k.w).toBe(4);
  });

  test("generator com this no init do for (forma do bundle)", () => {
    expect(r_gen).toBe(15);
  });
});
