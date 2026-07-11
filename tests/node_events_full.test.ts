// node:events — EventEmitter (the engine's canonical global class, reached
// ambiently; node:events registers the module specifier for it). Covers the
// full instance surface including the methods completed on the class.
import { describe, test, expect } from "rts:test";

// --- basic on + emit + arg forwarding ---------------------------------------
let hits = 0;
let lastArg = "";
const e1 = new EventEmitter();
e1.on("ping", (x: string) => { hits = hits + 1; lastArg = x; });
const had1 = e1.emit("ping", "hello");
e1.emit("ping", "world");
const hadNo = e1.emit("absent");

// --- prependListener order (Z before a,b) -----------------------------------
let order = "";
const e2 = new EventEmitter();
e2.on("go", () => { order = order + "a"; });
e2.addListener("go", () => { order = order + "b"; }); // alias
e2.prependListener("go", () => { order = order + "Z"; });
e2.emit("go");

// --- once -------------------------------------------------------------------
let onceHits = 0;
const e3 = new EventEmitter();
e3.once("boom", () => { onceHits = onceHits + 1; });
e3.emit("boom");
e3.emit("boom");

// --- listenerCount / removeListener / listeners -----------------------------
const e4 = new EventEmitter();
const fn = () => {};
e4.on("x", fn);
e4.on("x", () => {});
const cntBefore = e4.listenerCount("x");
const listArr = e4.listeners("x");
const listLen = listArr.length;
e4.removeListener("x", fn); // alias of off
const cntAfter = e4.listenerCount("x");

// --- multi-arg emit ---------------------------------------------------------
let sum = 0;
const e5 = new EventEmitter();
e5.on("nums", (a: i64, b: i64, c: i64) => { sum = a + b + c; });
e5.emit("nums", 10, 20, 12);

// --- max listeners ----------------------------------------------------------
const e6 = new EventEmitter();
const defMax = e6.getMaxListeners();
e6.setMaxListeners(25);
const newMax = e6.getMaxListeners();

// --- eventNames -------------------------------------------------------------
const e7 = new EventEmitter();
e7.on("alpha", () => {});
e7.on("beta", () => {});
const names = e7.eventNames();

describe("node:events EventEmitter", () => {
    test("emit true with listener", () => expect(had1).toBe(true));
    test("emit fires + forwards arg", () => expect(lastArg).toBe("world"));
    test("emit fires each time", () => expect(hits).toBe(2));
    test("emit false no listener", () => expect(hadNo).toBe(false));
    test("prepend + alias order", () => expect(order).toBe("Zab"));
    test("once fires once", () => expect(onceHits).toBe(1));
    test("listenerCount", () => expect(cntBefore).toBe(2));
    test("listeners() length", () => expect(listLen).toBe(2));
    test("removeListener alias", () => expect(cntAfter).toBe(1));
    test("multi-arg emit", () => expect(sum).toBe(42));
    test("getMaxListeners default", () => expect(defMax).toBe(10));
    test("setMaxListeners", () => expect(newMax).toBe(25));
    test("eventNames count", () => expect(names.length).toBe(2));
});
