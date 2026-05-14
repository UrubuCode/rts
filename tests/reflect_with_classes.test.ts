// (#218) Reflect com classes user, herancia, padroes de framework reativo
// (observer/proxy-like). Cobre integracao Reflect <-> classes existentes.

import { describe, test, expect } from "rts:test";

// 1. Classe simples — Reflect.get/has em instancia
class Counter {
    value: i64;
    constructor() { this.value = 0; }
    inc(): void { this.value = this.value + 1; }
}
const c = new Counter();
c.inc();
c.inc();
const cValue: i64 = c.value;          // direct access
const cHasValue = Reflect.has(c, "value");

// 2. Heranca + Reflect.has — instancia ve fields da subclasse
class Box {
    w: i64;
    h: i64;
    constructor(w: i64, h: i64) { this.w = w; this.h = h; }
}
class ColoredBox extends Box {
    color: string;
    constructor(w: i64, h: i64, color: string) {
        super(w, h);
        this.color = color;
    }
}
const cb = new ColoredBox(10, 20, "red");
const cbW: i64 = cb.w;
const cbHasColor = Reflect.has(cb, "color");
const cbHasMissing = Reflect.has(cb, "depth");

// 3. Reflect.set para atualizar field de instancia
class Profile {
    name: string;
    age: i64;
    constructor(n: string, a: i64) { this.name = n; this.age = a; }
}
const p = new Profile("alice", 30);
Reflect.set(p, "age", 31);
const pAge: i64 = p.age;

// 4. Padrao "observer simples" usando Reflect
let lastChange: string = "";
function setObserved(o: any, key: string, val: i64): void {
    Reflect.set(o, key, val);
    lastChange = key;
}
const state: any = {};
setObserved(state, "x", 1);
setObserved(state, "y", 2);
const stateX = Reflect.has(state, "x");
const stateY = Reflect.has(state, "y");

// 5. defineProperty + ownKeys — visibilidade
const dyn: any = {};
Reflect.defineProperty(dyn, "a", { value: 1 });
Reflect.defineProperty(dyn, "b", { value: 2 });
Reflect.defineProperty(dyn, "c", { value: 3 });
const dynKeys = Reflect.ownKeys(dyn).length;

// 6. Counter via Reflect.apply — pattern de "imediato com args dinamicos"
function pow(base: i64, exp: i64): i64 {
    let r: i64 = 1;
    for (let i = 0; i < exp; i++) r = r * base;
    return r;
}
const pow23 = Reflect.apply(pow.bind(null), null, [2, 3]);     // 8
const pow52 = Reflect.apply(pow.bind(null), null, [5, 2]);     // 25
const pow10 = Reflect.apply(pow.bind(null), null, [10, 0]);    // 1

// 7. Mudanca de descritor — getOwnPropertyDescriptor sempre devolve value v0
class Holder {
    v: i64;
    constructor() { this.v = 7; }
}
const h = new Holder();
const hDesc = Reflect.getOwnPropertyDescriptor(h, "v");
const hDescValue: i64 = Reflect.get(hDesc, "value");
const hDescConfig = Reflect.get(hDesc, "configurable");
const hDescEnum = Reflect.get(hDesc, "enumerable");
// (#795) Pre-coerce no top-level pra preservar tipo Handle do bool sentinel.
const hDescConfigStr = "" + hDescConfig;
const hDescEnumStr = "" + hDescEnum;

// 8. Reflect.has antes/depois de defineProperty
const lazy: any = {};
const beforeDef = Reflect.has(lazy, "x");
Reflect.defineProperty(lazy, "x", { value: 100 });
const afterDef = Reflect.has(lazy, "x");
const lazyX = Reflect.get(lazy, "x");

// 9. ownKeys preserva ordem de insercao (Map IndexMap)
const orderObj: any = {};
Reflect.set(orderObj, "first", 1);
Reflect.set(orderObj, "second", 2);
Reflect.set(orderObj, "third", 3);
const orderKeys = Reflect.ownKeys(orderObj);
const orderLen = orderKeys.length;

