// (#218 phase 3) Proxy traps: setPrototypeOf, defineProperty,
// getOwnPropertyDescriptor. Cada um tem path "trap" e "forward".

import { describe, test, expect } from "rts:test";

// 1. setPrototypeOf trap: retorna true sem efeito real.
const t1: any = {};
const p1: any = new Proxy(t1, {
    setPrototypeOf: (_t: any, _proto: any) => true
});
const newProto = { kind: "fake" };
const sp1 = Reflect.setPrototypeOf(p1, newProto);
// target nao foi modificado pelo trap (que so' retornou true)
const t1HasProto = Reflect.has(t1, "__proto__");

// 2. setPrototypeOf forward — escreve __proto__ no target
const t2: any = {};
const p2: any = new Proxy(t2, {});
const realProto = { kind: "real" };
const sp2 = Reflect.setPrototypeOf(p2, realProto);
const t2HasProto = Reflect.has(t2, "__proto__");
// E getPrototypeOf sobre proxy le o proto
const readProto = Reflect.getPrototypeOf(p2);
const readHasKind = Reflect.has(readProto, "kind");

// 3. setPrototypeOf trap rejeita (retorna false)
const rejectProto: any = new Proxy({} as any, {
    setPrototypeOf: (_t: any, _proto: any) => false
});
const sp3 = Reflect.setPrototypeOf(rejectProto, { x: 1 });

// 4. defineProperty trap intercepta — target nao recebe a key
const dt4: any = {};
const dp4: any = new Proxy(dt4, {
    defineProperty: (_t: any, _k: any, _d: any) => true
});
const dr4 = Reflect.defineProperty(dp4, "x", { value: 99 });
const dt4HasX = Reflect.has(dt4, "x");

// 5. defineProperty forward — chega no target
const dt5: any = {};
const dp5: any = new Proxy(dt5, {});
const dr5 = Reflect.defineProperty(dp5, "y", { value: 7 });
const dt5HasY = Reflect.has(dt5, "y");
const dt5Y: i64 = Reflect.get(dt5, "y");

// 6. defineProperty trap rejeita
const reject6: any = new Proxy({} as any, {
    defineProperty: (_t: any, _k: any, _d: any) => false
});
const dr6 = Reflect.defineProperty(reject6, "x", { value: 1 });

// 7. getOwnPropertyDescriptor trap retorna custom descriptor
const t7: any = { x: 100 };
const p7: any = new Proxy(t7, {
    getOwnPropertyDescriptor: (_t: any, _k: any) => {
        return { value: 42, writable: true, enumerable: true, configurable: true } as any;
    }
});
const desc7 = Reflect.getOwnPropertyDescriptor(p7, "x");
const desc7Value: i64 = Reflect.get(desc7, "value");
// (#680/50) Bool em obj literal vira sentinel — comparacao via String().
const desc7Writable = String(Reflect.get(desc7, "writable"));

// 8. getOwnPropertyDescriptor forward — sintetiza do target
const t8: any = { z: 50 };
const p8: any = new Proxy(t8, {});
const desc8 = Reflect.getOwnPropertyDescriptor(p8, "z");
const desc8Value: i64 = Reflect.get(desc8, "value");

// 9. getOwnPropertyDescriptor forward — chave inexistente nao tem `value`
const t9: any = { x: 1 };
const p9: any = new Proxy(t9, {});
const desc9 = Reflect.getOwnPropertyDescriptor(p9, "missing");
// desc9 eh handle 0 (invalido). Reflect.has(0, "value") -> false.
const desc9HasValue = Reflect.has(desc9, "value");

// 10. getOwnPropertyDescriptor trap retorna 0 (chave nao existe segundo trap)
const lyingDesc: any = new Proxy({ real: 1 } as any, {
    getOwnPropertyDescriptor: (_t: any, _k: any) => 0 as any
});
const desc10 = Reflect.getOwnPropertyDescriptor(lyingDesc, "real");
const desc10HasValue = Reflect.has(desc10, "value");

// 11. setPrototypeOf nao-Proxy (Map normal) ainda funciona (regressao)
const reg11: any = {};
const proto11 = { tag: "ok" };
const sp11 = Reflect.setPrototypeOf(reg11, proto11);
const reg11Proto = Reflect.getPrototypeOf(reg11);
const reg11HasTag = Reflect.has(reg11Proto, "tag");

// 12. defineProperty nao-Proxy (Map normal) ainda funciona
const reg12: any = {};
const dr12 = Reflect.defineProperty(reg12, "k", { value: 88 });
const reg12K: i64 = Reflect.get(reg12, "k");

// 13. getOwnPropertyDescriptor nao-Proxy ainda funciona
const reg13: any = { foo: 9 };
const desc13 = Reflect.getOwnPropertyDescriptor(reg13, "foo");
const desc13Value: i64 = Reflect.get(desc13, "value");

describe("proxy_phase3", () => {
    // setPrototypeOf
    test("setProto trap returns true", () => expect(sp1).toBe(true));
    test("setProto trap doesn't write target", () =>
        expect(t1HasProto).toBe(false));
    test("setProto forward returns true", () => expect(sp2).toBe(true));
    test("setProto forward writes target", () =>
        expect(t2HasProto).toBe(true));
    test("setProto + getProto reads via proxy", () =>
        expect(readHasKind).toBe(true));
    test("setProto trap rejects (returns false)", () =>
        expect(sp3).toBe(false));
    // defineProperty
    test("defineProperty trap returns true", () => expect(dr4).toBe(true));
    test("defineProperty trap doesn't reach target", () =>
        expect(dt4HasX).toBe(false));
    test("defineProperty forward returns true", () => expect(dr5).toBe(true));
    test("defineProperty forward writes target", () =>
        expect(dt5HasY).toBe(true));
    test("defineProperty forward value", () => expect(dt5Y).toBe(7));
    test("defineProperty trap rejects", () => expect(dr6).toBe(false));
    // getOwnPropertyDescriptor
    test("getOwnDesc trap returns custom value", () =>
        expect(desc7Value).toBe(42));
    test("getOwnDesc trap returns custom writable", () =>
        expect(desc7Writable).toBe("true"));
    test("getOwnDesc forward synthesizes from target", () =>
        expect(desc8Value).toBe(50));
    test("getOwnDesc forward missing has no value", () =>
        expect(desc9HasValue).toBe(false));
    test("getOwnDesc trap=0 has no value", () =>
        expect(desc10HasValue).toBe(false));
    // Regressao: nao-proxy ainda funciona
    test("setProto on plain map still works", () =>
        expect(reg11HasTag).toBe(true));
    test("defineProperty on plain map still works", () =>
        expect(reg12K).toBe(88));
    test("getOwnDesc on plain map still works", () =>
        expect(desc13Value).toBe(9));
});
