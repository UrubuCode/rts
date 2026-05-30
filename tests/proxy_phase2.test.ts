// (#218 phase 2) Proxy traps: ownKeys, apply, construct, getPrototypeOf.

import { describe, test, expect } from "rts:test";

// 1. ownKeys trap
const t1: any = { a: 1, b: 2 };
const p1: any = new Proxy(t1, {
    ownKeys: (_t: any) => ["x", "y", "z"]
});
const k1 = Reflect.ownKeys(p1);
const k1Len = k1.length;

// 2. ownKeys forward (sem trap)
const t2: any = { a: 1, b: 2, c: 3 };
const p2: any = new Proxy(t2, {});
const k2Len = Reflect.ownKeys(p2).length;

// 3. ownKeys trap retornando vazio
const p3: any = new Proxy({ a: 1 } as any, {
    ownKeys: (_t: any) => []
});
const k3Len = Reflect.ownKeys(p3).length;

// 4. apply trap intercepta proxy(args)
function userFn(): i64 { return 100; }
const userFnH = userFn.bind(null);
const callable1: any = new Proxy(userFnH, {
    apply: (_t: any, _this: any, _args: any) => 42
});
const r1: i64 = callable1();

// 5. apply forward (sem trap) — chama target
const callable2: any = new Proxy(userFnH, {});
const r2: i64 = callable2();

// 6. apply trap recebe args como array
function add2(a: i64, b: i64): i64 { return a + b; }
const add2H = add2.bind(null);
const calc: any = new Proxy(add2H, {
    apply: (_t: any, _this: any, _args: any) => 999
});
const r3: i64 = calc(10, 20);

// 7. apply via Reflect.apply tambem dispara trap
const r4: i64 = Reflect.apply(callable1, null, []);

// 8. construct forward (sem trap) — instancia Map vazio
function Counter(this: any, start: i64): void {}
const counterH = Counter.bind(null);
const proxyCtor: any = new Proxy(counterH, {});
const inst1 = Reflect.construct(proxyCtor, [42]);
const inst1IsObj = typeof inst1 === "object";

// 9. construct trap intercepta
const trapCtor: any = new Proxy(counterH, {
    construct: (_t: any, _args: any, _newT: any) => {
        return { custom: true } as any;
    }
});
const inst2 = Reflect.construct(trapCtor, [99]);
const inst2HasCustom = Reflect.has(inst2, "custom");

// 10. getPrototypeOf trap
const fakeProto = { magic: 1 };
const p5: any = new Proxy({} as any, {
    getPrototypeOf: (_t: any) => fakeProto
});
const proto5 = Reflect.getPrototypeOf(p5);
const proto5HasMagic = Reflect.has(proto5, "magic");

// 11. getPrototypeOf forward — target com __proto__ explicito
const protoBase = { kind: "base" };
const t6: any = { x: 1 };
Reflect.setPrototypeOf(t6, protoBase);
const p6: any = new Proxy(t6, {});
const proto6 = Reflect.getPrototypeOf(p6);
const proto6Kind: i64 = Reflect.has(proto6, "kind") ? 1 : 0;

// 12. ownKeys + Object.keys
// (#98) JS spec: Object.keys dispara [[GetOwnProperty]] por chave de
// ownKeys pra filtrar enumeraveis. Como "only" NAO existe no target e
// nao ha trap getOwnPropertyDescriptor, o forward retorna undefined ->
// nao-enumeravel -> filtrada. Bun/Node retornam [] (length 0).
const p7: any = new Proxy({ a: 1, b: 2 } as any, {
    ownKeys: (_t: any) => ["only"]
});
const k7Len = Object.keys(p7).length;

describe("proxy_phase2", () => {
    // ownKeys
    test("ownKeys trap returns custom array", () => expect(k1Len).toBe(3));
    test("ownKeys forward returns target keys", () => expect(k2Len).toBe(3));
    test("ownKeys trap returns empty", () => expect(k3Len).toBe(0));
    test("Object.keys via proxy ownKeys", () => expect(k7Len).toBe(0));
    // apply
    test("apply trap intercepts proxy()", () => expect(r1).toBe(42));
    test("apply forward calls target", () => expect(r2).toBe(100));
    test("apply trap with args", () => expect(r3).toBe(999));
    test("Reflect.apply dispatches trap", () => expect(r4).toBe(42));
    // construct
    test("construct forward returns object", () =>
        expect(inst1IsObj).toBe(true));
    test("construct trap returns custom object", () =>
        expect(inst2HasCustom).toBe(true));
    // getPrototypeOf
    test("getPrototypeOf trap returns fake proto", () =>
        expect(proto5HasMagic).toBe(true));
    test("getPrototypeOf forward returns target proto", () =>
        expect(proto6Kind).toBe(1));
});
