// Cross-runtime: `new String(x)` is a String exotic object — its index
// properties are own, enumerable, NON-writable and NON-configurable, `length` is
// own and hidden, and it compares == but never === to the primitive. 177/244 only
// call valueOf on one. Property writes go through Reflect.set so the result does
// not depend on strict mode.

const w: any = new String("ab");

// --- identity and coercion ---
console.log("typeof=" + typeof w);
console.log("typeof-prim=" + typeof "ab");
console.log("loose-eq=" + (w == "ab"));
console.log("strict-eq=" + ((w as any) === "ab"));
console.log("self-eq=" + (w == new String("ab")));
console.log("valueOf=" + w.valueOf());
console.log("toString=" + w.toString());
console.log("tag=" + Object.prototype.toString.call(w));
console.log("instanceof=" + (w instanceof String));
console.log("proto-is-String=" + (Object.getPrototypeOf(w) === String.prototype));
console.log("concat=" + ("x" + w));
console.log("template=" + `${w}`);
console.log("json=" + JSON.stringify(w));
console.log("json-nested=" + JSON.stringify({ a: new String("z") }));

// --- indices are own properties, length is too ---
console.log("idx0=" + w[0]);
console.log("idx1=" + w[1]);
console.log("idx2=" + String(w[2]));
console.log("length=" + w.length);
console.log("keys=" + Object.keys(w).join(","));
console.log("gopn=" + Object.getOwnPropertyNames(w).join(","));
console.log("has-0=" + ("0" in w));
console.log("has-2=" + ("2" in w));
console.log("hasOwn-length=" + Object.prototype.hasOwnProperty.call(w, "length"));

// --- descriptors: indices are enumerable but frozen in place ---
const d0: any = Object.getOwnPropertyDescriptor(w, "0");
console.log("desc0=" + d0.value + "/" + d0.writable + "/" + d0.enumerable + "/" + d0.configurable);
const dl: any = Object.getOwnPropertyDescriptor(w, "length");
console.log("desc-length=" + dl.value + "/" + dl.writable + "/" + dl.enumerable + "/" + dl.configurable);

// --- writing an index is refused; a NEW property is accepted ---
console.log("set-idx=" + Reflect.set(w, "0", "z"));
console.log("idx0-after=" + w[0]);
console.log("set-length=" + Reflect.set(w, "length", 9));
console.log("length-after=" + w.length);
console.log("set-new=" + Reflect.set(w, "extra", 7));
console.log("extra=" + w.extra);
console.log("keys-after=" + Object.keys(w).join(","));
console.log("delete-idx=" + Reflect.deleteProperty(w, "0"));
console.log("delete-extra=" + Reflect.deleteProperty(w, "extra"));

// --- extensibility: a fresh wrapper is extensible and NOT frozen ---
console.log("extensible=" + Object.isExtensible(new String("ab")));
console.log("frozen=" + Object.isFrozen(new String("ab")));
console.log("frozen-empty=" + Object.isFrozen(new String("")));
console.log("sealed-empty=" + Object.isSealed(Object.preventExtensions(new String(""))));

// --- for-in walks the index keys, spread walks code points ---
const seen: string[] = [];
for (const k in new String("abc")) seen.push(k);
console.log("forin=" + seen.join(","));
console.log("spread=" + [...(new String("ab") as any)].join("|"));
console.log("from=" + Array.from(new String("ab") as any).join("|"));

// --- String() without new returns a primitive ---
console.log("callable=" + typeof String("ab"));
console.log("callable-eq=" + (String("ab") === "ab"));
console.log("wrap-of-empty-len=" + new String("").length);
console.log("wrap-of-number=" + new String(42).valueOf());

// --- boxing an object twice does not double-wrap ---
console.log("rewrap=" + new String(new String("ab") as any).valueOf());
console.log("wrap-in-cond=" + (new String("") ? "truthy" : "falsy"));
