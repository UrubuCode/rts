// Pins Object.hasOwn as the null-prototype-safe form of hasOwnProperty, and
// what counts as an OWN property on a boxed string (every index plus length)
// versus on a sparse array. 264_object_hasown only walks an ordinary chain.

const bare: any = Object.create(null);
bare.k = 1;
bare["hasOwnProperty"] = "shadowed-but-a-string";

console.log("bare_hasOwn=" + Object.hasOwn(bare, "k"));
try {
  console.log("bare_method=" + bare.hasOwnProperty("k"));
} catch (e: any) {
  console.log("bare_method=throw:" + e.constructor.name);
}
console.log("bare_borrowed=" + Object.prototype.hasOwnProperty.call(bare, "k"));
console.log("bare_shadow_own=" + Object.hasOwn(bare, "hasOwnProperty"));
console.log("bare_toString_own=" + Object.hasOwn(bare, "toString"));

// an ordinary object where hasOwnProperty is shadowed by a non-function
const shadowed: any = { hasOwnProperty: 1, real: 2 };
console.log("shadow_hasOwn=" + Object.hasOwn(shadowed, "real"));
try {
  console.log("shadow_method=" + shadowed.hasOwnProperty("real"));
} catch (e: any) {
  console.log("shadow_method=throw:" + e.constructor.name);
}

// a string: indices and length are own, the methods are not
const s = "abc";
console.log("str_idx0=" + Object.hasOwn(s as any, "0"));
console.log("str_idx2=" + Object.hasOwn(s as any, 2 as any));
console.log("str_idx3=" + Object.hasOwn(s as any, "3"));
console.log("str_length=" + Object.hasOwn(s as any, "length"));
console.log("str_charAt=" + Object.hasOwn(s as any, "charAt"));
console.log("str_in=" + ("0" in Object(s)) + "," + ("charAt" in Object(s)));
console.log("str_names=" + Object.getOwnPropertyNames(Object(s)).join("|"));
const sd = Object.getOwnPropertyDescriptor(Object(s), "0") as any;
console.log("str_idx_desc=v=" + sd.value + ",w=" + sd.writable + ",e=" + sd.enumerable + ",c=" + sd.configurable);
const sl = Object.getOwnPropertyDescriptor(Object(s), "length") as any;
console.log("str_len_desc=v=" + sl.value + ",w=" + sl.writable + ",e=" + sl.enumerable + ",c=" + sl.configurable);
console.log("str_keys=" + Object.keys(Object(s)).join("|"));
console.log("empty_str_names=" + Object.getOwnPropertyNames(Object("")).join("|"));

// other primitives box to objects with no own index properties
console.log("num_names=" + Object.getOwnPropertyNames(Object(7)).length);
console.log("bool_names=" + Object.getOwnPropertyNames(Object(true)).length);
console.log("sym_names=" + Object.getOwnPropertyNames(Object(Symbol("d"))).join("|"));

// hasOwn coerces its first argument, so a primitive works directly
console.log("hasOwn_prim_str=" + Object.hasOwn("ab" as any, "1"));
console.log("hasOwn_prim_num=" + Object.hasOwn(7 as any, "toFixed"));
try {
  console.log("hasOwn_null=" + Object.hasOwn(null as any, "a"));
} catch (e: any) {
  console.log("hasOwn_null=throw:" + e.constructor.name);
}
try {
  console.log("hasOwn_undef=" + Object.hasOwn(undefined as any, "a"));
} catch (e: any) {
  console.log("hasOwn_undef=throw:" + e.constructor.name);
}

// a sparse array: the hole is not own, the length is
const sparse: any = [1, , 3];
console.log("sparse=" + Object.hasOwn(sparse, 0) + "," + Object.hasOwn(sparse, 1) + "," + Object.hasOwn(sparse, 2));
console.log("sparse_len=" + Object.hasOwn(sparse, "length") + ",value=" + sparse.length);
sparse[1] = undefined;
console.log("after_fill=" + Object.hasOwn(sparse, 1) + ",value=" + sparse[1]);
delete sparse[1];
console.log("after_delete=" + Object.hasOwn(sparse, 1));

// the key is coerced the same way property access coerces it
const numKeys: any = { 1: "one" };
console.log("coerce_num=" + Object.hasOwn(numKeys, 1) + "," + Object.hasOwn(numKeys, "1"));
console.log("coerce_negzero=" + Object.hasOwn({ 0: 1 } as any, -0));
const objKey: any = { toString() { return "made"; } };
console.log("coerce_obj=" + Object.hasOwn({ made: 1 } as any, objKey));

// symbols are own keys too
const sym = Symbol("s");
const withSym: any = { [sym]: 1 };
console.log("sym_own=" + Object.hasOwn(withSym, sym) + ",names=" + Object.getOwnPropertyNames(withSym).length);

// a getter-only property is still own
const acc: any = {};
Object.defineProperty(acc, "g", { get() { return 1; } });
console.log("acc_own=" + Object.hasOwn(acc, "g") + ",enumerable=" + Object.keys(acc).length);

// hasOwn on a proxy goes through getOwnPropertyDescriptor, not has
const traps: string[] = [];
const p: any = new Proxy({ x: 1 }, {
  has(t, k) { traps.push("has:" + String(k)); return Reflect.has(t, k); },
  getOwnPropertyDescriptor(t, k) { traps.push("gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
});
console.log("proxy_hasOwn=" + Object.hasOwn(p, "x"));
console.log("proxy_in=" + ("x" in p));
console.log("proxy_traps=" + traps.join("|"));
