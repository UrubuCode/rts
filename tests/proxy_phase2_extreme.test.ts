// (#218 phase 2) Cenarios extremos pra Proxy: combinacoes de traps,
// proxy aninhado, target=Array, ownKeys com mutacao concomitante,
// apply em fn variadica, construct via new sintatico (ainda nao trapeia,
// confirma forward), descriptor querying via Reflect, etc.

import { describe, test, expect } from "rts:test";

// 1. Proxy com 4 traps simultaneos (get + set + has + ownKeys)
const t1: any = { a: 1, b: 2 };
const p1: any = new Proxy(t1, {
    get: (_t: any, _k: any) => 100,
    set: (_t: any, _k: any, _v: i64) => true,
    has: (_t: any, _k: any) => true,
    ownKeys: (_t: any) => ["xx", "yy"]
});
const p1G: i64 = Reflect.get(p1, "a");
const p1H = Reflect.has(p1, "anything");
p1.z = 99;
const p1OK = Reflect.ownKeys(p1).length;

// 2. Proxy aninhado em array — push e map
const innerObj: any = { v: 5 };
const innerProxy: any = new Proxy(innerObj, { get: (_t, _k) => 77 });
const arr: any[] = [innerProxy];
const arrLen = arr.length;
const arrEl = Reflect.get(arr[0] as any, "v");

// 3. ownKeys trap retorna array com mais elementos que target
const small: any = { only: 1 };
const expandedKeys: any = new Proxy(small, {
    ownKeys: (_t: any) => ["one", "two", "three", "four", "five"]
});
const expLen = Reflect.ownKeys(expandedKeys).length;

// 4. ownKeys trap retorna array gigante
const giant: any = new Proxy({} as any, {
    ownKeys: (_t: any) => {
        const ks: string[] = [];
        for (let i = 0; i < 50; i++) {
            ks.push("k" + i);
        }
        return ks;
    }
});
const giantLen = Reflect.ownKeys(giant).length;

// 5. apply trap com args do tipo string
function joinFn(a: string, b: string): string { return a + b; }
const joinH = joinFn.bind(null);
const joinProxy: any = new Proxy(joinH, {
    apply: (_t: any, _this: any, _args: any) => "TRAPPED"
});
const joinR: string = joinProxy("x", "y");

// 6. apply trap modifica retorno baseado em args length (sem ler args)
function pow(b: i64, e: i64): i64 { return b * e; }
const powH = pow.bind(null);
const tripler: any = new Proxy(powH, {
    apply: (_t: any, _this: any, _args: any) => 3
});
const tr1: i64 = tripler();
const tr2: i64 = tripler(10);
const tr3: i64 = tripler(10, 20);
// Todos retornam 3 — trap nao olha args
const trSum = tr1 + tr2 + tr3;

// 7. apply forward — fn sem args, retorno scalar simples
function const42(): i64 { return 42; }
const c42H = const42.bind(null);
const c42P: any = new Proxy(c42H, {});
const multR: i64 = c42P();  // espera 42 via forward

// 8. construct forward — instancia + apply chama ctor com this
function Box(this: any, w: i64, h: i64): void {}
const boxH = Box.bind(null);
const boxProxy: any = new Proxy(boxH, {});
const box1 = Reflect.construct(boxProxy, [10, 20]);
const box1IsObj = typeof box1 === "object";

// 9. construct trap retorna obj completamente customizado (com varios fields)
const ctorH2 = Box.bind(null);
const fancy: any = new Proxy(ctorH2, {
    construct: (_t: any, _a: any, _nt: any) => {
        const o: any = {};
        o.alpha = 1;
        o.beta = 2;
        o.gamma = 3;
        return o;
    }
});
const fancyInst = Reflect.construct(fancy, []);
const fancyKeys = Reflect.ownKeys(fancyInst).length;
const fancyAlpha: i64 = Reflect.get(fancyInst, "alpha");

// 10. getPrototypeOf chain — proxy de proxy de obj
const base = { kind: "deep" };
const middle: any = { mid: 1 };
Reflect.setPrototypeOf(middle, base);
const middleProxy: any = new Proxy(middle, {});
// proxy do middle proxy
const outerProxy: any = new Proxy(middleProxy, {});
const outerProto = Reflect.getPrototypeOf(outerProxy);
// proto de middleProxy eh proto de middle, que eh `base`
const outerProtoHasKind = Reflect.has(outerProto, "kind");

