// Cross-runtime: String.prototype methods are GENERIC — they run RequireObject-
// Coercible on `this` and then ToString it, so .call(42) works, .call(null)
// throws a TypeError and .call(aSymbol) throws too (a Symbol is coercible but
// ToString refuses it). Nothing in the corpus detaches a String method.

function attempt(fn: () => any): string {
  try {
    const v = fn();
    return v === undefined ? "undefined" : String(v);
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

const upper = String.prototype.toUpperCase;
const slice = String.prototype.slice;
const idx = String.prototype.indexOf;
const at = String.prototype.at;
const repl = String.prototype.replace;

// --- primitives that coerce cleanly ---
console.log("num=" + attempt(() => upper.call(42 as any)));
console.log("num-float=" + attempt(() => slice.call(3.5 as any, 1)));
console.log("bool=" + attempt(() => upper.call(true as any)));
console.log("bigint=" + attempt(() => slice.call(10n as any, 0, 1)));
console.log("bool-charAt=" + attempt(() => String.prototype.charAt.call(false as any, 1)));

// --- objects go through ToString, i.e. toString/valueOf ---
console.log("array=" + attempt(() => idx.call([1, 2, 3] as any, ",")));
console.log("array-upper=" + attempt(() => upper.call(["a", "b"] as any)));
console.log("plain-obj=" + attempt(() => upper.call({} as any)));
console.log("custom=" + attempt(() => upper.call({ toString() { return "hi"; } } as any)));
console.log("valueof-only=" + attempt(() => upper.call({ valueOf() { return "vo"; } } as any)));
console.log("date-len=" + attempt(() => String.prototype.slice.call(new Date(0) as any, 0, 0).length));
console.log("regexp=" + attempt(() => idx.call(/ab/g as any, "b")));
console.log("fn-has-fn=" + attempt(() => String.prototype.includes.call(function f() {} as any, "f")));

// --- a String wrapper unwraps to its primitive ---
console.log("wrapper=" + attempt(() => upper.call(new String("ab") as any)));
console.log("wrapper-len=" + attempt(() => slice.call(new String("abc") as any, 1)));

// --- nullish `this` is a TypeError before any coercion happens ---
console.log("null=" + attempt(() => upper.call(null as any)));
console.log("undefined=" + attempt(() => upper.call(undefined as any)));
console.log("noarg=" + attempt(() => (0, String.prototype.trim)()));
console.log("null-at=" + attempt(() => at.call(null as any, 0)));

// --- a Symbol `this` reaches ToString and is refused there ---
console.log("symbol=" + attempt(() => upper.call(Symbol("s") as any)));
console.log("symbol-slice=" + attempt(() => slice.call(Symbol.iterator as any, 0)));

// --- an object whose toString throws propagates that error ---
console.log("throwing=" + attempt(() => upper.call({ toString() { throw new RangeError("x"); } } as any)));
console.log("no-tostring=" + attempt(() => upper.call(Object.create(null) as any)));

// --- the ARGUMENTS are coerced independently of `this` ---
console.log("arg-obj=" + attempt(() => idx.call("a,b" as any, { toString() { return ","; } } as any)));
console.log("arg-symbol=" + attempt(() => idx.call("ab" as any, Symbol("s") as any)));
console.log("arg-null=" + attempt(() => idx.call("anull" as any, null as any)));

// --- replace on a non-string `this` still sees the coerced text ---
console.log("replace-num=" + attempt(() => repl.call(12321 as any, /2/g, "-")));
console.log("replace-arr=" + attempt(() => repl.call([1, 2] as any, ",", ";")));

// --- static String methods ignore `this` entirely ---
const fcc = String.fromCharCode;
const fcp = String.fromCodePoint;
const raw = String.raw;
console.log("fromCharCode-detached=" + attempt(() => fcc(65, 66)));
console.log("fromCharCode-oncall=" + attempt(() => fcc.call(null as any, 67)));
console.log("fromCodePoint-detached=" + attempt(() => fcp(0x1f600).length));
console.log("raw-detached=" + attempt(() => raw({ raw: ["a", "c"] } as any, "b")));

// --- length is a data property on the prototype, valued 0 ---
console.log("proto-length=" + String.prototype.length);
console.log("proto-typeof=" + typeof String.prototype);
console.log("proto-valueOf=" + attempt(() => String.prototype.valueOf.call({} as any)));
console.log("valueOf-wrapper=" + attempt(() => String.prototype.valueOf.call(new String("z") as any)));
console.log("valueOf-primitive=" + attempt(() => String.prototype.valueOf.call("z" as any)));

// --- and the iterator is generic too ---
console.log("iter-num=" + attempt(() =>
  [...(String.prototype[Symbol.iterator].call(123 as any) as any)].join("|")));
console.log("iter-null=" + attempt(() => String.prototype[Symbol.iterator].call(null as any)));
