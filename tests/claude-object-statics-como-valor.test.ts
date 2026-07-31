import { describe, test, expect } from "rts:test";

// Ler um estático de `Object` como VALOR (`const d = Object.defineProperty`)
// bailava com "no such static field on class `Object`", embora a forma CHAMADA
// (`Object.defineProperty(o, k, desc)`) já funcionasse.
//
// O leitor genérico de estático de classe procura em `desc.statics`, populado
// pelos shims de `rts-primitives/src/object.ts`. Metade da superfície não tinha
// shim — então só metade era legível. Cada shim delega ao MESMO caminho nativo;
// nada é reimplementado.
//
// Importa porque bundle minificado guarda esses estáticos em variável o tempo
// todo (`var d = Object.defineProperty`).
//
// Valores conferidos contra o Node. Pré-computado no top-level.

const tDefineProperty = typeof Object.defineProperty;
const tDefineProperties = typeof Object.defineProperties;
const tFromEntries = typeof Object.fromEntries;
const tCreate = typeof Object.create;
const tSetPrototypeOf = typeof Object.setPrototypeOf;
const tGetOwnPropertyDescriptor = typeof Object.getOwnPropertyDescriptor;
const tGetOwnPropertySymbols = typeof Object.getOwnPropertySymbols;
const tPreventExtensions = typeof Object.preventExtensions;
const tIsExtensible = typeof Object.isExtensible;

// não basta ser "function": tem de ser CHAMÁVEL e fazer a coisa certa
const dp = Object.defineProperty;
const alvo: any = {};
dp(alvo, "a", { value: 7 });
const usado = alvo.a;

const fe = Object.fromEntries;
const doFromEntries = JSON.stringify(fe([["k", 1]]));

const ge = Object.getOwnPropertyDescriptor;
const desc: any = ge({ a: 5 }, "a");
const doDescriptor = desc.value;

// os que já funcionavam não podem regredir
const tKeys = typeof Object.keys;
const tAssign = typeof Object.assign;
const keysUsado = (Object.keys)({ x: 1, y: 2 }).join(",");

describe("estáticos de Object legíveis como valor", () => {
  test("defineProperty", () => expect(tDefineProperty).toBe("function"));
  test("defineProperties", () => expect(tDefineProperties).toBe("function"));
  test("fromEntries", () => expect(tFromEntries).toBe("function"));
  test("create", () => expect(tCreate).toBe("function"));
  test("setPrototypeOf", () => expect(tSetPrototypeOf).toBe("function"));
  test("getOwnPropertyDescriptor", () => expect(tGetOwnPropertyDescriptor).toBe("function"));
  test("getOwnPropertySymbols", () => expect(tGetOwnPropertySymbols).toBe("function"));
  test("preventExtensions", () => expect(tPreventExtensions).toBe("function"));
  test("isExtensible", () => expect(tIsExtensible).toBe("function"));
});

describe("o valor lido é realmente chamável", () => {
  test("defineProperty guardado define a propriedade", () => expect(usado).toBe(7));
  test("fromEntries guardado constrói o objeto", () => expect(doFromEntries).toBe('{"k":1}'));
  test("getOwnPropertyDescriptor guardado lê o descritor", () => expect(doDescriptor).toBe(5));
});

describe("não-regressões", () => {
  test("keys continua legível", () => expect(tKeys).toBe("function"));
  test("assign continua legível", () => expect(tAssign).toBe("function"));
  test("keys guardado continua funcionando", () => expect(keysUsado).toBe("x,y"));
});
