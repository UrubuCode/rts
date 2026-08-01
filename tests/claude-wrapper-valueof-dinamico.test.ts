import { describe, test, expect } from "rts:test";

// `valueOf()` / `toString()` sobre um WRAPPER primordial (`new Number(5)`,
// `new String("hi")`, `new Boolean(true)`) alcançado por um valor DINÂMICO —
// parâmetro, retorno de função, elemento de array, propriedade de objeto.
//
// Por variável local o front prova a classe e roteia direto; por valor dinâmico
// caía em dois buracos, ambos corrigidos:
//
//  1. `try_registry_virtual_dynamic` (front) emitia, para cada classe do
//     Registry com `instanceof_predicate` que declara o método, um braço
//     guardado — e o fall-through era o sentinela `undefined`. Como `Date`
//     declara `valueOf`, QUALQUER outro receptor lia `undefined`: uma classe do
//     Registry declarando o nome bastava para sombrear o método de todo o resto.
//     Agora o fall-through é o despacho genérico pela cadeia de protótipos, que
//     é exatamente o que rodaria se nenhuma classe do Registry declarasse o nome.
//
//  2. `ToPrimitive` (runtime) só andava sobre objetos COM SLOTS. Um wrapper de
//     value-class é um `Entry::Rtse` opaco cujos `valueOf`/`toString` moram no
//     Registry, não em slots — então toda coerção caía em `[object Object]`.
//     Agora OrdinaryToPrimitive tenta primeiro os membros registrados da classe
//     que o VALOR carrega (despacho por dado, nenhum nome de classe no motor).
//
// Todos os valores conferidos contra o Node. Pré-computado no top-level.

function vo(v: any): any {
  return v.valueOf();
}
function ts(v: any): any {
  return v.toString();
}
function str(v: any): any {
  return String(v);
}
function mkNum(n: any): any {
  return new Number(n);
}

// --- por PARÂMETRO --------------------------------------------------------
const voNum = vo(new Number(5));
const voStr = vo(new String("hi"));
const voBool = vo(new Boolean(true));
const voBoolF = vo(new Boolean(false));
const tsNum = ts(new Number(5));
const tsStr = ts(new String("hi"));
const tsBool = ts(new Boolean(true));
const tsNaN = ts(new Number(NaN));

// --- por RETORNO de função ------------------------------------------------
const voRet = vo(mkNum(7));
const tsRet = ts(mkNum(7));

// --- dentro de ARRAY ------------------------------------------------------
const arr: any[] = [new Number(1), new String("x"), new Boolean(false)];
const voArr0 = vo(arr[0]);
const voArr1 = vo(arr[1]);
const voArr2 = vo(arr[2]);
const tsArr0 = ts(arr[0]);

// --- como PROPRIEDADE de objeto ------------------------------------------
const obj: any = { n: new Number(42), s: new String("zz"), b: new Boolean(true) };
const voProp = vo(obj.n);
const voPropS = vo(obj.s);
const voPropB = vo(obj.b);
const tsProp = ts(obj.n);

// --- coerções que passam pelo mesmo ToPrimitive ---------------------------
const strOfParam = str(new Number(5));
const strOfParamS = str(new String("hi"));
const plus = (function (v: any) {
  return v + 1;
})(new Number(5));
const concat = (function (v: any) {
  return v + "!";
})(new String("hi"));
const numOf = (function (v: any) {
  return Number(v);
})(new Number(5));

// --- não-regressões -------------------------------------------------------
const typeofP = (function (v: any) {
  return typeof v;
})(new Number(5));
const instOf = (function (v: any) {
  return v instanceof Number;
})(new Number(5));
// o braço do Registry (Date declara `valueOf`) continua ganhando no seu receptor
const dateVo = (function (v: any) {
  return typeof v.valueOf();
})(new Date(0));
const dateVoVal = (function (v: any) {
  return v.valueOf();
})(new Date(0));
// valor local (caminho estático provado) não regride
const nLocal = new Number(5);
const voLocal = nLocal.valueOf();
const tsLocal = nLocal.toString();

describe("valueOf/toString de wrapper por parâmetro", () => {
  test("new Number(5).valueOf()", () => {
    expect(voNum).toBe(5);
  });

  test("new String('hi').valueOf()", () => {
    expect(voStr).toBe("hi");
  });

  test("new Boolean(true).valueOf()", () => {
    expect(voBool).toBe(true);
  });

  test("new Boolean(false).valueOf()", () => {
    expect(voBoolF).toBe(false);
  });

  test("new Number(5).toString()", () => {
    expect(tsNum).toBe("5");
  });

  test("new String('hi').toString()", () => {
    expect(tsStr).toBe("hi");
  });

  test("new Boolean(true).toString()", () => {
    expect(tsBool).toBe("true");
  });

  test("new Number(NaN).toString()", () => {
    expect(tsNaN).toBe("NaN");
  });
});

describe("valueOf/toString de wrapper por retorno de função", () => {
  test("valueOf", () => {
    expect(voRet).toBe(7);
  });

  test("toString", () => {
    expect(tsRet).toBe("7");
  });
});

describe("valueOf/toString de wrapper dentro de array", () => {
  test("Number no índice 0", () => {
    expect(voArr0).toBe(1);
  });

  test("String no índice 1", () => {
    expect(voArr1).toBe("x");
  });

  test("Boolean no índice 2", () => {
    expect(voArr2).toBe(false);
  });

  test("toString do índice 0", () => {
    expect(tsArr0).toBe("1");
  });
});

describe("valueOf/toString de wrapper como propriedade", () => {
  test("Number", () => {
    expect(voProp).toBe(42);
  });

  test("String", () => {
    expect(voPropS).toBe("zz");
  });

  test("Boolean", () => {
    expect(voPropB).toBe(true);
  });

  test("toString", () => {
    expect(tsProp).toBe("42");
  });
});

describe("coerções de wrapper dinâmico", () => {
  test("String(new Number(5))", () => {
    expect(strOfParam).toBe("5");
  });

  test("String(new String('hi'))", () => {
    expect(strOfParamS).toBe("hi");
  });

  test("new Number(5) + 1", () => {
    expect(plus).toBe(6);
  });

  test("new String('hi') + '!'", () => {
    expect(concat).toBe("hi!");
  });

  test("Number(new Number(5))", () => {
    expect(numOf).toBe(5);
  });
});

describe("não-regressões do despacho dinâmico", () => {
  test("typeof continua 'object'", () => {
    expect(typeofP).toBe("object");
  });

  test("instanceof Number continua true", () => {
    expect(instOf).toBe(true);
  });

  test("Date.valueOf dinâmico continua numérico", () => {
    expect(dateVo).toBe("number");
  });

  test("new Date(0).valueOf() === 0", () => {
    expect(dateVoVal).toBe(0);
  });

  test("valueOf por variável local não regride", () => {
    expect(voLocal).toBe(5);
  });

  test("toString por variável local não regride", () => {
    expect(tsLocal).toBe("5");
  });
});
