// (#218) Proxy phase 1 — get/set/has/deleteProperty traps + forward.

import { describe, test, expect } from "rts:test";

// 1. Get trap intercepta
const t1: any = { x: 10, y: 20 };
const h1: any = { get: (_t: any, _k: any) => 42 };
const p1: any = new Proxy(t1, h1);
const p1x: i64 = p1.x;
const p1any: i64 = p1.anything;
const p1y: i64 = p1.y;

// 2. Sem trap, faz forward pro target
const t2: any = { foo: 100, bar: 200 };
const h2: any = {};
const p2: any = new Proxy(t2, h2);
const p2foo: i64 = p2.foo;
const p2bar: i64 = p2.bar;
// JS fiel: key ausente le' undefined (era coercao i64→0 do modelo velho).
const p2miss = p2.missing;

// 3. Set trap intercepta
let setCount = 0;
const t3: any = { x: 0 };
const h3: any = {
    set: (_t: any, _k: any, _v: i64) => {
        setCount = setCount + 1;
        return true;
    }
};
const p3: any = new Proxy(t3, h3);
p3.a = 1;
p3.b = 2;
p3.c = 3;
// target NAO foi modificado porque a trap nao chama Reflect.set —
// `Reflect.has(t3, "a")` retorna false comprovando.
const t3hasA = Reflect.has(t3, "a");

// 4. Sem set trap, faz forward
const t4: any = {};
const h4: any = {};
const p4: any = new Proxy(t4, h4);
p4.foo = 99;
p4.bar = 77;
const t4foo: i64 = t4.foo;
const t4bar: i64 = t4.bar;

// 5. Has trap
const t5: any = { real: 1 };
const h5: any = {
    has: (_t: any, _k: any) => true   // sempre true
};
const p5: any = new Proxy(t5, h5);
const p5real = Reflect.has(p5, "real");
const p5fake = Reflect.has(p5, "anything");

// 6. Sem has trap, forward (so' real existe)
const t6: any = { x: 1 };
const h6: any = {};
const p6: any = new Proxy(t6, h6);
const p6has_x = Reflect.has(p6, "x");
const p6has_y = Reflect.has(p6, "y");

// 7. deleteProperty trap
let delCount = 0;
const t7: any = { x: 1, y: 2, z: 3 };
const h7: any = {
    deleteProperty: (_t: any, _k: any) => {
        delCount = delCount + 1;
        return true;
    }
};
const p7: any = new Proxy(t7, h7);
Reflect.deleteProperty(p7, "x");
Reflect.deleteProperty(p7, "y");
// target nao foi modificado (trap nao chamou Reflect.delete)
const t7stillX = Reflect.has(t7, "x");

// 8. Sem deleteProperty trap, forward
const t8: any = { a: 1, b: 2 };
const h8: any = {};
const p8: any = new Proxy(t8, h8);
Reflect.deleteProperty(p8, "a");
const t8hasA = Reflect.has(t8, "a");
const t8hasB = Reflect.has(t8, "b");

// 9. Validacao: handler nao-Map devolve handle 0 (falsy)
// Em RTS, criar Proxy(target, "string-fake") deve retornar 0.
// Skip — usar um Map como handler eh o caminho normal. Apenas
// confirma que objeto literal vazio funciona (caso #2 ja' cobre).

// 10. Get trap pode delegar ao target via `t.k` direto
const t10: any = { count: 5 };
const h10: any = {
    get: (tar: any, k: any) => {
        // Acesso direto a propriedade do target — sem recursao
        return tar.count + 1;
    }
};
const p10: any = new Proxy(t10, h10);
const p10v: i64 = p10.count;
const p10any: i64 = p10.anything;

describe("proxy_phase1", () => {
    // 1. get trap
    test("get trap intercepts existing", () => expect(p1x).toBe(42));
    test("get trap intercepts missing", () => expect(p1any).toBe(42));
    test("get trap intercepts y", () => expect(p1y).toBe(42));
    // 2. forward
    test("get forward foo", () => expect(p2foo).toBe(100));
    test("get forward bar", () => expect(p2bar).toBe(200));
    test("get forward missing is undefined", () => expect(p2miss === undefined).toBe(true));
    // 3. set trap
    test("set trap counts invocations", () => expect(setCount).toBe(3));
    test("set trap does not reach target unless explicit", () =>
        expect(t3hasA).toBe(false));
    // 4. set forward
    test("set forward writes to target foo", () => expect(t4foo).toBe(99));
    test("set forward writes to target bar", () => expect(t4bar).toBe(77));
    // 5. has trap
    test("has trap real", () => expect(p5real).toBe(true));
    test("has trap fake (always true)", () => expect(p5fake).toBe(true));
    // 6. has forward
    test("has forward present", () => expect(p6has_x).toBe(true));
    test("has forward missing", () => expect(p6has_y).toBe(false));
    // 7. deleteProperty trap
    test("delete trap counts", () => expect(delCount).toBe(2));
    test("delete trap doesn't touch target", () => expect(t7stillX).toBe(true));
    // 8. delete forward
    test("delete forward removes from target", () => expect(t8hasA).toBe(false));
    test("delete forward preserves siblings", () => expect(t8hasB).toBe(true));
    // 10. trap usa target
    test("trap reads target.count + 1", () => expect(p10v).toBe(6));
    test("trap returns same value for missing key", () =>
        expect(p10any).toBe(6));
});
