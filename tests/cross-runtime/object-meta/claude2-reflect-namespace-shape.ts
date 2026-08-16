// Pins Reflect as a NAMESPACE object rather than a constructor: it is a plain
// object with no [[Call]] or [[Construct]], its thirteen methods each have a
// fixed name and arity, and every one of them refuses a non-object first
// argument with a TypeError.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

console.log("typeof=" + typeof Reflect);
console.log("tag=" + Object.prototype.toString.call(Reflect));
console.log("proto=" + (Object.getPrototypeOf(Reflect) === Object.prototype));
console.log("extensible=" + Object.isExtensible(Reflect));
attempt("call", () => String((Reflect as any)()));
attempt("construct", () => String(new (Reflect as any)()));

const names: string[] = Object.getOwnPropertyNames(Reflect).sort();
console.log("names=" + names.join("|"));
console.log("count=" + names.length);
console.log("symbols=" + Object.getOwnPropertySymbols(Reflect).map(String).join("|"));
console.log("enumerable=" + Object.keys(Reflect).length);
console.log("tag_value=" + (Reflect as any)[Symbol.toStringTag]);

for (const n of names) {
  const fn: any = (Reflect as any)[n];
  const d = Object.getOwnPropertyDescriptor(Reflect, n) as any;
  console.log("m:" + n + "=" + typeof fn + ",name=" + fn.name + ",len=" + fn.length +
    ",w=" + d.writable + ",e=" + d.enumerable + ",c=" + d.configurable +
    ",proto=" + (Object.getPrototypeOf(fn) === Function.prototype) +
    ",hasPrototype=" + ("prototype" in fn));
}

// none of them is constructable
for (const n of names) {
  try {
    new ((Reflect as any)[n])({}, "k");
    console.log("new:" + n + "=ok");
  } catch (e: any) {
    console.log("new:" + n + "=throw:" + e.constructor.name);
  }
}

// each object-taking method refuses a primitive, where the Object twin coerces
attempt("get_primitive", () => String(Reflect.get(1 as any, "toFixed")));
attempt("set_primitive", () => String(Reflect.set("s" as any, "0", "x")));
attempt("has_primitive", () => String(Reflect.has(1 as any, "x")));
attempt("ownKeys_primitive", () => Reflect.ownKeys("ab" as any).join("|"));
attempt("getproto_primitive", () => String(Reflect.getPrototypeOf("ab" as any)));
attempt("gopd_primitive", () => String(Reflect.getOwnPropertyDescriptor("ab" as any, "0")));
attempt("delete_primitive", () => String(Reflect.deleteProperty(1 as any, "x")));
attempt("isExtensible_primitive", () => String(Reflect.isExtensible(1 as any)));
attempt("preventExtensions_primitive", () => String(Reflect.preventExtensions(1 as any)));
attempt("defineProperty_primitive", () => String(Reflect.defineProperty(1 as any, "x", { value: 1 })));
attempt("setproto_primitive", () => String(Reflect.setPrototypeOf(1 as any, null)));
attempt("apply_nonfn", () => String(Reflect.apply({} as any, null, [])));
attempt("construct_nonfn", () => String(Reflect.construct({} as any, [])));
// the Object statics coerce the same primitives instead
console.log("object_keys_string=" + Object.keys("ab").join("|"));
console.log("object_getproto_string=" + (Object.getPrototypeOf("ab") === String.prototype));
console.log("object_isExtensible_number=" + Object.isExtensible(1 as any));

// argument shapes: a missing key becomes "undefined", a symbol stays a symbol
const holder: any = { undefined: "U" };
const symbol = Symbol("k");
holder[symbol] = "S";
console.log("key_missing=" + Reflect.get(holder, undefined as any));
console.log("key_symbol=" + Reflect.get(holder, symbol));
console.log("key_number=" + Reflect.get({ 3: "three" } as any, 3 as any));
console.log("has_undefined_key=" + Reflect.has(holder, undefined as any));
attempt("defineProperty_baddesc", () => String(Reflect.defineProperty({} as any, "x", 1 as any)));
attempt("apply_bad_args", () => String(Reflect.apply(function () { return 1; }, null, 5 as any)));
console.log("apply_missing_args=" + (() => { try { return String((Reflect.apply as any)(function () { return 1; }, null)); } catch (e: any) { return "throw:" + e.constructor.name; } })());