// 10. Reflect.deleteProperty no meio de ops + ownKeys conta correto
const deletable: any = {};
Reflect.set(deletable, "a", 1);
Reflect.set(deletable, "b", 2);
Reflect.set(deletable, "c", 3);
Reflect.deleteProperty(deletable, "b");
const delKeysLen = Reflect.ownKeys(deletable).length;
const delHasA = Reflect.has(deletable, "a");
const delHasB = Reflect.has(deletable, "b");
const delHasC = Reflect.has(deletable, "c");

// 11. defineProperty unrolled (sem array de strings em loop — limitacao
// atual do codegen com indexacao em array<string>).
const config: any = {};
Reflect.defineProperty(config, "debug", { value: 1 });
Reflect.defineProperty(config, "verbose", { value: 2 });
Reflect.defineProperty(config, "timeout", { value: 3 });
Reflect.defineProperty(config, "retries", { value: 4 });
Reflect.defineProperty(config, "host", { value: 5 });
const cfgLen = Reflect.ownKeys(config).length;
const cfgHasHost = Reflect.has(config, "host");

// 12. Pattern: "merge" via Reflect — copia keys de a pra b (unrolled).
const src: any = { x: 10, y: 20, z: 30 };
const dst: any = { w: 99 };
Reflect.set(dst, "x", Reflect.get(src, "x"));
Reflect.set(dst, "y", Reflect.get(src, "y"));
Reflect.set(dst, "z", Reflect.get(src, "z"));
const dstLen = Reflect.ownKeys(dst).length;
const dstHasW = Reflect.has(dst, "w");
const dstHasX = Reflect.has(dst, "x");

describe("reflect_with_classes", () => {
    // 1
    test("class instance value via direct access", () => expect(cValue).toBe(2));
    test("Reflect.has on class instance", () => expect(cHasValue).toBe(true));
    // 2
    test("subclass inherits parent field", () => expect(cbW).toBe(10));
    test("subclass own field visible", () => expect(cbHasColor).toBe(true));
    test("subclass missing field absent", () => expect(cbHasMissing).toBe(false));
    // 3
    test("Reflect.set updates class field", () => expect(pAge).toBe(31));
    // 4
    test("observer pattern stateX present", () => expect(stateX).toBe(true));
    test("observer pattern stateY present", () => expect(stateY).toBe(true));
    test("observer pattern lastChange", () => expect(lastChange).toBe("y"));
    // 5
    test("defineProperty x3 + ownKeys", () => expect(dynKeys).toBe(3));
    // 6
    test("apply pow(2,3)", () => expect(pow23).toBe(8));
    test("apply pow(5,2)", () => expect(pow52).toBe(25));
    test("apply pow(10,0)", () => expect(pow10).toBe(1));
    // 7
    test("class field descriptor value", () => expect(hDescValue).toBe(7));
    test("class field descriptor configurable", () => expect(hDescConfigStr).toBe("true"));
    test("class field descriptor enumerable", () => expect(hDescEnumStr).toBe("true"));
    // 8
    test("has before defineProperty", () => expect(beforeDef).toBe(false));
    test("has after defineProperty", () => expect(afterDef).toBe(true));
    test("get after defineProperty", () => expect(lazyX).toBe(100));
    // 9
    test("ownKeys length after sequential set", () => expect(orderLen).toBe(3));
    // 10
    test("ownKeys after delete count", () => expect(delKeysLen).toBe(2));
    test("delete preserves a", () => expect(delHasA).toBe(true));
    test("delete removes b", () => expect(delHasB).toBe(false));
    test("delete preserves c", () => expect(delHasC).toBe(true));
    // 11
    test("loop defineProperty count", () => expect(cfgLen).toBe(5));
    test("loop defineProperty has host key", () =>
        expect(cfgHasHost).toBe(true));
    // 12
    test("merge via Reflect dst length", () => expect(dstLen).toBe(4));
    test("merge preserves dst original", () => expect(dstHasW).toBe(true));
    test("merge adds src key", () => expect(dstHasX).toBe(true));
});
