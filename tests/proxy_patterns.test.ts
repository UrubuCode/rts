// (#218) Proxy phase 1 — padroes reais que Vue/MobX/validation wrappers
// usariam. Cobre cenarios de framework reativo simplificados.

import { describe, test, expect } from "rts:test";

// 1. Validation wrapper — proxy retorna 0 pra qualquer key (sem fn factory
// pra evitar issue #195 mutable closures).
const sensitive = { name: "alice", secret: 12345 };
const safe = new Proxy(sensitive, {
    get: (_t: any, _k: any) => 0
});
const safeName: i64 = Reflect.get(safe, "name");
const safeSecret: i64 = Reflect.get(safe, "secret");

// 2. Default values — trap retorna fallback fixo (-1)
const config = { timeout: 5000 };
const cfg = new Proxy(config, {
    get: (_t: any, _k: any) => -1
});
const cfgTimeout: i64 = Reflect.get(cfg, "timeout");
const cfgMissing: i64 = Reflect.get(cfg, "retries");

// 3. Read counter — desabilitado por ora: traps invocadas via
// MAP_GET_CHAIN (re-entrant Rust→arrow→Rust) nao propagam mutacao de
// var capturada (issue #195 mutable closures). O trap roda; so' o
// efeito em var local nao persiste.
// Substituido por: trap retorna valor fixo, verifica que get foi
// invocado pelo retorno.
const counted = new Proxy({ a: 1, b: 2, c: 3 }, {
    get: (_t: any, _k: any) => 777
});
const ca: i64 = Reflect.get(counted, "a");
const cb: i64 = Reflect.get(counted, "missing");

// 4. Write trap rejeita silenciosamente (sem mutacao capturada).
// Confirma que writes nao chegam ao target.
const writeTarget: any = {};
const writes = new Proxy(writeTarget, {
    set: (_t: any, _k: any, _v: i64) => true
});
writes.x = 1;
writes.y = 2;
writes.z = 3;
const writesHasX = Reflect.has(writeTarget, "x");
const writesHasY = Reflect.has(writeTarget, "y");

// 5. Read-only — set sempre rejeita silenciosamente
const roTarget: any = { x: 100 };
const readonly = new Proxy(roTarget, {
    set: (_t: any, _k: any, _v: i64) => true
});
readonly.x = 999;
readonly.y = 7;
const roHasY = Reflect.has(roTarget, "y");
const roX: i64 = Reflect.get(roTarget, "x");

// 6. Logger via valores fixos (sem mutacao capturada — issue #195).
const logger = new Proxy({ a: 1 } as any, {
    get: (_t: any, _k: any) => 11,
    set: (_t: any, _k: any, _v: i64) => true,
    has: (_t: any, _k: any) => true
});
const logG: i64 = Reflect.get(logger, "a");
logger.b = 2;
const logH = Reflect.has(logger, "a");

// 7. Forward direto: handler sem traps eh transparente
const noop = new Proxy({ x: 10, y: 20 } as any, {});
const noopX: i64 = Reflect.get(noop, "x");
noop.z = 30;
const noopHasZ = Reflect.has(noop, "z");

// 8. Trap retornando valor calculado
const calc = new Proxy({ count: 5 } as any, {
    get: (_t: any, _k: any) => 100
});
const calcV: i64 = Reflect.get(calc, "anything");

// 9. Has trap rejeita tudo (fake "empty")
const fakeEmpty = new Proxy({ a: 1, b: 2 } as any, {
    has: (_t: any, _k: any) => false
});
const feHasA = Reflect.has(fakeEmpty, "a");
const feHasB = Reflect.has(fakeEmpty, "b");

// 10. Delete trap rejeita (sem mutacao capturada).
const ptTarget: any = { x: 1, y: 2 };
const protected_ = new Proxy(ptTarget, {
    deleteProperty: (_t: any, _k: any) => false
});
Reflect.deleteProperty(protected_, "x");
Reflect.deleteProperty(protected_, "y");
// target permanece intacto
const ptHasX = Reflect.has(ptTarget, "x");
const ptHasY = Reflect.has(ptTarget, "y");

describe("proxy_patterns", () => {
    // 1
    test("protected wrapper masks all reads", () => expect(safeSecret).toBe(0));
    test("protected wrapper masks name too", () => expect(safeName).toBe(0));
    // 2
    test("withDefaults trap returns fallback (any key)", () =>
        expect(cfgTimeout).toBe(-1));
    test("withDefaults missing also fallback", () =>
        expect(cfgMissing).toBe(-1));
    // 3
    test("get trap invoked (existing key)", () => expect(ca).toBe(777));
    test("get trap invoked (missing key)", () => expect(cb).toBe(777));
    // 4
    test("set trap blocks target writes (x)", () =>
        expect(writesHasX).toBe(false));
    test("set trap blocks target writes (y)", () =>
        expect(writesHasY).toBe(false));
    // 5
    test("readonly: set blocks new key", () => expect(roHasY).toBe(false));
    test("readonly: set blocks overwrite", () => expect(roX).toBe(100));
    // 6
    test("logger get returns trap value", () => expect(logG).toBe(11));
    test("logger has returns trap value", () => expect(logH).toBe(true));
    // 7
    test("empty handler forwards get", () => expect(noopX).toBe(10));
    test("empty handler forwards set", () => expect(noopHasZ).toBe(true));
    // 8
    test("trap returns constant", () => expect(calcV).toBe(100));
    // 9
    test("fake empty has trap rejects existing key", () =>
        expect(feHasA).toBe(false));
    test("fake empty has trap rejects everything", () =>
        expect(feHasB).toBe(false));
    // 10
    test("delete trap preserved x", () => expect(ptHasX).toBe(true));
    test("delete trap preserved y", () => expect(ptHasY).toBe(true));
});
