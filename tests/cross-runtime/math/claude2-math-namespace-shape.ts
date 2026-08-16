// ONE thing: Math is an ORDINARY object used to hold names — not a function,
// not a constructor, and with nothing enumerable on it. Its methods are plain
// functions without a prototype property, so none of them can be `new`ed.

// --- what Math itself is ---
console.log("typeof=" + typeof Math);
console.log("tag=" + Object.prototype.toString.call(Math));
console.log("toStringTag=" + String((Math as any)[Symbol.toStringTag]));
console.log("proto_is_Object=" + (Object.getPrototypeOf(Math) === Object.prototype));
console.log("instanceof_Object=" + (Math instanceof Object));
console.log("is_extensible=" + Object.isExtensible(Math));
console.log("is_frozen=" + Object.isFrozen(Math));

// --- nothing on Math is enumerable ---
console.log("keys=[" + Object.keys(Math).join(",") + "]");
console.log("json=" + JSON.stringify(Math));
const seen: string[] = [];
for (const k in Math) {
  seen.push(k);
}
console.log("for_in=[" + seen.join(",") + "]");
console.log("entries=[" + Object.entries(Math).join(",") + "]");
console.log("spread_keys=[" + Object.keys({ ...Math }).join(",") + "]");

// --- the toStringTag descriptor: read-only but configurable ---
const tagDesc = Object.getOwnPropertyDescriptor(Math, Symbol.toStringTag) as any;
console.log("tag_flags=" + [tagDesc.writable, tagDesc.enumerable, tagDesc.configurable].join(","));
console.log("tag_value=" + String(tagDesc.value));
// (the INDEX of the symbol key is not comparable: engines carry a different
// number of extra own properties on Math, so only its presence is pinned)
console.log("ownKeys_has_symbol=" + (Reflect.ownKeys(Math).indexOf(Symbol.toStringTag as any) >= 0));

// --- a method descriptor: writable and configurable, never enumerable ---
const maxDesc = Object.getOwnPropertyDescriptor(Math, "max") as any;
console.log("max_flags=" + [maxDesc.writable, maxDesc.enumerable, maxDesc.configurable].join(","));
console.log("max_is_value_prop=" + (typeof maxDesc.get === "undefined" && typeof maxDesc.value === "function"));

// --- every named method is present and own ---
const names: string[] = [
  "abs", "acos", "acosh", "asin", "asinh", "atan", "atan2", "atanh", "cbrt",
  "ceil", "clz32", "cos", "cosh", "exp", "expm1", "floor", "fround", "hypot",
  "imul", "log", "log10", "log1p", "log2", "max", "min", "pow", "random",
  "round", "sign", "sin", "sinh", "sqrt", "tan", "tanh", "trunc",
];
const missing: string[] = [];
const notOwn: string[] = [];
for (const n of names) {
  if (typeof (Math as any)[n] !== "function") missing.push(n);
  if (!Object.prototype.hasOwnProperty.call(Math, n)) notOwn.push(n);
}
console.log("missing=[" + missing.join(",") + "]");
console.log("not_own=[" + notOwn.join(",") + "]");

// --- the arity of every one of them, which the spec fixes ---
const arities: string[] = [];
for (const n of names) {
  arities.push(n + ":" + String((Math as any)[n].length));
}
console.log("arity=" + arities.join(" "));

// --- and the name of every one of them ---
const badNames: string[] = [];
for (const n of names) {
  if ((Math as any)[n].name !== n) badNames.push(n + "->" + (Math as any)[n].name);
}
console.log("name_mismatch=[" + badNames.join(",") + "]");

// --- none of the methods is a constructor, so none has a prototype ---
const withProto: string[] = [];
for (const n of names) {
  if (Object.prototype.hasOwnProperty.call((Math as any)[n], "prototype")) withProto.push(n);
}
console.log("methods_with_prototype=[" + withProto.join(",") + "]");

function attempt(label: string, fn: () => any): void {
  try {
    console.log(label + "=" + String(fn()));
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}
attempt("new_Math_abs", () => new (Math.abs as any)(1));
attempt("new_Math_max", () => new (Math.max as any)(1, 2));
attempt("call_Math", () => (Math as any)());
attempt("new_Math", () => new (Math as any)());
attempt("reflect_construct_floor", () => Reflect.construct(Math.floor as any, [1.5]));

// --- the constants are not functions and there are exactly eight of them ---
const constants: string[] = ["E", "LN10", "LN2", "LOG10E", "LOG2E", "PI", "SQRT1_2", "SQRT2"];
const constFlags: string[] = [];
for (const c of constants) {
  const d = Object.getOwnPropertyDescriptor(Math, c) as any;
  constFlags.push(c + ":" + [d.writable, d.enumerable, d.configurable].join(""));
}
console.log("const_flags=" + constFlags.join(" "));

// --- a method is not bound to Math: it has no receiver at all ---
const detached = Math.abs;
console.log("detached_abs=" + String(detached(-5)));
console.log("call_with_null=" + String(Math.abs.call(null, -6)));
console.log("apply_with_string=" + String(Math.max.apply("x" as any, [1, 7, 3])));
console.log("reflect_apply=" + String(Reflect.apply(Math.min, undefined, [4, 2, 8])));
