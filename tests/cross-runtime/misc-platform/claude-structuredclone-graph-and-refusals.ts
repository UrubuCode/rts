// ONE thing: what structuredClone preserves about a GRAPH — shared references
// stay shared, cycles survive, prototypes do not — and which values it refuses.
const shared = { id: 1 };
const graph: any = { a: shared, b: shared, list: [shared] };
graph.self = graph;

const c = structuredClone(graph);
console.log("sharedPreserved=" + (c.a === c.b) + " inList=" + (c.list[0] === c.a));
console.log("notSameAsSource=" + (c.a !== shared));
console.log("cycle=" + (c.self === c) + " depth=" + (c.self.self.a.id));

// Built-ins that survive with their identity as a TYPE.
const d = new Date(0);
const r = /ab+c/gi;
const m = new Map<any, any>([["k", { v: 1 }]]);
const s = new Set([1, 2, 2]);
const cl = structuredClone({ d, r, m, s });
console.log("date=" + (cl.d instanceof Date) + " iso=" + cl.d.toISOString());
console.log("regexp=" + (cl.r instanceof RegExp) + " src=" + cl.r.source + " flags=" + cl.r.flags + " lastIndex=" + cl.r.lastIndex);
console.log("map=" + (cl.m instanceof Map) + " size=" + cl.m.size + " deep=" + (cl.m.get("k") !== m.get("k")));
console.log("set=" + (cl.s instanceof Set) + " size=" + cl.s.size);

// Typed arrays and buffers keep their kind and their bytes.
const ta = new Uint16Array([1, 2, 3]);
const ct = structuredClone(ta);
console.log("typed=" + (ct instanceof Uint16Array) + " v=" + Array.from(ct).join(",") + " detached=" + (ta.length === 3));
const ab = new Uint8Array([9, 8]).buffer;
const cab = structuredClone(ab);
console.log("buffer=" + (cab instanceof ArrayBuffer) + " len=" + cab.byteLength + " srcAlive=" + (ab.byteLength === 2));

// An Error clones as an Error with name and message.
const err = structuredClone(new RangeError("boom"));
console.log("error=" + (err instanceof RangeError) + " name=" + err.name + " msg=" + err.message);

// Primitives and wrappers.
console.log("primitives=" + [structuredClone(0), structuredClone(""), String(structuredClone(null)), String(structuredClone(undefined)), structuredClone(true)].join("|"));
console.log("negZero=" + Object.is(structuredClone(-0), -0));
console.log("bigint=" + structuredClone(2n ** 70n));
console.log("boxed=" + (structuredClone(new Number(5)) instanceof Number));

// The PROTOTYPE is not preserved: a class instance comes back as a plain object.
class Point { x = 1; y = 2; sum() { return this.x + this.y; } }
const cp: any = structuredClone(new Point());
console.log("proto=" + (cp instanceof Point) + " plain=" + (Object.getPrototypeOf(cp) === Object.prototype));
console.log("fields=" + cp.x + "," + cp.y + " method=" + (typeof cp.sum));

// Accessors are flattened to their current value; non-enumerable own props are dropped.
const acc: any = { get computed() { return 7; } };
Object.defineProperty(acc, "hidden", { value: 1, enumerable: false });
const cacc = structuredClone(acc);
const dsc: any = Object.getOwnPropertyDescriptor(cacc, "computed");
console.log("accessorFlattened=" + (dsc && "value" in dsc) + " v=" + cacc.computed);
console.log("hiddenDropped=" + !("hidden" in cacc));

// Symbol keys are dropped.
const sym = Symbol("s");
const withSym: any = { plain: 1 };
withSym[sym] = 2;
console.log("symbolDropped=" + (Object.getOwnPropertySymbols(structuredClone(withSym)).length === 0));

// What it refuses.
function refuse(label: string, v: any) {
  try { structuredClone(v); console.log(label + "=cloned"); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
refuse("function", () => 1);
refuse("symbol", Symbol("x"));
refuse("nestedFunction", { f() {} });
refuse("weakmap", new WeakMap());
refuse("proxyOfPlain", new Proxy({ a: 1 }, {}));
