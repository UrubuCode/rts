// ONE thing: the shape of the BigInt constructor and the brand checks on
// BigInt.prototype. Unlike Number, BigInt cannot be `new`ed, its prototype is
// an ORDINARY object with no [[BigIntData]] — so valueOf on the prototype
// itself throws — and its methods accept a primitive or a wrapper, nothing else.

function attempt(label: string, fn: () => any): void {
  try {
    console.log(label + "=" + String(fn()));
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}

// --- the constructor object ---
console.log("typeof=" + typeof BigInt);
console.log("name=" + BigInt.name);
console.log("length=" + BigInt.length);
console.log("keys=[" + Object.keys(BigInt).join(",") + "]");
console.log("proto_is_Function=" + (Object.getPrototypeOf(BigInt) === Function.prototype));
const protoDesc = Object.getOwnPropertyDescriptor(BigInt, "prototype") as any;
console.log("prototype_flags=" + [protoDesc.writable, protoDesc.enumerable, protoDesc.configurable].join(","));

// --- it is callable but not constructible ---
console.log("call=" + String(BigInt(5)));
attempt("new_BigInt", () => new (BigInt as any)(5));
attempt("reflect_construct", () => Reflect.construct(BigInt as any, [5]));
attempt("subclass_construct", () => {
  class Big extends (BigInt as any) {}
  return new Big(5);
});

// --- the statics ---
const statics: string[] = ["asIntN", "asUintN"];
for (const s of statics) {
  const fn = (BigInt as any)[s];
  console.log("static_" + s + "=" + typeof fn + ":" + String(fn.length) + ":" + String(fn.name));
}
console.log("asIntN=" + String(BigInt.asIntN(8, 255n)) + "," + String(BigInt.asIntN(64, 2n ** 63n)));
console.log("asUintN=" + String(BigInt.asUintN(8, -1n)) + "," + String(BigInt.asUintN(64, -1n)));

// --- the prototype: ordinary object, no internal slot of its own ---
console.log("proto_typeof=" + typeof BigInt.prototype);
console.log("proto_tag=" + Object.prototype.toString.call(BigInt.prototype));
console.log("proto_toStringTag=" + String((BigInt.prototype as any)[Symbol.toStringTag]));
const tagDesc = Object.getOwnPropertyDescriptor(BigInt.prototype, Symbol.toStringTag) as any;
console.log("toStringTag_flags=" + [tagDesc.writable, tagDesc.enumerable, tagDesc.configurable].join(","));
console.log("proto_of_proto=" + (Object.getPrototypeOf(BigInt.prototype) === Object.prototype));
console.log("constructor_identity=" + (BigInt.prototype.constructor === BigInt));
console.log("proto_keys=[" + Object.keys(BigInt.prototype).join(",") + "]");
attempt("valueOf_on_prototype", () => BigInt.prototype.valueOf.call(BigInt.prototype));
attempt("toString_on_prototype", () => BigInt.prototype.toString.call(BigInt.prototype));

// --- prototype method arity ---
const methods: string[] = ["toString", "valueOf"];
for (const m of methods) {
  const fn = (BigInt.prototype as any)[m];
  console.log("method_" + m + "=" + typeof fn + ":" + String(fn.length) + ":" + String(fn.name));
}

// --- brand checks: a primitive and its wrapper pass, everything else fails ---
attempt("valueOf_primitive", () => BigInt.prototype.valueOf.call(7n));
attempt("valueOf_wrapper", () => BigInt.prototype.valueOf.call(Object(7n)));
attempt("valueOf_number", () => BigInt.prototype.valueOf.call(7));
attempt("valueOf_number_wrapper", () => BigInt.prototype.valueOf.call(new Number(7)));
attempt("valueOf_string", () => BigInt.prototype.valueOf.call("7"));
attempt("valueOf_null", () => BigInt.prototype.valueOf.call(null));
attempt("valueOf_undefined", () => BigInt.prototype.valueOf.call(undefined));
attempt("valueOf_plain_object", () => BigInt.prototype.valueOf.call({}));
attempt("valueOf_fake_slot", () => BigInt.prototype.valueOf.call({ valueOf: () => 7n }));
attempt("toString_primitive_radix", () => BigInt.prototype.toString.call(255n, 16));
attempt("toString_wrapper_radix", () => BigInt.prototype.toString.call(Object(255n), 2));
attempt("toString_number", () => BigInt.prototype.toString.call(255));
attempt("toString_bad_radix", () => BigInt.prototype.toString.call(255n, 1));
attempt("toString_radix_37", () => BigInt.prototype.toString.call(255n, 37));

// --- the wrapper is an object with an internal slot, and it is never equal
//     to another wrapper of the same value ---
const w1: any = Object(7n);
const w2: any = Object(7n);
console.log("wrapper_typeof=" + typeof w1);
console.log("wrapper_tag=" + Object.prototype.toString.call(w1));
console.log("wrapper_strict=" + (w1 === w2) + " loose=" + (w1 == w2) + " self=" + (w1 === w1));
console.log("wrapper_vs_primitive=" + (w1 == 7n) + "," + ((w1 as any) === 7n));
console.log("wrapper_valueOf=" + String(w1.valueOf()) + " typeof=" + typeof w1.valueOf());
console.log("wrapper_proto=" + (Object.getPrototypeOf(w1) === BigInt.prototype));
console.log("wrapper_is_truthy=" + (Object(0n) ? "yes" : "no"));
w1.tagged = "extra";
console.log("wrapper_takes_properties=" + w1.tagged + " keys=" + Object.keys(w1).join(","));

// --- a primitive answers method calls through the prototype without boxing
//     anything observable ---
console.log("primitive_toString=" + (255n).toString(16));
console.log("primitive_valueOf=" + String((255n).valueOf()) + " typeof=" + typeof (255n).valueOf());
console.log("primitive_constructor=" + ((255n).constructor === BigInt));
console.log("primitive_instanceof=" + ((255n as any) instanceof BigInt) + " wrapper=" + (w2 instanceof BigInt));
// (a plain `p.x = 1` on the primitive is NOT asserted here: it is silently
// ignored in sloppy mode and a TypeError in strict mode, and these fixtures run
// as CommonJS under one runtime and as a module under another. Reflect.set
// answers the same question without depending on the mode.)
attempt("reflect_set_on_primitive", () => Reflect.set(255n as any, "x", 1));
attempt("reflect_set_on_wrapper", () => Reflect.set(Object(255n), "x", 1));
console.log("primitive_property_read=" + String((255n as any).x));
