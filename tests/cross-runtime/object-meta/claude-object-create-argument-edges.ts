// Pins Object.create's two arguments at their edges: the prototype must be an
// object or null (undefined is NOT accepted), and the properties map is read as
// own ENUMERABLE keys including symbols, with every omitted attribute defaulting
// to false. 387 uses only well-formed arguments.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

// the prototype argument
console.log("null_proto=" + Object.getPrototypeOf(Object.create(null)));
console.log("object_proto=" + (Object.getPrototypeOf(Object.create(Object.prototype)) === Object.prototype));
console.log("fn_proto=" + (Object.getPrototypeOf(Object.create(Math.max as any)) === Math.max));
console.log("array_proto=" + (Object.getPrototypeOf(Object.create([] as any)) === Array.prototype ? "no" : "the-array-itself"));
attempt("undefined_proto", () => String(Object.create(undefined as any)));
attempt("number_proto", () => String(Object.create(7 as any)));
attempt("string_proto", () => String(Object.create("s" as any)));
attempt("boolean_proto", () => String(Object.create(true as any)));
attempt("symbol_proto", () => String(Object.create(Symbol("s") as any)));

// no second argument, or undefined, means "no properties"
console.log("no_props=" + Reflect.ownKeys(Object.create(null)).length);
console.log("undef_props=" + Reflect.ownKeys(Object.create(null, undefined)).length);
attempt("null_props", () => String(Reflect.ownKeys(Object.create(null, null as any)).length));
attempt("number_props", () => String(Reflect.ownKeys(Object.create(null, 7 as any)).length));
console.log("string_props=" + (() => {
  try { return Reflect.ownKeys(Object.create(null, "ab" as any)).join("|"); } catch (e: any) { return "throw:" + e.constructor.name; }
})());

// every omitted attribute defaults to false
const defaults: any = Object.create(null, { k: { value: 1 } });
const dd = Object.getOwnPropertyDescriptor(defaults, "k") as any;
console.log("defaults=w=" + dd.writable + ",e=" + dd.enumerable + ",c=" + dd.configurable);
console.log("defaults_keys=" + Object.keys(defaults).length + ",ownkeys=" + Reflect.ownKeys(defaults).length);

// a symbol key in the map lands as a symbol property
const sk = Symbol("sk");
const withSym: any = Object.create(null, { [sk]: { value: "S", enumerable: true } } as any);
console.log("sym_key=" + withSym[sk] + ",symbols=" + Object.getOwnPropertySymbols(withSym).map(String).join("|"));

// a NON-ENUMERABLE entry in the map is skipped entirely
const map: any = { visible: { value: 1, enumerable: true } };
Object.defineProperty(map, "skipped", { value: { value: 2, enumerable: true }, enumerable: false });
console.log("map_filter=" + Reflect.ownKeys(Object.create(null, map)).join("|"));

// an INHERITED entry in the map is skipped too
const inheritedMap: any = Object.create({ fromProto: { value: 3, enumerable: true } });
inheritedMap.own = { value: 4, enumerable: true };
console.log("map_inherited=" + Reflect.ownKeys(Object.create(null, inheritedMap)).join("|"));

// the map's values are read through getters, once each, in own-key order
const reads: string[] = [];
const getterMap: any = {};
Object.defineProperty(getterMap, "b", { get() { reads.push("b"); return { value: 2, enumerable: true }; }, enumerable: true });
Object.defineProperty(getterMap, "1", { get() { reads.push("1"); return { value: 1, enumerable: true }; }, enumerable: true });
Object.defineProperty(getterMap, "a", { get() { reads.push("a"); return { value: 3, enumerable: true }; }, enumerable: true });
const built: any = Object.create(null, getterMap);
console.log("map_reads=" + reads.join("|"));
console.log("map_result=" + Reflect.ownKeys(built).join("|"));

// a descriptor value that is not an object is refused
attempt("desc_primitive", () => String(Reflect.ownKeys(Object.create(null, { k: 1 } as any)).length));
attempt("desc_null", () => String(Reflect.ownKeys(Object.create(null, { k: null } as any)).length));

// the created object is extensible and its prototype is mutable
const child: any = Object.create(null, { k: { value: 1, writable: true, configurable: true } });
console.log("extensible=" + Object.isExtensible(child));
child.added = 2;
console.log("added=" + Reflect.ownKeys(child).join("|"));
console.log("setproto=" + Reflect.setPrototypeOf(child, Object.prototype) + ",toString=" + typeof child.toString);

// Object.create(null) really has nothing, not even the __proto__ accessor
const bare: any = Object.create(null);
console.log("bare_ownkeys=" + Reflect.ownKeys(bare).length);
console.log("bare_proto_desc=" + Object.getOwnPropertyDescriptor(bare, "__proto__"));
console.log("bare_in=" + ("toString" in bare) + "," + ("__proto__" in bare));
console.log("bare_json=" + JSON.stringify(Object.assign(bare, { a: 1 })));
attempt("bare_tostring", () => String(bare + ""));
console.log("bare_tag=" + Object.prototype.toString.call(bare));

// the properties map may itself be a PROXY
const traps: string[] = [];
const proxyMap: any = new Proxy({ p: { value: 9, enumerable: true } } as any, {
  ownKeys(t) { traps.push("ownKeys"); return Reflect.ownKeys(t); },
  getOwnPropertyDescriptor(t, k) { traps.push("gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
  get(t, k, r) { traps.push("get:" + String(k)); return Reflect.get(t, k, r); },
});
const fromProxy: any = Object.create(null, proxyMap);
console.log("proxy_map_result=" + fromProxy.p);
console.log("proxy_map_traps=" + traps.join("|"));
