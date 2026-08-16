// Cross-runtime: structuredClone copies a graph rather than a tree -- shared
// references stay shared, cycles survive, and Map/Set/Date/RegExp/TypedArray
// come back as themselves. Focus: identity, not just value.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

log("hasStructuredClone=" + (typeof structuredClone));

// 1) a plain object is a copy, not the same object
const src = { a: 1, b: "two", c: true, d: null, e: [1, 2, 3] };
const cp: any = structuredClone(src);
log("notSame=" + (cp !== src));
log("keys=" + Object.keys(cp).join(","));
log("values=" + cp.a + "," + cp.b + "," + cp.c + "," + String(cp.d) + "," + cp.e.join("-"));
log("arrayIsArray=" + Array.isArray(cp.e) + " arrayNotSame=" + (cp.e !== src.e));
log("proto=" + (Object.getPrototypeOf(cp) === Object.prototype));

// 2) a shared reference stays ONE object on the other side
const shared = { tag: "shared" };
const graph: any = structuredClone({ left: shared, right: shared });
log("sharedStaysShared=" + (graph.left === graph.right));
log("sharedIsCopy=" + (graph.left !== shared));

// 3) a cycle survives
const cyc: any = { name: "root" };
cyc.self = cyc;
cyc.kids = [cyc, { parent: cyc }];
const c2: any = structuredClone(cyc);
log("cycleSelf=" + (c2.self === c2));
log("cycleThroughArray=" + (c2.kids[0] === c2));
log("cycleThroughChild=" + (c2.kids[1].parent === c2));
log("cycleName=" + c2.name);

// 4) Map and Set keep type, order and structure
const m = new Map<any, any>([["k1", 1], ["k2", { deep: true }]]);
m.set(shared, "byObjectKey");
const mc: any = structuredClone(m);
log("mapIsMap=" + (mc instanceof Map) + " size=" + mc.size);
log("mapKeys=" + Array.from(mc.keys()).map(function (k: any) { return typeof k === "string" ? k : "obj"; }).join(","));
log("mapDeep=" + mc.get("k2").deep + " deepNotSame=" + (mc.get("k2") !== m.get("k2")));

const s = new Set([3, 1, 2, "x"]);
const sc: any = structuredClone(s);
log("setIsSet=" + (sc instanceof Set) + " size=" + sc.size);
log("setOrder=" + Array.from(sc).join(","));

// 5) Date and RegExp
const d = new Date(0);
const dc: any = structuredClone(d);
log("dateIsDate=" + (dc instanceof Date) + " time=" + dc.getTime() + " notSame=" + (dc !== d));

const re = /ab+c/gimsu;
const rc: any = structuredClone(re);
log("regexpIsRegExp=" + (rc instanceof RegExp));
log("regexpSource=" + rc.source + " flags=" + rc.flags);

// 6) typed arrays and ArrayBuffer
const ta = new Uint16Array([1, 2, 65535]);
const tc: any = structuredClone(ta);
log("typedIs=" + (tc instanceof Uint16Array) + " len=" + tc.length + " vals=" + Array.from(tc).join(","));
log("typedBufferNotSame=" + (tc.buffer !== ta.buffer) + " byteLength=" + tc.buffer.byteLength);

// 7) an Error clones as an Error of the same kind
const err: any = structuredClone(new RangeError("x"));
log("errorIsError=" + (err instanceof Error) + " name=" + err.name + " ctor=" + err.constructor.name);

// 8) BigInt and primitives round-trip
log("bigint=" + structuredClone(9007199254740993n).toString());
log("negZero=" + Object.is(structuredClone(-0), -0));
log("nan=" + Number.isNaN(structuredClone(NaN)));
log("undef=" + String(structuredClone(undefined)));

// 9) a function is NOT cloneable
log("functionThrows=" + (function () {
  try { structuredClone(function () { }); return "no"; } catch (e: any) { return e.name; }
})());
log("symbolThrows=" + (function () {
  try { structuredClone(Symbol("s")); return "no"; } catch (e: any) { return e.name; }
})());
log("objectWithMethodThrows=" + (function () {
  try { structuredClone({ f: function () { } }); return "no"; } catch (e: any) { return e.name; }
})());

// 10) a class instance loses its prototype but keeps its own data
class Point { x = 1; y = 2; }
const pc: any = structuredClone(new Point());
log("classData=" + pc.x + "," + pc.y);
log("classProtoLost=" + (pc instanceof Point) + " isPlain=" + (Object.getPrototypeOf(pc) === Object.prototype));

// 11) getters are read, not carried across
const withGetter = { plain: 1, get computed() { return 40 + 2; } };
const gc: any = structuredClone(withGetter);
log("getterValue=" + gc.computed);
log("getterBecameData=" + (Object.getOwnPropertyDescriptor(gc, "computed").get === undefined));

console.log("end");
