// ONE thing: Object.prototype.toString over the whole built-in zoo. Some kinds
// answer from an internal slot with NO Symbol.toStringTag at all, some carry a
// string tag, and some carry a GETTER — three different mechanisms behind one
// observable answer.
function tag(label: string, v: any) {
  console.log(label + "=" + Object.prototype.toString.call(v));
}

// The nine slot-driven kinds, which have no toStringTag property.
tag("undefined", undefined);
tag("null", null);
tag("number", 1);
tag("string", "s");
tag("boolean", true);
tag("symbol", Symbol("x"));
tag("bigint", 1n);
tag("object", {});
tag("array", []);
tag("function", function () {});
tag("arrow", () => {});
tag("class", class {});
tag("error", new Error("m"));
tag("typeError", new TypeError("m"));
tag("date", new Date(0));
tag("regexp", /x/);
tag("arguments", (function () { return arguments; })());
tag("boxedNumber", new Number(1));
tag("boxedString", new String("s"));
tag("boxedBoolean", new Boolean(true));
tag("nullProto", Object.create(null));

// The tagged kinds — a plain string-valued Symbol.toStringTag on the prototype.
tag("map", new Map());
tag("set", new Set());
tag("weakMap", new WeakMap());
tag("weakSet", new WeakSet());
tag("promise", Promise.resolve());
tag("symbolWrapper", Object(Symbol("x")));
tag("bigintWrapper", Object(1n));
tag("generatorFn", function* () {});
tag("generatorObj", (function* () {})());
tag("asyncFn", async function () {});
tag("asyncGenFn", async function* () {});
tag("math", Math);
tag("json", JSON);
tag("reflect", Reflect);
tag("arrayIterator", [].values());
tag("stringIterator", ""[Symbol.iterator]());
tag("mapIterator", new Map().values());
tag("setIterator", new Set().values());
tag("regexpStringIterator", "a".matchAll(/a/g));
tag("weakRef", new WeakRef({}));
tag("finalizationRegistry", new FinalizationRegistry(() => {}));

// The getter-driven ones: %TypedArray% and DataView answer from a GETTER that
// returns undefined for a wrong receiver, so the tag falls back to [object Object].
tag("uint8", new Uint8Array(1));
tag("dataView", new DataView(new ArrayBuffer(1)));
tag("arrayBuffer", new ArrayBuffer(1));
const taGetter: any = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(Uint8Array.prototype), Symbol.toStringTag);
console.log("typedArrayTagIsGetter=" + (taGetter ? typeof taGetter.get : "absent"));
console.log("typedArrayTagWrongReceiver=" + String(taGetter && taGetter.get ? taGetter.get.call({}) : "n/a"));

// A user tag on a plain object, on a class, as a getter, and inherited.
const plain: any = {}; plain[Symbol.toStringTag] = "Custom";
tag("userTag", plain);
class Tagged { get [Symbol.toStringTag]() { return "Tagged"; } }
tag("classGetterTag", new Tagged());
const inherited = Object.create(plain);
tag("inheritedTag", inherited);
const numericTag: any = {}; numericTag[Symbol.toStringTag] = 42;
tag("nonStringTag", numericTag);
const throwing: any = {};
Object.defineProperty(throwing, Symbol.toStringTag, { get() { throw new RangeError("boom"); } });
try { Object.prototype.toString.call(throwing); } catch (e: any) { console.log("throwingTag=" + e.constructor.name); }

// Overriding the tag on an EXOTIC kind: the slot-driven ones ignore it, the
// tagged ones follow it. Installed with defineProperty rather than assignment —
// Map.prototype's tag is a NON-WRITABLE data property, so a plain write is
// refused, and whether that refusal throws depends on the mode.
function own(o: any, v: string) { Object.defineProperty(o, Symbol.toStringTag, { value: v, configurable: true }); return o; }
console.log("mapProtoTagWritable=" + Object.getOwnPropertyDescriptor(Map.prototype, Symbol.toStringTag)!.writable);
console.log("assignRefused=" + Reflect.set(new Map(), Symbol.toStringTag, "NotMap"));
tag("arrayWithTag", own([], "NotArray"));
tag("mapWithTag", own(new Map(), "NotMap"));
tag("dateWithTag", own(new Date(0), "NotDate"));
tag("promiseWithTag", own(Promise.resolve(), "NotPromise"));

// A Proxy reports its TARGET's kind for the slot-driven cases.
tag("proxyArray", new Proxy([], {}));
tag("proxyFn", new Proxy(function () {}, {}));
tag("proxyPlain", new Proxy({}, {}));
