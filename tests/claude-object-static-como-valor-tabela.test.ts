import { describe, test, expect } from "rts:test";

// A tabela ordenada `OBJECT_FN_OPS` (crates/rts-runtime/src/adapters/value/objfn.rs)
// é a ÚNICA fonte que as duas pontas leem: o front resolve o nome da propriedade
// para a posição nela, o thunk do runtime despacha pela mesma posição.
//
// O código carregado no `env` é 1-BASED de propósito. `__rtsadp_fn_invoke` reserva
// `env == 0` como sentinela de "esta função não captura nada" e o reescreve para a
// palavra `undefined` antes do thunk ver. Com código 0-based o PRIMEIRO item da
// tabela chega como índice enorme, erra a tabela e devolve `undefined` — enquanto
// `typeof` continua dizendo `"function"`. Esse bug já foi pago uma vez em
// `MATH_FN_OPS`; aqui o primeiro item (`assign`, em ordem alfabética) é exercitado
// EXPLICITAMENTE, não um do meio.
//
// Valores conferidos contra o Node v22. Pré-computado no top-level.

// ---- PRIMEIRO ITEM DA TABELA: `assign` ----
const tAssign = typeof Object.assign;
const as = Object.assign;
const alvoAssign: any = as({ a: 1 }, { b: 2 });
const assignSoma = alvoAssign.a + alvoAssign.b; // 3
// `assign` é variádico: mais de uma fonte, última escrita vence.
const alvoMulti: any = as({ a: 1 }, { b: 2 }, { a: 9 });
const assignMulti = alvoMulti.a + "," + alvoMulti.b; // "9,2"
// devolve o PRÓPRIO alvo, não uma cópia
const alvoIdent: any = {};
const assignDevolveAlvo = as(alvoIdent, { z: 1 }) === alvoIdent; // true

// ---- SEGUNDO ITEM: `create` (o vizinho — pega um off-by-one na direção oposta) ----
const cr = Object.create;
const criado: any = cr({ herdado: 42 });
const createHerda = criado.herdado; // 42

// ---- resto da superfície, uma leitura por entrada da tabela ----
const tCreate = typeof Object.create;
const tDefineProperties = typeof Object.defineProperties;
const tDefineProperty = typeof Object.defineProperty;
const tEntries = typeof Object.entries;
const tFreeze = typeof Object.freeze;
const tFromEntries = typeof Object.fromEntries;
const tGetOwnPropertyDescriptor = typeof Object.getOwnPropertyDescriptor;
const tGetOwnPropertyDescriptors = typeof Object.getOwnPropertyDescriptors;
const tGetOwnPropertyNames = typeof Object.getOwnPropertyNames;
const tGetOwnPropertySymbols = typeof Object.getOwnPropertySymbols;
const tGetPrototypeOf = typeof Object.getPrototypeOf;
const tHasOwn = typeof Object.hasOwn;
const tIs = typeof Object.is;
const tIsExtensible = typeof Object.isExtensible;
const tIsFrozen = typeof Object.isFrozen;
const tIsSealed = typeof Object.isSealed;
const tKeys = typeof Object.keys;
const tPreventExtensions = typeof Object.preventExtensions;
const tSeal = typeof Object.seal;
const tSetPrototypeOf = typeof Object.setPrototypeOf;
const tValues = typeof Object.values;

// ---- cada valor lido tem de COMPUTAR o mesmo que a forma chamada ----
const ks = Object.keys;
const kLido = ks({ x: 1, y: 2 }).join(",");
const kChamado = Object.keys({ x: 1, y: 2 }).join(","); // "x,y"

const vs = Object.values;
const vLido = vs({ x: 1, y: 2 }).join(",");
const vChamado = Object.values({ x: 1, y: 2 }).join(","); // "1,2"

const en = Object.entries;
const eLido = JSON.stringify(en({ x: 1 }));
const eChamado = JSON.stringify(Object.entries({ x: 1 })); // [["x",1]]

const ehIgual = Object.is;
const isNaNNaN = ehIgual(NaN, NaN); // true — SameValue, não ===
const isZeros = ehIgual(0, -0); // false

const temProprio = Object.hasOwn;
const hasOwnSim = temProprio({ a: 1 }, "a"); // true
const hasOwnNao = temProprio({ a: 1 }, "b"); // false

// integridade: os trampolins devolvem um FLAG, mas JS devolve o OBJETO
const fz = Object.freeze;
const objFz: any = { a: 1 };
const fzDevolveObj = fz(objFz) === objFz; // true
const fzCongelou = Object.isFrozen(objFz); // true

const sl = Object.seal;
const objSl: any = { a: 1 };
const slDevolveObj = sl(objSl) === objSl; // true
const slSelou = Object.isSealed(objSl); // true

const pe = Object.preventExtensions;
const objPe: any = { a: 1 };
const peDevolveObj = pe(objPe) === objPe; // true
const peBloqueou = Object.isExtensible(objPe); // false

// predicados lidos como valor devolvem BOOLEAN de verdade, não 0/1
const isExt = Object.isExtensible;
const isFrz = Object.isFrozen;
const tipoIsExt = typeof isExt({}); // "boolean"
const valIsExt = isExt({}); // true
const valIsFrz = isFrz(Object.freeze({})); // true

// `length` de cada estático lido como valor (spec)
const lenAssign = (Object.assign as any).length; // 2
const lenKeys = (Object.keys as any).length; // 1
const lenDefineProperty = (Object.defineProperty as any).length; // 3

// `name` do estático lido como valor
const nomeAssign = (Object.assign as any).name; // "assign"

