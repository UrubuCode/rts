// Cross-runtime: Symbol.hasInstance makes `instanceof` work on a right-hand
// side that is NOT a function at all, and its result is coerced to a boolean —
// while a plain non-callable right-hand side is a TypeError.

// --- a plain object with @@hasInstance answers instanceof ---
const evens: any = {
  [Symbol.hasInstance](v: any) { return typeof v === "number" && v % 2 === 0; },
};
console.log("plain_even=" + (4 instanceof evens));
console.log("plain_odd=" + (5 instanceof evens));
console.log("plain_string=" + ("4" instanceof evens));
console.log("plain_null=" + (null instanceof evens));
console.log("rhs_typeof=" + typeof evens);

// --- the return value is coerced with ToBoolean ---
const truthy: any = { [Symbol.hasInstance]() { return "yes"; } };
const falsy: any = { [Symbol.hasInstance]() { return 0; } };
const undef: any = { [Symbol.hasInstance]() { return undefined; } };
const nanRet: any = { [Symbol.hasInstance]() { return NaN; } };
const objRet: any = { [Symbol.hasInstance]() { return {}; } };
console.log("coerce_string=" + (1 instanceof truthy));
console.log("coerce_zero=" + (1 instanceof falsy));
console.log("coerce_undefined=" + (1 instanceof undef));
console.log("coerce_nan=" + (1 instanceof nanRet));
console.log("coerce_object=" + (1 instanceof objRet));
console.log("coerce_typeof=" + typeof (1 instanceof truthy));

// --- the hook receives the LEFT operand as its argument, `this` as the RHS ---
const spy: any = {
  mark: "rhs",
  [Symbol.hasInstance](v: any) {
    seenArg = String(v);
    seenThis = this.mark;
    return true;
  },
};
let seenArg = "";
let seenThis = "";
const dummy = { k: 1 };
const _ = dummy instanceof spy;
console.log("hook_arg_type=" + seenArg);
console.log("hook_this=" + seenThis);
console.log("hook_primitive_arg=" + ((7 as any) instanceof spy) + ":" + seenArg);

// --- inherited from a prototype works as well ---
const base: any = { [Symbol.hasInstance]() { return true; } };
const derived: any = Object.create(base);
console.log("inherited=" + (1 instanceof derived));

// --- a FUNCTION with @@hasInstance overrides the ordinary prototype walk ---
function Ordinary() { /* plain constructor */ }
const inst: any = new (Ordinary as any)();
console.log("default_walk=" + (inst instanceof Ordinary));
Object.defineProperty(Ordinary, Symbol.hasInstance, { value: () => false, configurable: true });
console.log("overridden_false=" + (inst instanceof Ordinary));
delete (Ordinary as any)[Symbol.hasInstance];
console.log("restored_walk=" + (inst instanceof Ordinary));

// --- Function.prototype[Symbol.hasInstance] is the default, and is
//     non-writable / non-configurable ---
const fd: any = Object.getOwnPropertyDescriptor(Function.prototype, Symbol.hasInstance);
console.log("default_hook_type=" + typeof fd.value);
console.log("default_hook_name=" + fd.value.name);
console.log("default_hook_length=" + fd.value.length);
console.log("default_hook_flags=" + fd.writable + ":" + fd.enumerable + ":" + fd.configurable);
console.log("default_hook_direct=" + fd.value.call(Ordinary, inst));
console.log("default_hook_miss=" + fd.value.call(Ordinary, {}));

// --- refusals ---
function bad(label: string, fn: () => any): void {
  try { const v = fn(); console.log(label + "=no_throw:" + String(v)); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
bad("rhs_plain_object", () => (1 as any) instanceof ({} as any));
bad("rhs_number", () => (1 as any) instanceof (5 as any));
bad("rhs_string", () => (1 as any) instanceof ("s" as any));
bad("rhs_null", () => (1 as any) instanceof (null as any));
bad("rhs_undefined", () => (1 as any) instanceof (undefined as any));
bad("hook_not_callable", () => (1 as any) instanceof ({ [Symbol.hasInstance]: 42 } as any));
bad("hook_throws", () => (1 as any) instanceof ({ [Symbol.hasInstance]() { throw new RangeError("no"); } } as any));

// --- a hook explicitly set to null/undefined falls back to the default ---
const nulled: any = function NulledCtor() { /* ctor */ };
Object.defineProperty(nulled, Symbol.hasInstance, { value: undefined, configurable: true });
const ni: any = new nulled();
console.log("hook_undefined_falls_back=" + (ni instanceof nulled));
Object.defineProperty(nulled, Symbol.hasInstance, { value: null, configurable: true });
console.log("hook_null_falls_back=" + (ni instanceof nulled));

// --- an arrow function has no .prototype, so the default hook throws ---
const arrow: any = () => 1;
bad("arrow_rhs", () => ({} as any) instanceof arrow);
