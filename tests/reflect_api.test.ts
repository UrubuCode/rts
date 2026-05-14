// (#218) Reflect API — get/set/has/deleteProperty/ownKeys/getPrototypeOf/
// setPrototypeOf/isExtensible/preventExtensions reusam Object.* / map_*.
// apply/construct/getOwnPropertyDescriptor/defineProperty sao novos.

import { describe, test, expect } from "rts:test";

const obj = { a: 1, b: 2, c: 3 };

const a = Reflect.get(obj, "a");
const b = Reflect.get(obj, "missing");
const hasA = Reflect.has(obj, "a");
const hasMiss = Reflect.has(obj, "missing");

const obj2 = { x: 10 };
Reflect.set(obj2, "y", 20);
const xv = Reflect.get(obj2, "x");
const yv = Reflect.get(obj2, "y");

const obj3 = { p: 1, q: 2 };
Reflect.deleteProperty(obj3, "p");
const hasP = Reflect.has(obj3, "p");
const hasQ = Reflect.has(obj3, "q");

const obj4 = { foo: 1, bar: 2 };
const keys = Reflect.ownKeys(obj4);
const keysLen = keys.length;

// apply: reusa Function.prototype.apply.
// Em RTS, user fn precisa virar handle Function antes de Function.apply
// despachar — `.bind(null)` reifica. Anotacao `: i64` no resultado pra
// codegen formatar o retorno como int (Reflect.apply retorna i64 raw).
function add3(a: i64, b: i64, c: i64): i64 { return a + b + c; }
const add3Handle = add3.bind(null);
const sum: i64 = Reflect.apply(add3Handle, null, [10, 20, 30]);

// construct: cria instancia (Map vazio) e roda o constructor com `this`=instancia.
// Em RTS, fn user nao tem slot reservado pra `this` implicito quando declarada
// como `function f(start)`, entao o construtor precisa aceitar `this` como
// primeiro param explicito via `.bind` ou usando arrow capturando contexto.
// Caso de borda v0: documentamos que Reflect.construct retorna a instancia
// alocada (Map vazio) — o constructor ainda nao popula `this` automaticamente.
const counter = Reflect.construct(((Counter as any).bind(null)), [42]);
function Counter(start: i64): void {}
const counterIsObject = typeof counter === "object";

// getOwnPropertyDescriptor: retorna {value, writable, enumerable, configurable}
const desc = Reflect.getOwnPropertyDescriptor(obj, "a");
const descValue = Reflect.get(desc, "value");
const descWritable = Reflect.get(desc, "writable");
const descMissing = Reflect.getOwnPropertyDescriptor(obj, "nope");

// defineProperty: extrai value do descriptor e seta
const obj5: any = {};
Reflect.defineProperty(obj5, "z", { value: 99 });
const zv = Reflect.get(obj5, "z");

describe("reflect_api", () => {
    test("Reflect.get hit", () => expect(a).toBe(1));
    test("Reflect.get miss", () => expect(b).toBe(0));
    test("Reflect.has true", () => expect(hasA).toBe(true));
    test("Reflect.has false", () => expect(hasMiss).toBe(false));
    test("Reflect.set then get", () => expect(yv).toBe(20));
    test("Reflect.set preserves existing", () => expect(xv).toBe(10));
    test("Reflect.deleteProperty removes", () => expect(hasP).toBe(false));
    test("Reflect.deleteProperty preserves siblings", () => expect(hasQ).toBe(true));
    test("Reflect.ownKeys length", () => expect(keysLen).toBe(2));
    test("Reflect.apply with args", () => expect(sum).toBe(60));
    test("Reflect.construct returns object", () =>
        expect(counterIsObject).toBe(true));
    test("Reflect.getOwnPropertyDescriptor value", () => expect(descValue).toBe(1));
    test("Reflect.getOwnPropertyDescriptor writable", () =>
        expect(descWritable).toBe(1));
    // (#795) Missing prop retorna handle de string "undefined" (ECMA spec).
    test("Reflect.getOwnPropertyDescriptor missing", () =>
        expect(descMissing).toBe("undefined"));
    test("Reflect.defineProperty writes value", () => expect(zv).toBe(99));
});