// 11. Combinacao: ownKeys + has + get formam um "view" coerente
const dbView: any = new Proxy({} as any, {
    ownKeys: (_t: any) => ["id", "name", "email"],
    has: (_t: any, _k: any) => true,    // afirma que toda key existe
    get: (_t: any, _k: any) => 42        // valor stub
});
const dbKeys = Reflect.ownKeys(dbView).length;
const dbHasFoo = Reflect.has(dbView, "foo");
const dbGetBar: i64 = Reflect.get(dbView, "bar");

// 12. Reflect.has rejeita tudo, mas get ainda retorna
const enigma: any = new Proxy({ real: 1 } as any, {
    has: (_t: any, _k: any) => false,
    get: (_t: any, _k: any) => 7
});
const eHas = Reflect.has(enigma, "real");
const eGet: i64 = Reflect.get(enigma, "real");

// 13. construct chamado em fn var-args (ignora arg count)
function VarCtor(this: any, ...vals: i64[]): void {}
const varH = VarCtor.bind(null);
const varProxy: any = new Proxy(varH, {});
const varInst = Reflect.construct(varProxy, []);
const varInstObj = typeof varInst === "object";

// 14. apply executado N vezes — sem GC issues
function double_(x: i64): i64 { return x * 2; }
const dH = double_.bind(null);
const dProxy: any = new Proxy(dH, {
    apply: (_t: any, _this: any, _args: any) => 5
});
let dSum: i64 = 0;
for (let i = 0; i < 10; i++) {
    dSum = dSum + dProxy();
}
// Cada call retorna 5; 10 chamadas = 50

// 15. ownKeys re-aplicado apos set forward
const evolving: any = { initial: 1 };
const evP: any = new Proxy(evolving, {});
const ev0 = Reflect.ownKeys(evP).length;
evP.added = 2;     // forward set
const ev1 = Reflect.ownKeys(evP).length;
evP.more = 3;
const ev2 = Reflect.ownKeys(evP).length;

describe("proxy_phase2_extreme", () => {
    // 1. multi-trap
    test("multi: get returns trap value", () => expect(p1G).toBe(100));
    test("multi: has returns trap value", () => expect(p1H).toBe(true));
    test("multi: ownKeys returns trap array", () => expect(p1OK).toBe(2));
    // 2. nested in array
    test("array containing proxy", () => expect(arrLen).toBe(1));
    test("array proxy element get", () => expect(arrEl).toBe(77));
    // 3-4. ownKeys size
    test("ownKeys can grow target", () => expect(expLen).toBe(5));
    test("ownKeys with 50 keys", () => expect(giantLen).toBe(50));
    // 5. apply trap with strings
    test("apply trap returns string", () => expect(joinR).toBe("TRAPPED"));
    // 6. apply trap ignoring args
    test("apply trap ignores arg count (3 calls × 3)", () =>
        expect(trSum).toBe(9));
    // 7. apply forward with multiple args
    test("apply forward (no args) returns 42", () => expect(multR).toBe(42));
    // 8-9. construct
    test("construct forward returns object", () =>
        expect(box1IsObj).toBe(true));
    test("construct trap returns custom object — keys", () =>
        expect(fancyKeys).toBe(3));
    test("construct trap returns custom object — alpha", () =>
        expect(fancyAlpha).toBe(1));
    // 10. proto chain via proxy
    test("getPrototypeOf walks via proxy of proxy", () =>
        expect(outerProtoHasKind).toBe(true));
    // 11. coherent view
    test("dbView ownKeys", () => expect(dbKeys).toBe(3));
    test("dbView has stubs all true", () => expect(dbHasFoo).toBe(true));
    test("dbView get stubs all 42", () => expect(dbGetBar).toBe(42));
    // 12. has lies but get works
    test("has=false but get=7 (independent traps)", () =>
        expect(eHas).toBe(false));
    test("get returns despite has lying", () => expect(eGet).toBe(7));
    // 13. var-args ctor
    test("var-args constructor via proxy", () =>
        expect(varInstObj).toBe(true));
    // 14. loop apply
    test("apply trap stable in loop (10×5)", () => expect(dSum).toBe(50));
    // 15. ownKeys grows after set
    test("ownKeys initial=1", () => expect(ev0).toBe(1));
    test("ownKeys after one set=2", () => expect(ev1).toBe(2));
    test("ownKeys after two sets=3", () => expect(ev2).toBe(3));
});
