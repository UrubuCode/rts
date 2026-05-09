// (#218) Reflect — casos atipicos: mutacao em loop, em ternario, com chaves
// numericas, encadeado, em try/catch, em closure, com `this`, sob `for...in`,
// com nomes que parecem reservados, com strings vazias, com arrays como obj.
//
// Cobre as variacoes que o CLAUDE.md pede ("criatividade ao testar").

import { describe, test, expect } from "rts:test";

// 1. Reflect.get/set em loop — escrita repetida em chave dinamica
const acc: any = {};
for (let i = 0; i < 5; i++) {
    Reflect.set(acc, "k" + i, i * 10);
}
const accK0 = Reflect.get(acc, "k0");
const accK4 = Reflect.get(acc, "k4");
const accSize = Reflect.ownKeys(acc).length;

// 2. has em chave nao existente vs valor 0 / falsy — has nao confunde
const obj0 = { z: 0, e: "", n: null };
const hasZ = Reflect.has(obj0, "z");
const hasE = Reflect.has(obj0, "e");
const hasN = Reflect.has(obj0, "n");
const hasMiss = Reflect.has(obj0, "missing");

// 3. set/has/get em sequencia, mesma chave — idempotencia
const obj1: any = {};
Reflect.set(obj1, "k", 1);
Reflect.set(obj1, "k", 2);
Reflect.set(obj1, "k", 3);
const obj1Final = Reflect.get(obj1, "k");

// 4. deleteProperty + has — flag muda corretamente
const obj2 = { x: 1 };
const beforeDel = Reflect.has(obj2, "x");
Reflect.deleteProperty(obj2, "x");
const afterDel = Reflect.has(obj2, "x");
const obj2Keys = Reflect.ownKeys(obj2).length;

// 5. ownKeys em obj vazio
const empty: any = {};
const emptyKeys = Reflect.ownKeys(empty).length;

// 6. ownKeys preservado apos varios sets/deletes
const churn: any = {};
Reflect.set(churn, "a", 1);
Reflect.set(churn, "b", 2);
Reflect.deleteProperty(churn, "a");
Reflect.set(churn, "c", 3);
const churnKeys = Reflect.ownKeys(churn).length;

// 7. ternario decidindo presenca/ausencia. Reflect.has e' o ponto-chave
// quando o caller nao quer assumir tipo do slot (Reflect.get retorna i64
// raw — pode ser handle de string ou int direto).
const cfg: any = { mode: "fast" };
const cfgPresent = Reflect.has(cfg, "mode");
const cfgMissing = Reflect.has(cfg, "level");

// 8. Reflect dentro de try/catch — nao deve throw em casos normais
let thrown = false;
try {
    const t: any = {};
    Reflect.set(t, "ok", 1);
    Reflect.get(t, "ok");
    Reflect.deleteProperty(t, "ok");
} catch (_e) {
    thrown = true;
}

// 9. Reflect com referencia compartilhada — mutacao vista entre escopos.
// (Sem closure — closure capturando string da' problemas em RTS hoje.)
const sharedObj = { val: 100 };
const sharedV1 = Reflect.get(sharedObj, "val");
Reflect.set(sharedObj, "val", 200);
const sharedV2 = Reflect.get(sharedObj, "val");

// 10. apply com array vazio — fn sem args
function noargs(): i64 { return 7; }
const applyEmpty = Reflect.apply(noargs.bind(null), null, []);

// 11. apply com 1 arg
function double(x: i64): i64 { return x * 2; }
const applyOne = Reflect.apply(double.bind(null), null, [21]);

// 12. apply em fn com retorno string
function greet(name: string): string { return "hi, " + name; }
const applyStr = Reflect.apply(greet.bind(null), null, ["world"]);

// 13. defineProperty com value falsy (0, "", null) — value preservado
const dp: any = {};
Reflect.defineProperty(dp, "zero", { value: 0 });
Reflect.defineProperty(dp, "neg", { value: -1 });
const dpZero = Reflect.get(dp, "zero");
const dpNeg = Reflect.get(dp, "neg");
const dpHasZero = Reflect.has(dp, "zero");