// prototipagem lida como valor
const gpo = Object.getPrototypeOf;
const spo = Object.setPrototypeOf;
const baseProto: any = { marca: "base" };
const filho: any = spo({}, baseProto);
const protoIgual = gpo(filho) === baseProto; // true

// nomes próprios lidos como valor
const gopn = Object.getOwnPropertyNames;
const gopnLido = gopn({ a: 1, b: 2 }).join(","); // "a,b"

const gopds = Object.getOwnPropertyDescriptors;
const descs: any = gopds({ a: 5 });
const gopdsValor = descs.a.value; // 5

// um estático que NÃO existe continua `undefined`, não vira função fantasma
const naoExiste = (Object as any).naoExisteMesmo;

describe("primeiro item da tabela (`assign`) — a armadilha do env 0", () => {
  test("assign é legível como valor", () => expect(tAssign).toBe("function"));
  test("assign guardado copia as chaves", () => expect(assignSoma).toBe(3));
  test("assign guardado é variádico, última escrita vence", () =>
    expect(assignMulti).toBe("9,2"));
  test("assign guardado devolve o próprio alvo", () =>
    expect(assignDevolveAlvo).toBe(true));
  test("assign guardado declara length 2", () => expect(lenAssign).toBe(2));
  test("assign guardado se chama `assign`", () => expect(nomeAssign).toBe("assign"));
});

describe("segundo item da tabela (`create`) — off-by-one na outra direção", () => {
  test("create é legível como valor", () => expect(tCreate).toBe("function"));
  test("create guardado instala o protótipo", () => expect(createHerda).toBe(42));
});

describe("toda entrada da tabela é legível como valor", () => {
  test("defineProperties", () => expect(tDefineProperties).toBe("function"));
  test("defineProperty", () => expect(tDefineProperty).toBe("function"));
  test("entries", () => expect(tEntries).toBe("function"));
  test("freeze", () => expect(tFreeze).toBe("function"));
  test("fromEntries", () => expect(tFromEntries).toBe("function"));
  test("getOwnPropertyDescriptor", () =>
    expect(tGetOwnPropertyDescriptor).toBe("function"));
  test("getOwnPropertyDescriptors", () =>
    expect(tGetOwnPropertyDescriptors).toBe("function"));
  test("getOwnPropertyNames", () => expect(tGetOwnPropertyNames).toBe("function"));
  test("getOwnPropertySymbols", () => expect(tGetOwnPropertySymbols).toBe("function"));
  test("getPrototypeOf", () => expect(tGetPrototypeOf).toBe("function"));
  test("hasOwn", () => expect(tHasOwn).toBe("function"));
  test("is", () => expect(tIs).toBe("function"));
  test("isExtensible", () => expect(tIsExtensible).toBe("function"));
  test("isFrozen", () => expect(tIsFrozen).toBe("function"));
  test("isSealed", () => expect(tIsSealed).toBe("function"));
  test("keys", () => expect(tKeys).toBe("function"));
  test("preventExtensions", () => expect(tPreventExtensions).toBe("function"));
  test("seal", () => expect(tSeal).toBe("function"));
  test("setPrototypeOf", () => expect(tSetPrototypeOf).toBe("function"));
  test("values", () => expect(tValues).toBe("function"));
});

describe("o valor lido computa o mesmo que a forma chamada", () => {
  test("keys", () => expect(kLido).toBe(kChamado));
  test("keys dá x,y", () => expect(kLido).toBe("x,y"));
  test("values", () => expect(vLido).toBe(vChamado));
  test("values dá 1,2", () => expect(vLido).toBe("1,2"));
  test("entries", () => expect(eLido).toBe(eChamado));
  test("entries dá [[\"x\",1]]", () => expect(eLido).toBe('[["x",1]]'));
  test("getOwnPropertyNames", () => expect(gopnLido).toBe("a,b"));
  test("getOwnPropertyDescriptors", () => expect(gopdsValor).toBe(5));
});

describe("semântica de borda pelo valor lido", () => {
  test("is(NaN, NaN) é true (SameValue, não ===)", () => expect(isNaNNaN).toBe(true));
  test("is(0, -0) é false", () => expect(isZeros).toBe(false));
  test("hasOwn própria", () => expect(hasOwnSim).toBe(true));
  test("hasOwn ausente", () => expect(hasOwnNao).toBe(false));
  test("getPrototypeOf/setPrototypeOf casam", () => expect(protoIgual).toBe(true));
});

describe("integridade: devolve o OBJETO, não o flag interno", () => {
  test("freeze devolve o alvo", () => expect(fzDevolveObj).toBe(true));
  test("freeze congela de verdade", () => expect(fzCongelou).toBe(true));
  test("seal devolve o alvo", () => expect(slDevolveObj).toBe(true));
  test("seal sela de verdade", () => expect(slSelou).toBe(true));
  test("preventExtensions devolve o alvo", () => expect(peDevolveObj).toBe(true));
  test("preventExtensions bloqueia de verdade", () => expect(peBloqueou).toBe(false));
});

describe("predicados lidos como valor devolvem boolean, não 0/1", () => {
  test("typeof isExtensible({}) é boolean", () => expect(tipoIsExt).toBe("boolean"));
  test("isExtensible({}) é true", () => expect(valIsExt).toBe(true));
  test("isFrozen(freeze({})) é true", () => expect(valIsFrz).toBe(true));
});

describe("length e não-membros", () => {
  test("keys declara length 1", () => expect(lenKeys).toBe(1));
  test("defineProperty declara length 3", () => expect(lenDefineProperty).toBe(3));
  test("um não-membro continua undefined", () =>
    expect(naoExiste === undefined).toBe(true));
});
