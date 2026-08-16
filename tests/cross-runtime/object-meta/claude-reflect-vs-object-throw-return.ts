// Pins the systematic split between the Object statics and their Reflect twins:
// Reflect answers a boolean and rejects a non-object argument, while Object
// throws on failure but COERCES a primitive (ES2015 loosened the statics).
// 188/346 call the happy paths only.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

const frozen: any = Object.freeze({ a: 1 });

// defineProperty: throw vs false
attempt("obj_define", () => { Object.defineProperty(frozen, "b", { value: 1 }); return "ok"; });
console.log("ref_define=" + Reflect.defineProperty(frozen, "b", { value: 1 }));
// and the return VALUE differs on success too
const defineTarget: any = {};
console.log("obj_define_ret=" + (Object.defineProperty(defineTarget, "x", { value: 1 }) === defineTarget));
console.log("ref_define_ret=" + Reflect.defineProperty({} as any, "x", { value: 1 }));

// setPrototypeOf: throw vs false
const nonExt: any = Object.preventExtensions({});
attempt("obj_setproto", () => { Object.setPrototypeOf(nonExt, { p: 1 }); return "ok"; });
console.log("ref_setproto=" + Reflect.setPrototypeOf(nonExt, { p: 1 }));
console.log("ref_setproto_same=" + Reflect.setPrototypeOf(nonExt, Object.prototype));

// deleteProperty has no Object twin at all, so the boolean is the only report
console.log("ref_delete_frozen=" + Reflect.deleteProperty(frozen, "a") + ",still=" + frozen.a);
console.log("ref_delete_loose=" + Reflect.deleteProperty({ a: 1 } as any, "a"));
console.log("ref_delete_missing=" + Reflect.deleteProperty(frozen, "zzz"));

// primitives: Object.* coerces (or is the identity), Reflect.* throws
attempt("obj_getproto_num", () => String(Object.getPrototypeOf(7 as any) === Number.prototype));
attempt("ref_getproto_num", () => String(Reflect.getPrototypeOf(7 as any)));
attempt("obj_keys_num", () => String(Object.keys(7 as any).length));
attempt("ref_ownkeys_num", () => String(Reflect.ownKeys(7 as any)));
attempt("obj_freeze_num", () => String(Object.freeze(7 as any)));
attempt("obj_gopn_str", () => Object.getOwnPropertyNames("ab" as any).join("|"));
attempt("obj_gopd_str", () => String((Object.getOwnPropertyDescriptor("ab" as any, "0") as any).value));
attempt("ref_gopd_str", () => String(Reflect.getOwnPropertyDescriptor("ab" as any, "0")));
attempt("obj_setproto_num", () => String(Object.setPrototypeOf(7 as any, null) as any));
attempt("ref_has_num", () => String(Reflect.has(7 as any, "toFixed")));
attempt("obj_isext_num", () => String(Object.isExtensible(7 as any)));
attempt("ref_isext_num", () => String(Reflect.isExtensible(7 as any)));

// null and undefined are refused by both
attempt("obj_keys_null", () => String(Object.keys(null as any).length));
attempt("obj_getproto_null", () => String(Object.getPrototypeOf(null as any)));
attempt("obj_freeze_null", () => String(Object.freeze(null as any)));
attempt("obj_assign_null_src", () => JSON.stringify(Object.assign({ k: 1 }, null as any)));

// the descriptor argument itself is validated the same way by both
attempt("obj_define_baddesc", () => { Object.defineProperty({} as any, "x", 1 as any); return "ok"; });
attempt("ref_define_baddesc", () => String(Reflect.defineProperty({} as any, "x", 1 as any)));
attempt("obj_define_both", () => { Object.defineProperty({} as any, "x", { value: 1, get() { return 2; } } as any); return "ok"; });
attempt("ref_define_both", () => String(Reflect.defineProperty({} as any, "x", { value: 1, get() { return 2; } } as any)));

// Reflect.get/set take a key that is coerced exactly like bracket access
const src: any = { "1": "one", "true": "T", "[object Object]": "OBJ" };
console.log("ref_get_num=" + Reflect.get(src, 1 as any));
console.log("ref_get_bool=" + Reflect.get(src, true as any));
console.log("ref_get_obj=" + Reflect.get(src, {} as any));

// Reflect.ownKeys is the only one that reports strings AND symbols together
const s = Symbol("s");
const both: any = { str: 1, [s]: 2 };
console.log("ref_ownkeys=" + Reflect.ownKeys(both).map(String).join("|"));
console.log("obj_names=" + Object.getOwnPropertyNames(both).join("|"));
console.log("obj_symbols=" + Object.getOwnPropertySymbols(both).map(String).join("|"));

// Reflect has no keys/values/entries/assign/create/freeze twins
console.log("no_twins=" + ["keys", "values", "entries", "assign", "create", "freeze", "seal", "is"]
  .map((n) => n + ":" + ((Reflect as any)[n] === undefined))
  .join("|"));

// every Reflect member is a plain function, and Reflect itself is not callable
console.log("reflect_typeof=" + typeof Reflect);
console.log("reflect_tag=" + Object.prototype.toString.call(Reflect));
console.log("reflect_members=" + Object.getOwnPropertyNames(Reflect).sort().join("|"));
console.log("reflect_ctor=" + ((Reflect as any).prototype === undefined));
