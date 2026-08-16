// Cross-runtime: `description` vs `toString()` vs implicit coercion. A symbol
// converts to a string only when asked EXPLICITLY; every implicit path throws.

const named = Symbol("hello");
const bare = Symbol();
const empty = Symbol("");
const undef = Symbol(undefined);

// --- description ---
console.log("named=" + named.description);
console.log("bare=" + String(bare.description) + ":" + typeof bare.description);
console.log("empty=[" + empty.description + "]:" + empty.description.length);
console.log("undef=" + String(undef.description));
console.log("bare_eq_undef=" + (bare.description === undef.description));

// --- the description argument is coerced with ToString ---
console.log("num_desc=" + Symbol(7 as any).description);
console.log("null_desc=" + Symbol(null as any).description);
console.log("bool_desc=" + Symbol(false as any).description);
console.log("obj_desc=" + Symbol({ toString() { return "od"; } } as any).description);

// --- description is a PROTOTYPE accessor, not an own property ---
console.log("own_desc=" + Object.prototype.hasOwnProperty.call(named, "description"));
const dd: any = Object.getOwnPropertyDescriptor(Symbol.prototype, "description");
console.log("desc_get=" + typeof dd.get + ":set=" + (dd.set === undefined));
console.log("desc_flags=" + dd.enumerable + ":" + dd.configurable);
console.log("desc_getter_name=" + dd.get.name);

// --- toString() ---
console.log("named_tostring=" + named.toString());
console.log("bare_tostring=" + bare.toString());
console.log("empty_tostring=" + empty.toString());
console.log("registered_tostring=" + Symbol.for("claude-d").toString());
console.log("wellknown_tostring=" + Symbol.iterator.toString());
console.log("wellknown_description=" + Symbol.iterator.description);

// --- String() is the one explicit conversion that works ---
console.log("String_named=" + String(named));
console.log("String_bare=" + String(bare));

// --- every implicit conversion throws ---
function bad(label: string, fn: () => any): void {
  try { const v = fn(); console.log(label + "=no_throw:" + String(v)); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
bad("concat", () => (named as any) + "");
bad("concat_left", () => "" + (named as any));
bad("template", () => `${named as any}`);
bad("unary_plus", () => +(named as any));
bad("number_ctor", () => Number(named as any));
bad("minus", () => (named as any) - 1);
bad("multiply", () => (named as any) * 2);
bad("bitnot", () => ~(named as any));
bad("loose_eq_string", () => (named as any) == "Symbol(hello)");
bad("relational", () => (named as any) < "z");
bad("parseInt", () => parseInt(named as any));

// --- but boolean coercion and strict equality are fine ---
console.log("truthy=" + Boolean(named) + ":" + (named ? "y" : "n"));
console.log("not=" + !named);
console.log("strict_eq=" + (named === named) + ":" + ((named as any) === Symbol("hello")));
console.log("loose_eq_self=" + ((named as any) == named));
console.log("loose_eq_null=" + ((named as any) == null));
console.log("typeof=" + typeof named);

// --- toString/valueOf shape on the prototype ---
console.log("valueOf=" + (Symbol.prototype.valueOf.call(named) === named));
console.log("toString_length=" + Symbol.prototype.toString.length);
bad("toString_on_plain", () => Symbol.prototype.toString.call({}));
bad("description_on_plain", () => dd.get.call({}));

// --- the wrapper object answers the same, and its tag ---
const wrapper: any = Object(named);
console.log("wrapper_typeof=" + typeof wrapper);
console.log("wrapper_description=" + wrapper.description);
console.log("wrapper_tostring=" + wrapper.toString());
console.log("wrapper_valueOf_same=" + (wrapper.valueOf() === named));
console.log("wrapper_tag=" + Object.prototype.toString.call(named));
console.log("prototype_tag=" + (Object.getOwnPropertyDescriptor(Symbol.prototype, Symbol.toStringTag) as any).value);
bad("wrapper_concat", () => (wrapper as any) + "");
