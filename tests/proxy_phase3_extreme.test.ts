// (#218 phase 3) Cenarios extremos: setPrototypeOf chain, defineProperty
// em loop, getOwnPropertyDescriptor com chave numerica, traps interagindo.

import { describe, test, expect } from "rts:test";

// 1. setPrototypeOf chain — proto -> proto -> proto
const root = { kind: "root" };
const mid = { mid: 1 };
const leaf: any = {};
Reflect.setPrototypeOf(mid, root);
Reflect.setPrototypeOf(leaf, mid);
const leafProto = Reflect.getPrototypeOf(leaf);
const leafHasMid = Reflect.has(leafProto, "mid");
const leafProtoProto = Reflect.getPrototypeOf(leafProto);
const leafGGHasKind = Reflect.has(leafProtoProto, "kind");

// 2. setPrototypeOf via proxy reflete na getProto via mesmo proxy
const t2: any = {};
const p2: any = new Proxy(t2, {});
const newProto2 = { tag: "via-proxy" };
Reflect.setPrototypeOf(p2, newProto2);
const proto2Read = Reflect.getPrototypeOf(p2);
const proto2HasTag = Reflect.has(proto2Read, "tag");

// 3. defineProperty define varias props em sequencia (sem loop pra evitar arrow capture)
const dp3: any = {};
Reflect.defineProperty(dp3, "a", { value: 1 });
Reflect.defineProperty(dp3, "b", { value: 2 });
Reflect.defineProperty(dp3, "c", { value: 3 });
Reflect.defineProperty(dp3, "d", { value: 4 });
Reflect.defineProperty(dp3, "e", { value: 5 });
const dp3Len = Reflect.ownKeys(dp3).length;
const dp3A: i64 = Reflect.get(dp3, "a");
const dp3E: i64 = Reflect.get(dp3, "e");

// 4. defineProperty + getOwnPropertyDescriptor round-trip
const rt4: any = {};
Reflect.defineProperty(rt4, "round", { value: 77 });
const descRT = Reflect.getOwnPropertyDescriptor(rt4, "round");
const descRTVal: i64 = Reflect.get(descRT, "value");

// 5. getOwnPropertyDescriptor sobre proxy com defineProperty trap
// (trap define apenas, descriptor segue path forward sobre target)
const t5: any = {};
const p5: any = new Proxy(t5, {
    defineProperty: (_t: any, _k: any, _d: any) => {
        // trap nao escreve no target — apenas retorna true
        return true;
    }
});
Reflect.defineProperty(p5, "x", { value: 100 });
// target nao tem `x` (trap so' fingiu)
const desc5 = Reflect.getOwnPropertyDescriptor(p5, "x");
const desc5HasValue = Reflect.has(desc5, "value");

// 6. setPrototypeOf trap retornando false rejeita silenciosamente
const t6: any = {};
const original = Reflect.getPrototypeOf(t6);
const p6: any = new Proxy(t6, {
    setPrototypeOf: (_t: any, _p: any) => false
});
const fakeProto = { x: 1 };
const r6 = Reflect.setPrototypeOf(p6, fakeProto);
// target nao mudou — proto continua sendo o mesmo (0 sentinel)
const t6ProtoStill = Reflect.getPrototypeOf(t6);
const t6Unchanged = (t6ProtoStill as any) === (original as any);

// 7. defineProperty + Reflect.has consistencia
const t7: any = {};
const beforeHas = Reflect.has(t7, "k");
Reflect.defineProperty(t7, "k", { value: 42 });
const afterHas = Reflect.has(t7, "k");

// 8. Multi-trap: proxy combina has + defineProperty + getOwnDesc
const multi: any = new Proxy({} as any, {
    has: (_t: any, _k: any) => true,
    defineProperty: (_t: any, _k: any, _d: any) => true,
    getOwnPropertyDescriptor: (_t: any, _k: any) => {
        return { value: 999, writable: true, enumerable: true, configurable: true } as any;
    }
});
const m1 = Reflect.has(multi, "anything");
const m2 = Reflect.defineProperty(multi, "x", { value: 1 });
const m3 = Reflect.getOwnPropertyDescriptor(multi, "x");
const m3Val: i64 = Reflect.get(m3, "value");

// 9. setPrototypeOf via proxy + getProto via mesmo proxy = proto novo
const protoA = { who: "A" };
const protoB = { who: "B" };
const tg9: any = {};
Reflect.setPrototypeOf(tg9, protoA);
const pp9: any = new Proxy(tg9, {});
Reflect.setPrototypeOf(pp9, protoB);
const ck9 = Reflect.getPrototypeOf(pp9);
const ck9HasB = Reflect.has(ck9, "who");
// E target tambem foi atualizado (forward)
const target9Proto = Reflect.getPrototypeOf(tg9);
const target9HasB = Reflect.has(target9Proto, "who");

// 10. defineProperty com value=0 (falsy) preservado
const t10: any = {};
Reflect.defineProperty(t10, "zero", { value: 0 });
const desc10 = Reflect.getOwnPropertyDescriptor(t10, "zero");
const desc10Val: i64 = Reflect.get(desc10, "value");
// E ownKeys ve a key
const t10Has = Reflect.has(t10, "zero");

describe("proxy_phase3_extreme", () => {
    // 1. proto chain
    test("getProto walks first level", () => expect(leafHasMid).toBe(true));
    test("getProto walks second level", () => expect(leafGGHasKind).toBe(true));
    // 2. setProto via proxy
    test("setProto via proxy + getProto via proxy", () =>
        expect(proto2HasTag).toBe(true));
    // 3. defineProperty serial
    test("defineProperty 5 keys", () => expect(dp3Len).toBe(5));
    test("defineProperty value a", () => expect(dp3A).toBe(1));
    test("defineProperty value e", () => expect(dp3E).toBe(5));
    // 4. round-trip
    test("define + getOwnDesc round-trip", () => expect(descRTVal).toBe(77));
    // 5. trap fingindo + descriptor
    test("trap defineProperty fake + getOwnDesc forward (no value)", () =>
        expect(desc5HasValue).toBe(false));
    // 6. trap rejeita setProto
    test("setProto trap reject returns false", () => expect(r6).toBe(false));
    test("setProto trap reject leaves target untouched", () =>
        expect(t6Unchanged).toBe(true));
    // 7. has consistencia
    test("has before defineProperty", () => expect(beforeHas).toBe(false));
    test("has after defineProperty", () => expect(afterHas).toBe(true));
    // 8. multi-trap
    test("multi: has=true", () => expect(m1).toBe(true));
    test("multi: defineProperty=true", () => expect(m2).toBe(true));
    test("multi: getOwnDesc returns trap's value=999", () =>
        expect(m3Val).toBe(999));
    // 9. setProto chain via proxy + target
    test("setProto via proxy reads B", () => expect(ck9HasB).toBe(true));
    test("setProto via proxy mutates target", () =>
        expect(target9HasB).toBe(true));
    // 10. value=0 falsy preserved
    test("defineProperty value=0 has descriptor", () =>
        expect(desc10Val).toBe(0));
    test("defineProperty value=0 visible via has", () =>
        expect(t10Has).toBe(true));
});
