// Cross-runtime: structuredClone over the collection types. A Map/Set survives
// as a Map/Set with its order intact, the graph's internal sharing and cycles
// are preserved, and everything that carries behaviour rather than data is
// refused.

const m = new Map<any, any>([["b", 2], ["a", 1], [3, "three"]]);
const cloneM: any = structuredClone(m);
console.log("map_is_map=" + (cloneM instanceof Map));
console.log("map_distinct=" + (cloneM === m));
console.log("map_size=" + cloneM.size);
console.log("map_order=" + [...cloneM.keys()].join(","));
console.log("map_values=" + [...cloneM.values()].join(","));
console.log("map_key_types=" + [...cloneM.keys()].map((k: any) => typeof k).join(","));
cloneM.set("z", 9);
console.log("source_unchanged=" + m.size + ":" + cloneM.size);

const s = new Set([3, 1, 2, 1]);
const cloneS: any = structuredClone(s);
console.log("set_is_set=" + (cloneS instanceof Set));
console.log("set_order=" + [...cloneS].join(","));
console.log("set_size=" + cloneS.size);

// --- an object KEY is cloned too, so lookups by the original key miss ---
const key = { id: 1 };
const byObject = new Map([[key, "held"]]);
const clonedByObject: any = structuredClone(byObject);
console.log("object_key_miss=" + String(clonedByObject.get(key)));
console.log("object_key_size=" + clonedByObject.size);
const clonedKey = [...clonedByObject.keys()][0];
console.log("object_key_cloned=" + (clonedKey === key) + ":" + (clonedKey as any).id);
console.log("object_key_hit=" + clonedByObject.get(clonedKey));

// --- sharing inside the graph is preserved; two slots stay one object ---
const shared = { n: 1 };
const sharing = new Map<string, any>([["x", shared], ["y", shared]]);
const clonedSharing: any = structuredClone(sharing);
console.log("sharing_preserved=" + (clonedSharing.get("x") === clonedSharing.get("y")));
console.log("sharing_not_original=" + (clonedSharing.get("x") === shared));

// --- a cycle through a Map is fine ---
const cyclic = new Map<string, any>();
cyclic.set("self", cyclic);
cyclic.set("n", 5);
const clonedCyclic: any = structuredClone(cyclic);
console.log("cycle_self=" + (clonedCyclic.get("self") === clonedCyclic));
console.log("cycle_other=" + clonedCyclic.get("n"));

const cyclicSet = new Set<any>();
cyclicSet.add(cyclicSet);
const clonedCyclicSet: any = structuredClone(cyclicSet);
console.log("set_cycle=" + clonedCyclicSet.has(clonedCyclicSet) + ":" + clonedCyclicSet.size);

// --- nesting: a Map of Sets of Maps ---
const nested = new Map<string, any>([["outer", new Set([new Map([["deep", 1]])])]]);
const clonedNested: any = structuredClone(nested);
const inner = [...clonedNested.get("outer")][0];
console.log("nested=" + (inner instanceof Map) + ":" + inner.get("deep"));

// --- data-carrying built-ins survive; behaviour-carrying ones do not ---
function attempt(label: string, value: any): void {
  try {
    const c: any = structuredClone(value);
    console.log(label + "=ok:" + Object.prototype.toString.call(c));
  } catch (e: any) {
    console.log(label + "=" + e.constructor.name + ":" + e.name);
  }
}
attempt("date", new Date(0));
attempt("regexp", /ab+c/gi);
attempt("error", new TypeError("x"));
attempt("arraybuffer", new ArrayBuffer(4));
attempt("uint8", new Uint8Array([1, 2]));
attempt("bigint", 10n);
attempt("boxed_number", new Number(1));
attempt("boxed_string", new String("s"));
attempt("null_proto", Object.create(null));
attempt("array", [1, [2]]);
attempt("weakmap", new WeakMap());
attempt("weakset", new WeakSet());
attempt("function", function f() { /* not cloneable */ });
attempt("symbol", Symbol("s"));
attempt("map_with_function_value", new Map([["f", function () { /* nope */ }]]));
attempt("set_with_symbol", new Set([Symbol("s")]));
attempt("promise", Promise.resolve(1));
attempt("proxy_of_object", new Proxy({ a: 1 }, {}));

// --- a Date and a RegExp inside a Map keep their kind ---
const rich = new Map<string, any>([["d", new Date(86400000)], ["r", /x/g]]);
const clonedRich: any = structuredClone(rich);
console.log("rich_date=" + (clonedRich.get("d") instanceof Date) + ":" + clonedRich.get("d").toISOString());
console.log("rich_regexp=" + (clonedRich.get("r") instanceof RegExp) + ":" + clonedRich.get("r").source + ":" + clonedRich.get("r").flags);

// --- a class instance is cloned as a plain object, losing its prototype ---
class Point {
  x = 1;
  y = 2;
  sum(): number { return this.x + this.y; }
}
const clonedPoint: any = structuredClone(new Point());
console.log("class_plain=" + (clonedPoint instanceof Point) + ":" + (Object.getPrototypeOf(clonedPoint) === Object.prototype));
console.log("class_fields=" + clonedPoint.x + "," + clonedPoint.y + ":method=" + typeof clonedPoint.sum);

const classInMap: any = structuredClone(new Map([["p", new Point()]]));
console.log("class_in_map=" + (classInMap.get("p") instanceof Point) + ":" + classInMap.get("p").x);

// --- getters are read and stored as data; non-enumerable own props are dropped ---
const shaped: any = { get computed() { return 42; } };
Object.defineProperty(shaped, "hidden", { value: 7, enumerable: false });
const clonedShaped: any = structuredClone(new Map([["o", shaped]])).get("o");
console.log("getter_becomes_data=" + clonedShaped.computed + ":" + typeof (Object.getOwnPropertyDescriptor(clonedShaped, "computed") as any).get);
console.log("hidden_dropped=" + ("hidden" in clonedShaped));

// --- a symbol-keyed entry on a cloned object is dropped, a symbol KEY throws ---
const withSym: any = { plain: 1 };
withSym[Symbol("k")] = 2;
const clonedWithSym: any = structuredClone(withSym);
console.log("symbol_prop_dropped=" + Object.getOwnPropertySymbols(clonedWithSym).length + ":" + clonedWithSym.plain);

// --- -0 and NaN keys survive the round trip as themselves ---
const numeric = new Map<any, string>([[-0, "neg"], [NaN, "nan"]]);
const clonedNumeric: any = structuredClone(numeric);
const clonedKeys = [...clonedNumeric.keys()];
console.log("neg_zero_normalised=" + (1 / (clonedKeys[0] as number)));
console.log("nan_key=" + Number.isNaN(clonedKeys[1] as number) + ":" + clonedNumeric.get(NaN));
