// Cross-runtime: what happens when Symbol.toPrimitive is NOT a well-behaved
// method — null/undefined fall back to valueOf/toString, an object return is a
// TypeError, and a non-callable non-nullish value is refused outright.

// --- an explicit null/undefined hook falls back to OrdinaryToPrimitive ---
const nullHook: any = {
  [Symbol.toPrimitive]: null,
  valueOf() { return 5; },
  toString() { return "five"; },
};
console.log("null_hook_number=" + (nullHook * 2));
console.log("null_hook_string=" + String(nullHook));
console.log("null_hook_default=" + (nullHook + ""));

const undefHook: any = {
  [Symbol.toPrimitive]: undefined,
  valueOf() { return 7; },
  toString() { return "seven"; },
};
console.log("undef_hook_number=" + (undefHook - 1));
console.log("undef_hook_string=" + `${undefHook}`);

// --- with no hook at all, the order is valueOf-then-toString for
//     number/default, toString-then-valueOf for string ---
const order: string[] = [];
const spy: any = {
  valueOf() { order.push("valueOf"); return 3; },
  toString() { order.push("toString"); return "three"; },
};
order.length = 0; const n = spy * 1;
console.log("number_hint=" + order.join(",") + "=>" + n);
order.length = 0; const s = String(spy);
console.log("string_hint=" + order.join(",") + "=>" + s);
order.length = 0; const d = spy + "";
console.log("default_hint=" + order.join(",") + "=>" + d);

// --- if valueOf returns an object, toString is tried next ---
const objValueOf: any = {
  valueOf() { return {}; },
  toString() { return "fromToString"; },
};
console.log("valueOf_object=" + (objValueOf + ""));

// --- if BOTH return objects, it is a TypeError ---
const bothObjects: any = { valueOf() { return {}; }, toString() { return {}; } };
function bad(label: string, fn: () => any): void {
  try { const v = fn(); console.log(label + "=no_throw:" + String(v)); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
bad("both_objects", () => bothObjects + "");
bad("both_objects_number", () => bothObjects * 1);

// --- a hook that returns an object is a TypeError, whatever the hint ---
const objHook: any = { [Symbol.toPrimitive]() { return { boxed: true }; } };
bad("hook_object_default", () => objHook + "");
bad("hook_object_string", () => String(objHook));
bad("hook_object_number", () => +objHook);
bad("hook_array_return", () => ({ [Symbol.toPrimitive]() { return []; } } as any) + "");

// --- a hook returning a SYMBOL is allowed (a symbol is a primitive) ---
const symHook: any = { [Symbol.toPrimitive]() { return Symbol("prim"); } };
console.log("hook_symbol_typeof=" + typeof (symHook as any)[Symbol.toPrimitive]("default"));
bad("hook_symbol_concat", () => symHook + "");

// --- a non-callable, non-nullish hook is refused before any fallback ---
const numHook: any = { [Symbol.toPrimitive]: 42, valueOf() { return 1; } };
bad("hook_number", () => numHook + "");
const strHook: any = { [Symbol.toPrimitive]: "nope", toString() { return "t"; } };
bad("hook_string", () => String(strHook));

// --- the hook is called with exactly one argument: the hint string ---
const hints: string[] = [];
const hintSpy: any = {
  [Symbol.toPrimitive](h: any) { hints.push(String(h) + "/" + typeof h + "/" + arguments.length); return 1; },
};
const _a = hintSpy + 1;
const _b = `${hintSpy}`;
const _c = +hintSpy;
const _d = hintSpy == 1;
const _e = hintSpy < 2;
console.log("hints=" + hints.join(","));

// --- Date is the one built-in whose DEFAULT hint behaves like "string" ---
const dt = new Date(Date.UTC(2020, 0, 2, 3, 4, 5));
console.log("date_default_is_string=" + ((dt as any) + "" === dt.toString()));
console.log("date_number_is_time=" + (+dt === dt.getTime()));
console.log("date_template_is_string=" + (`${dt}` === dt.toString()));
console.log("date_hook_type=" + typeof (Date.prototype as any)[Symbol.toPrimitive]);
console.log("date_hook_name=" + (Date.prototype as any)[Symbol.toPrimitive].name);
console.log("date_hook_length=" + (Date.prototype as any)[Symbol.toPrimitive].length);
bad("date_hook_bad_hint", () => (Date.prototype as any)[Symbol.toPrimitive].call(dt, "weird"));
bad("date_hook_no_hint", () => (Date.prototype as any)[Symbol.toPrimitive].call(dt));
bad("date_hook_plain_receiver", () => (Date.prototype as any)[Symbol.toPrimitive].call({}, "string"));

// --- an ordinary object's default hint behaves like "number" ---
const plainSpy: any = { valueOf() { return 9; }, toString() { return "nine"; } };
console.log("plain_default=" + (plainSpy + ""));
console.log("plain_relational=" + (plainSpy > 8));
console.log("plain_loose_eq=" + (plainSpy == 9));
