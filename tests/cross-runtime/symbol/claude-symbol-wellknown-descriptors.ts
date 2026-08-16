// Cross-runtime: the well-known symbols are non-writable, non-enumerable,
// NON-CONFIGURABLE own data properties of the Symbol constructor — the one
// property shape in the language that cannot be redefined at all.

const names = [
  "iterator", "asyncIterator", "hasInstance", "isConcatSpreadable",
  "match", "matchAll", "replace", "search", "species", "split",
  "toPrimitive", "toStringTag", "unscopables",
];

// --- every one of them exists, is a symbol, and is an OWN property ---
let allSymbols = true;
let allOwn = true;
for (const n of names) {
  if (typeof (Symbol as any)[n] !== "symbol") allSymbols = false;
  if (!Object.prototype.hasOwnProperty.call(Symbol, n)) allOwn = false;
}
console.log("count=" + names.length);
console.log("all_symbols=" + allSymbols);
console.log("all_own=" + allOwn);

// --- the descriptor is identical for all of them ---
let flags = "";
for (const n of names) {
  const d: any = Object.getOwnPropertyDescriptor(Symbol, n);
  const f = String(d.writable) + String(d.enumerable) + String(d.configurable);
  if (flags === "") flags = f;
  else if (flags !== f) flags = "MIXED";
}
console.log("uniform_flags=" + flags);
const one: any = Object.getOwnPropertyDescriptor(Symbol, "iterator");
console.log("writable=" + one.writable);
console.log("enumerable=" + one.enumerable);
console.log("configurable=" + one.configurable);
console.log("is_data=" + (one.get === undefined && one.set === undefined));

// --- each description is "Symbol.<name>" ---
let descOk = true;
for (const n of names) {
  if ((Symbol as any)[n].description !== "Symbol." + n) descOk = false;
}
console.log("descriptions_match=" + descOk);
console.log("iterator_description=" + Symbol.iterator.description);
console.log("toPrimitive_tostring=" + Symbol.toPrimitive.toString());

// --- none of them is in the global registry ---
let anyRegistered = false;
for (const n of names) {
  if (Symbol.keyFor((Symbol as any)[n]) !== undefined) anyRegistered = true;
}
console.log("any_registered=" + anyRegistered);

// --- they are not enumerable, so they do not show up as string keys ---
console.log("symbol_own_names_nonempty=" + (Object.getOwnPropertyNames(Symbol).length > 0));
console.log("symbol_keys=" + Object.keys(Symbol).length);
let inNames = true;
for (const n of names) {
  if (Object.getOwnPropertyNames(Symbol).indexOf(n) < 0) inNames = false;
}
console.log("present_in_getOwnPropertyNames=" + inNames);

// --- redefining is refused because they are non-configurable ---
function bad(label: string, fn: () => any): void {
  try { fn(); console.log(label + "=no_throw"); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
bad("redefine_value", () => Object.defineProperty(Symbol, "iterator", { value: 1 }));
bad("redefine_writable", () => Object.defineProperty(Symbol, "iterator", { writable: true }));
bad("redefine_getter", () => Object.defineProperty(Symbol, "iterator", { get() { return 1; } }));
console.log("reflect_delete=" + Reflect.deleteProperty(Symbol, "iterator"));
console.log("reflect_set=" + Reflect.set(Symbol, "iterator", 1));
console.log("still_symbol=" + typeof Symbol.iterator);

// --- redefining with the SAME value is a no-op that succeeds ---
const same = Object.defineProperty(Symbol, "iterator", { value: Symbol.iterator });
console.log("redefine_same_ok=" + (same === Symbol));

// --- the constructor's own metadata ---
console.log("symbol_name=" + Symbol.name + ":length=" + Symbol.length);
console.log("prototype_flags=" + JSON.stringify(Object.getOwnPropertyDescriptor(Symbol, "prototype")));
console.log("proto_ctor=" + (Symbol.prototype.constructor === Symbol));
console.log("proto_of_symbol=" + (Object.getPrototypeOf(Symbol) === Function.prototype));
console.log("proto_parent=" + (Object.getPrototypeOf(Symbol.prototype) === Object.prototype));

// --- Symbol.prototype[Symbol.toPrimitive] exists and is a method ---
const tp: any = Object.getOwnPropertyDescriptor(Symbol.prototype, Symbol.toPrimitive);
console.log("proto_toPrimitive=" + typeof tp.value + ":" + tp.value.name);
console.log("proto_toPrimitive_flags=" + tp.writable + ":" + tp.enumerable + ":" + tp.configurable);
