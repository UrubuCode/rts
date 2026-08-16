// ONE thing: the shape of the Number constructor and of Number.prototype.
// The static constants are frozen data properties, the static parsers are the
// SAME function objects as the globals, and Number.prototype is itself a Number
// object whose [[NumberData]] is +0 — so valueOf works on it.

// --- the constructor object ---
console.log("typeof=" + typeof Number);
console.log("name=" + Number.name);
console.log("length=" + Number.length);
console.log("keys=[" + Object.keys(Number).join(",") + "]");
console.log("json=" + JSON.stringify(Number));
console.log("proto_is_Function=" + (Object.getPrototypeOf(Number) === Function.prototype));
console.log("prototype_flags=" + (() => {
  const d = Object.getOwnPropertyDescriptor(Number, "prototype") as any;
  return [d.writable, d.enumerable, d.configurable].join(",");
})());

// --- the constants are frozen data properties ---
const constants: string[] = [
  "MAX_SAFE_INTEGER", "MIN_SAFE_INTEGER", "MAX_VALUE", "MIN_VALUE",
  "EPSILON", "POSITIVE_INFINITY", "NEGATIVE_INFINITY", "NaN",
];
for (const c of constants) {
  const d = Object.getOwnPropertyDescriptor(Number, c) as any;
  console.log(c + "=" + String(d.value) + " flags=" + [d.writable, d.enumerable, d.configurable].join(","));
}
console.log("NaN_is_nan=" + Number.isNaN(Number.NaN));
console.log("infinities=" + (Number.POSITIVE_INFINITY === Infinity) + "," + (Number.NEGATIVE_INFINITY === -Infinity));
console.log("safe_relation=" + (Number.MIN_SAFE_INTEGER === -Number.MAX_SAFE_INTEGER));
console.log("min_value_is_subnormal=" + (Number.MIN_VALUE / 2 === 0));

// --- writing to a frozen constant is silently ignored (Reflect.set reports) ---
console.log("reflect_set_MAX=" + Reflect.set(Number, "MAX_VALUE", 1));
console.log("MAX_unchanged=" + (Number.MAX_VALUE === 1.7976931348623157e308));
console.log("reflect_delete_EPSILON=" + Reflect.deleteProperty(Number, "EPSILON"));
console.log("EPSILON_still_here=" + (typeof Number.EPSILON));

// --- the static parsers ARE the global ones ---
console.log("parseInt_identity=" + (Number.parseInt === parseInt));
console.log("parseFloat_identity=" + (Number.parseFloat === parseFloat));
console.log("isNaN_is_not_global=" + (Number.isNaN === (globalThis as any).isNaN));
console.log("isFinite_is_not_global=" + (Number.isFinite === (globalThis as any).isFinite));

// --- static method presence and arity ---
const statics: string[] = ["isFinite", "isInteger", "isNaN", "isSafeInteger", "parseFloat", "parseInt"];
const staticInfo: string[] = [];
for (const s of statics) {
  const fn = (Number as any)[s];
  staticInfo.push(s + ":" + typeof fn + ":" + String(fn.length) + ":" + String(fn.name));
}
console.log("statics=" + staticInfo.join(" "));

// --- prototype method presence and arity ---
const protos: string[] = ["toExponential", "toFixed", "toPrecision", "toString", "valueOf", "constructor"];
const protoInfo: string[] = [];
for (const p of protos) {
  const fn = (Number.prototype as any)[p];
  protoInfo.push(p + ":" + typeof fn + ":" + String(fn.length));
}
console.log("prototype_methods=" + protoInfo.join(" "));
console.log("constructor_identity=" + (Number.prototype.constructor === Number));
console.log("proto_keys=[" + Object.keys(Number.prototype).join(",") + "]");

// --- Number.prototype is itself a Number object holding +0 ---
console.log("proto_tag=" + Object.prototype.toString.call(Number.prototype));
console.log("proto_valueOf=" + String(Number.prototype.valueOf.call(Number.prototype)));
console.log("proto_toString=" + Number.prototype.toString.call(Number.prototype));
console.log("proto_plus_zero=" + Object.is(Number.prototype.valueOf.call(Number.prototype), 0));
console.log("proto_of_proto=" + (Object.getPrototypeOf(Number.prototype) === Object.prototype));

// --- called versus constructed ---
console.log("call_no_args=" + String(Number()) + " neg0:" + Object.is(Number(), -0));
console.log("call_undefined=" + String(Number(undefined)));
console.log("call_typeof=" + typeof Number(5));
console.log("new_typeof=" + typeof new Number(5));
console.log("new_tag=" + Object.prototype.toString.call(new Number(5)));
console.log("new_equals_primitive=" + (new Number(5) == 5) + "," + ((new Number(5) as any) === 5));
console.log("new_no_args_valueOf=" + String(new Number().valueOf()));
console.log("instanceof=" + (new Number(5) instanceof Number) + "," + ((5 as any) instanceof Number));

// --- a subclass keeps the internal slot ---
class Counted extends Number {
  extra(): string {
    return "x" + this.valueOf();
  }
}
const sub = new Counted(7);
console.log("subclass_valueOf=" + String(sub.valueOf()));
console.log("subclass_extra=" + sub.extra());
console.log("subclass_tag=" + Object.prototype.toString.call(sub));
console.log("subclass_plus=" + String((sub as any) + 1));
console.log("subclass_instanceof=" + (sub instanceof Counted) + "," + (sub instanceof Number));

// --- the prototype methods brand-check their receiver ---
function attempt(label: string, fn: () => any): void {
  try {
    console.log(label + "=" + String(fn()));
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}
attempt("valueOf_on_object", () => Number.prototype.valueOf.call({}));
attempt("toString_on_string", () => Number.prototype.toString.call("5"));
attempt("toFixed_on_null", () => Number.prototype.toFixed.call(null, 2));
attempt("toFixed_on_wrapper", () => Number.prototype.toFixed.call(new Number(1.234), 2));
attempt("toString_on_primitive", () => Number.prototype.toString.call(255, 16));
attempt("valueOf_on_bigint", () => Number.prototype.valueOf.call(1n));
