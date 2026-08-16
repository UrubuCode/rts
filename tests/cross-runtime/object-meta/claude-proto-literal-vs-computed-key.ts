// Pins the FOUR spellings of __proto__ in an object literal: `__proto__: v` and
// `"__proto__": v` mutate the prototype, while `["__proto__"]: v`, the shorthand
// and the method form all create an ordinary own property. 389_proto_accessor
// covers only the plain identifier form.

const p: any = { kind: "PROTO" };

const litIdent: any = { __proto__: p, own: 1 };
console.log("ident_proto=" + (Object.getPrototypeOf(litIdent) === p));
console.log("ident_own=" + Object.getOwnPropertyNames(litIdent).join("|"));
console.log("ident_kind=" + litIdent.kind);

const litString: any = { "__proto__": p, own: 1 };
console.log("string_proto=" + (Object.getPrototypeOf(litString) === p));
console.log("string_own=" + Object.getOwnPropertyNames(litString).join("|"));

const key = "__proto__";
const litComputed: any = { [key]: p, own: 1 };
console.log("computed_proto=" + (Object.getPrototypeOf(litComputed) === p));
console.log("computed_own=" + Object.getOwnPropertyNames(litComputed).join("|"));
console.log("computed_read=" + (litComputed.__proto__ === p));

const __proto__ = p;
const litShorthand: any = { __proto__, own: 1 };
console.log("shorthand_proto=" + (Object.getPrototypeOf(litShorthand) === p));
console.log("shorthand_own=" + Object.getOwnPropertyNames(litShorthand).join("|"));

const litMethod: any = { __proto__() { return "m"; }, own: 1 };
console.log("method_proto=" + (Object.getPrototypeOf(litMethod) === Object.prototype));
console.log("method_own=" + Object.getOwnPropertyNames(litMethod).join("|"));
console.log("method_call=" + litMethod.__proto__());

// JSON.parse always creates an own property, never a prototype
const parsed: any = JSON.parse('{"__proto__":{"polluted":true},"safe":1}');
console.log("json_proto_is_object_proto=" + (Object.getPrototypeOf(parsed) === Object.prototype));
console.log("json_own=" + Object.getOwnPropertyNames(parsed).join("|"));
console.log("json_polluted=" + parsed.__proto__.polluted);
console.log("json_global_clean=" + (({} as any).polluted === undefined));

// the accessor itself lives on Object.prototype and is not enumerable
const d = Object.getOwnPropertyDescriptor(Object.prototype, "__proto__") as any;
console.log("accessor_kind=" + (d && "get" in d ? "accessor" : "data"));
console.log("accessor_flags=e=" + d.enumerable + ",c=" + d.configurable);
console.log("accessor_get_type=" + typeof d.get + ",set=" + typeof d.set);
console.log("getter_matches=" + (d.get.call(litIdent) === p));

// a null-prototype object has no such accessor, so the name is inert
const bare: any = Object.create(null);
bare.__proto__ = { injected: true };
console.log("bare_proto=" + Object.getPrototypeOf(bare));
console.log("bare_own=" + Object.getOwnPropertyNames(bare).join("|"));
console.log("bare_injected=" + bare.__proto__.injected);

// assigning a PRIMITIVE through the setter is silently ignored
const q: any = { a: 1 };
q.__proto__ = 5;
console.log("prim_ignored=" + (Object.getPrototypeOf(q) === Object.prototype));
q.__proto__ = "str";
console.log("prim_ignored2=" + (Object.getPrototypeOf(q) === Object.prototype));
console.log("prim_own=" + Object.getOwnPropertyNames(q).join("|"));

// but null IS accepted
q.__proto__ = null;
console.log("null_accepted=" + Object.getPrototypeOf(q));
console.log("after_null_read=" + q.__proto__);

// defineProperty with the name creates an own data property, shadowing the accessor
const shadow: any = {};
Object.defineProperty(shadow, "__proto__", { value: "own-value", enumerable: true, writable: true, configurable: true });
console.log("shadow_read=" + shadow.__proto__);
console.log("shadow_proto=" + (Object.getPrototypeOf(shadow) === Object.prototype));
console.log("shadow_keys=" + Object.keys(shadow).join("|"));
console.log("shadow_json=" + JSON.stringify(shadow));

// a literal may not mutate the prototype twice
console.log("dup_ok=" + (Object.getPrototypeOf({ __proto__: p, ["__proto__"]: 1 } as any) === p));
