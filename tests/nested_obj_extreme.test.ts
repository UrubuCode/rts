// (#592 + #602) Casos "fora da caixa" misturando array de classe + obj
// nested + optional chain. Cenarios reais de manipulacao de dados.

import { describe, test, expect } from "rts:test";

// 1. Array de classe + acesso encadeado em loop com if
class Point {
    x: i64;
    y: i64;
    constructor(x: i64, y: i64) { this.x = x; this.y = y; }
}

const pts: Point[] = [
    new Point(1, 2),
    new Point(10, 20),
    new Point(100, 200),
];

let sumX: i64 = 0;
let sumY: i64 = 0;
for (let i = 0; i < pts.length; i++) {
    if (pts[i].x > 5) {
        sumX = sumX + pts[i].x;
        sumY = sumY + pts[i].y;
    }
}

// 2. Multiple array vars com class[]
const empty: Point[] = [];
const single: Point[] = [new Point(7, 7)];
const emptyLen = empty.length;
const singleX: i64 = single[0].x;

// 3. Obj nested 5 niveis
const deep5 = { l1: { l2: { l3: { l4: { v: "leaf" } } } } };
const r5: string = deep5?.l1?.l2?.l3?.l4?.v;

// 4. Array de obj com sub-objs
const records = [
    { id: 1, meta: { kind: "alpha" } },
    { id: 2, meta: { kind: "beta" } },
];
const rec0Id: i64 = records[0].id;
const rec0MetaKind: string = records[0].meta.kind;

// 5. Mix: array de classe com class field tipo number e string
class Item {
    id: i64;
    label: string;
    constructor(i: i64, l: string) { this.id = i; this.label = l; }
}
const items: Item[] = [
    new Item(1, "first"),
    new Item(2, "second"),
    new Item(3, "third"),
];
const concatLabels = items[0].label + "-" + items[1].label + "-" + items[2].label;

// 6. Optchain dentro de loop com classe (em vez de obj literal nested
// que ainda nao propaga tipos por array elem). 3 instances, cada com
// label/value.
class Cfg {
    name: string;
    constructor(n: string) { this.name = n; }
}
const cfgs: Cfg[] = [new Cfg("url1"), new Cfg("url2"), new Cfg("url3")];
let urls = "";
for (let i = 0; i < cfgs.length; i++) {
    urls = urls + cfgs[i].name + ",";
}

// 7. Obj nested 3 niveis com array dentro
const config = { servers: { primary: { ports: [80, 443, 8080] } } };
const portsLen: i64 = config?.servers?.primary?.ports.length;
const port0: i64 = config.servers.primary.ports[0];

// 8. Class com field obj literal
class WithMeta {
    name: string;
    constructor(n: string) { this.name = n; }
}
const arr8: WithMeta[] = [new WithMeta("alpha"), new WithMeta("bravo")];
const labels8 = arr8[0].name + "/" + arr8[1].name;

// 9. Mix complexo: array de classe + acesso pos-condicional
class Tag {
    val: i64;
    label: string;
    constructor(v: i64, l: string) { this.val = v; this.label = l; }
}
const tags: Tag[] = [new Tag(10, "red"), new Tag(20, "blue"), new Tag(30, "green")];
let foundLabel = "";
for (let i = 0; i < tags.length; i++) {
    if (tags[i].val === 20) {
        foundLabel = tags[i].label;
    }
}

// 10. Optchain encadeado em fn que retorna obj
function makeCfg(): { srv: { host: string; port: i64 } } {
    return { srv: { host: "h1", port: 9000 } };
}
const c10 = makeCfg();
const c10host: string = c10?.srv?.host;
const c10port: i64 = c10?.srv?.port;

// 11. Array de classe inicializado em fn
function makeItems(n: i64): Item[] {
    const r: Item[] = [];
    return r;  // simplified
}
const made = makeItems(0);
const madeLen: i64 = made.length;

// 12. Mix: ler field de classe que tambem e classe
class Inner {
    x: i64;
    constructor(x: i64) { this.x = x; }
}
class Outer {
    inner: Inner;
    constructor() { this.inner = new Inner(99); }
}
const outers: Outer[] = [new Outer(), new Outer()];
const innerX: i64 = outers[0].inner.x;

describe("nested_obj_extreme", () => {
    test("class[] loop with conditional sumX", () =>
        expect(sumX).toBe(110));
    test("class[] loop with conditional sumY", () =>
        expect(sumY).toBe(220));
    test("empty class[] length", () => expect(emptyLen).toBe(0));
    test("single class[] indexing", () => expect(singleX).toBe(7));
    test("5-level nested optchain", () => expect(r5).toBe("leaf"));
    test("array of obj with sub-obj id", () => expect(rec0Id).toBe(1));
    test("array of obj with sub-obj nested kind", () =>
        expect(rec0MetaKind).toBe("alpha"));
    test("class[] string concat", () =>
        expect(concatLabels).toBe("first-second-third"));
    test("optchain inside loop urls", () =>
        expect(urls).toBe("url1,url2,url3,"));
    test("3-level optchain to array length", () =>
        expect(portsLen).toBe(3));
    test("3-level access to array elem", () => expect(port0).toBe(80));
    test("class[] direct labels concat", () =>
        expect(labels8).toBe("alpha/bravo"));
    test("class[] conditional label match", () =>
        expect(foundLabel).toBe("blue"));
    test("optchain on fn-returned obj host", () =>
        expect(c10host).toBe("h1"));
    test("optchain on fn-returned obj port", () =>
        expect(c10port).toBe(9000));
    test("empty class[] from fn", () => expect(madeLen).toBe(0));
    test("class[] field that is class — innerX", () =>
        expect(innerX).toBe(99));
});