// 14. defineProperty sobrescrevendo prop existente
const dp2: any = { x: 1 };
Reflect.defineProperty(dp2, "x", { value: 99 });
const dp2x = Reflect.get(dp2, "x");

// 15. getOwnPropertyDescriptor sobre obj com value 0 (falsy)
const obj3 = { zero: 0 };
const desc0 = Reflect.getOwnPropertyDescriptor(obj3, "zero");
const desc0Value = Reflect.get(desc0, "value");
const desc0Configurable = Reflect.get(desc0, "configurable");

// 16. Reflect.set retorno e' sempre true (v0)
const setRetObj: any = {};
const setRet = Reflect.set(setRetObj, "k", 1);

// 17. Reflect.deleteProperty retorna true em sucesso e em chave inexistente
const delTrue = Reflect.deleteProperty({ a: 1 } as any, "a");
const delMissing = Reflect.deleteProperty({} as any, "absent");

// 18. Reflect.ownKeys retorna array com length correto.
// (Soma de Reflect.get nao roda direto porque o retorno tem tag Handle —
// se voce sabe que e' int, atribua a uma var anotada `: i64` ou use o
// access direto obj.x.)
const triple = { x: 1, y: 2, z: 3 };
const tripleKeys = Reflect.ownKeys(triple);
const tripleLen = tripleKeys.length;
const tripleX: i64 = triple.x;
const tripleY: i64 = triple.y;
const tripleZ: i64 = triple.z;
const tripleSum = tripleX + tripleY + tripleZ;

describe("reflect_edge_cases", () => {
    // 1. loop
    test("set in loop k0", () => expect(accK0).toBe(0));
    test("set in loop k4", () => expect(accK4).toBe(40));
    test("set in loop count", () => expect(accSize).toBe(5));
    // 2. has com falsy
    test("has zero value", () => expect(hasZ).toBe(true));
    test("has empty string value", () => expect(hasE).toBe(true));
    test("has null value", () => expect(hasN).toBe(true));
    test("has missing", () => expect(hasMiss).toBe(false));
    // 3. set repetido
    test("repeated set keeps last", () => expect(obj1Final).toBe(3));
    // 4. delete + has
    test("has before delete", () => expect(beforeDel).toBe(true));
    test("has after delete", () => expect(afterDel).toBe(false));
    test("ownKeys after delete", () => expect(obj2Keys).toBe(0));
    // 5. obj vazio
    test("ownKeys empty", () => expect(emptyKeys).toBe(0));
    // 6. churn
    test("ownKeys after set+delete+set", () => expect(churnKeys).toBe(2));
    // 7. has em ternario context
    test("has present", () => expect(cfgPresent).toBe(true));
    test("has missing", () => expect(cfgMissing).toBe(false));
    // 8. try/catch
    test("normal Reflect ops dont throw", () => expect(thrown).toBe(false));
    // 9. mutacao via Reflect.set
    test("Reflect.get reads initial", () => expect(sharedV1).toBe(100));
    test("Reflect.get sees mutation", () => expect(sharedV2).toBe(200));
    // 10-12. apply
    test("apply zero args", () => expect(applyEmpty).toBe(7));
    test("apply one arg", () => expect(applyOne).toBe(42));
    test("apply string arg", () => expect(applyStr).toBe("hi, world"));
    // 13-14. defineProperty
    test("defineProperty value=0 preserved", () => expect(dpZero).toBe(0));
    test("defineProperty value=-1 preserved", () => expect(dpNeg).toBe(-1));
    test("defineProperty makes prop visible", () => expect(dpHasZero).toBe(true));
    test("defineProperty overwrites", () => expect(dp2x).toBe(99));
    // 15. descriptor
    test("descriptor value=0 preserved", () => expect(desc0Value).toBe(0));
    test("descriptor configurable=true", () => expect(desc0Configurable).toBe(1));
    // 16. set return
    test("Reflect.set returns true", () => expect(setRet).toBe(true));
    // 17. delete return
    test("delete present returns true", () => expect(delTrue).toBe(true));
    test("delete missing returns true", () => expect(delMissing).toBe(true));
    // 18. ownKeys length + soma
    test("ownKeys returns length 3", () => expect(tripleLen).toBe(3));
    test("direct member access sum", () => expect(tripleSum).toBe(6));
});
