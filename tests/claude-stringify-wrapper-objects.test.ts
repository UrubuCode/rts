import { describe, test, expect } from "rts:test";

// `JSON.stringify(new Number(5))` rendia `{}` em vez de `5`.
//
// Pela spec (SerializeJSONProperty), um wrapper Boolean/Number/String serializa
// como o PRIMITIVO embrulhado, ignorando propriedades expando. O prelude já fazia
// isso — mas só reconhecia UMA das duas formas de wrapper do motor:
//
//   Object(5)      → objeto de shape `{ __prim: 5 }`  (rts-primitives/object.ts)
//   new Number(5)  → instância de classe cujo primitivo sai por `valueOf()`
//
// O `.ts` checava apenas `__prim`, então a segunda forma caía no caminho de
// objeto comum e virava `{}`. A correção acrescenta o teste por `instanceof` +
// `valueOf()` — ambos já funcionavam —, mantendo a checagem de `__prim` para a
// primeira forma. Corrigido no prelude, não no motor: é semântica de JSON.
//
// Valores conferidos contra o Node. Pré-computado no top-level.

const numWrapper = JSON.stringify(new Number(5));
const numNaN = JSON.stringify(new Number(NaN));
const numInfinity = JSON.stringify(new Number(Infinity));
const strWrapper = JSON.stringify(new String("hi"));
const strComAspas = JSON.stringify(new String('a"b'));
const boolTrue = JSON.stringify(new Boolean(true));
const boolFalse = JSON.stringify(new Boolean(false));

const aninhado = JSON.stringify({ a: new Number(7) });
const emArray = JSON.stringify([new Number(1), new String("x"), new Boolean(false)]);
const profundo = JSON.stringify({ a: { b: [new Number(3)] } });

// a outra forma de wrapper não pode regredir
const viaObject = JSON.stringify(Object(5));
const viaObjectStr = JSON.stringify(Object("h"));

// objeto comum continua objeto
const objComum = JSON.stringify({ a: 1 });
const objComValueOf = JSON.stringify({ valueOf: () => 9, a: 1 });
const arrayComum = JSON.stringify([1, 2]);

// primitivos não regridem
const primNum = JSON.stringify(5);
const primStr = JSON.stringify("hi");
const primBool = JSON.stringify(true);
const primNull = JSON.stringify(null);

describe("JSON.stringify desembrulha wrapper objects", () => {
  test("new Number(5)", () => {
    expect(numWrapper).toBe("5");
  });

  test("new Number(NaN) vira null", () => {
    expect(numNaN).toBe("null");
  });

  test("new Number(Infinity) vira null", () => {
    expect(numInfinity).toBe("null");
  });

  test("new String('hi')", () => {
    expect(strWrapper).toBe('"hi"');
  });

  test("new String com aspas é escapado", () => {
    expect(strComAspas).toBe('"a\\"b"');
  });

  test("new Boolean(true)", () => {
    expect(boolTrue).toBe("true");
  });

  test("new Boolean(false)", () => {
    expect(boolFalse).toBe("false");
  });

  test("wrapper aninhado em objeto", () => {
    expect(aninhado).toBe('{"a":7}');
  });

  test("wrappers em array", () => {
    expect(emArray).toBe('[1,"x",false]');
  });

  test("wrapper em profundidade", () => {
    expect(profundo).toBe('{"a":{"b":[3]}}');
  });
});

describe("não-regressões do stringify", () => {
  test("Object(5) — a outra forma de wrapper", () => {
    expect(viaObject).toBe("5");
  });

  test("Object('h') — a outra forma de wrapper", () => {
    expect(viaObjectStr).toBe('"h"');
  });

  test("objeto comum continua objeto", () => {
    expect(objComum).toBe('{"a":1}');
  });

  test("objeto com valueOf próprio NÃO é desembrulhado", () => {
    expect(objComValueOf).toBe('{"a":1}');
  });

  test("array comum", () => {
    expect(arrayComum).toBe("[1,2]");
  });

  test("primitivo number", () => {
    expect(primNum).toBe("5");
  });

  test("primitivo string", () => {
    expect(primStr).toBe('"hi"');
  });

  test("primitivo boolean", () => {
    expect(primBool).toBe("true");
  });

  test("null", () => {
    expect(primNull).toBe("null");
  });
});
