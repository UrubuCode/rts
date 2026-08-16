// Cross-runtime: the SHAPE of Array.prototype[Symbol.unscopables] — the object
// itself, its prototype, its entries and the flags on the property that holds
// it. (Deliberately no `with` statement anywhere: that is a SyntaxError under a
// module goal, and the object is observable without one.)

const u: any = (Array.prototype as any)[Symbol.unscopables];

// --- the property that holds it ---
const d: any = Object.getOwnPropertyDescriptor(Array.prototype, Symbol.unscopables);
console.log("is_data=" + (d.get === undefined && d.set === undefined));
console.log("flags=" + d.writable + ":" + d.enumerable + ":" + d.configurable);
console.log("typeof=" + typeof u);
console.log("same_object_each_read=" + (u === (Array.prototype as any)[Symbol.unscopables]));

// --- it is a null-prototype object, so nothing is inherited into the answer ---
console.log("proto=" + String(Object.getPrototypeOf(u)));
console.log("has_toString=" + ("toString" in u));
console.log("tag=" + Object.prototype.toString.call(u));

// --- every entry is the boolean true, held as a plain writable data property ---
const keys = Object.keys(u);
console.log("count_positive=" + (keys.length > 0));
console.log("all_own=" + keys.every((k) => Object.prototype.hasOwnProperty.call(u, k)));
console.log("all_true=" + keys.every((k) => u[k] === true));
console.log("all_boolean=" + keys.every((k) => typeof u[k] === "boolean"));
let uniformFlags = "";
for (const k of keys) {
  const kd: any = Object.getOwnPropertyDescriptor(u, k);
  const f = String(kd.writable) + String(kd.enumerable) + String(kd.configurable);
  if (uniformFlags === "") uniformFlags = f;
  else if (uniformFlags !== f) uniformFlags = "MIXED";
}
console.log("entry_flags=" + uniformFlags);
console.log("no_symbol_keys=" + (Object.getOwnPropertySymbols(u).length === 0));

// --- the names on the list are exactly the ones added after ES5, each of which
//     would have shadowed an existing variable name inside a `with` block ---
const expected = [
  "at", "copyWithin", "entries", "fill", "find", "findIndex", "findLast",
  "findLastIndex", "flat", "flatMap", "includes", "keys", "toReversed",
  "toSorted", "toSpliced", "values",
];
let listed = "";
for (const n of expected) listed += n + ":" + (n in u) + " ";
console.log("expected_present=" + listed.trim());
console.log("expected_all_listed=" + expected.every((n) => u[n] === true));

// --- and the ES5-era names are deliberately NOT on it ---
const absent = ["length", "push", "pop", "join", "map", "filter", "forEach", "slice", "splice", "concat", "indexOf", "reverse", "sort", "reduce", "constructor", "with"];
let missing = "";
for (const n of absent) missing += n + ":" + (n in u) + " ";
console.log("absent_names=" + missing.trim());

// --- every listed name is a real member of Array.prototype ---
console.log("all_listed_exist=" + keys.every((k) => k in Array.prototype));
console.log("all_listed_are_methods=" + keys.every((k) => typeof (Array.prototype as any)[k] === "function"));

// --- which other built-ins carry one at all ---
function carries(label: string, o: any): void {
  console.log(label + "=" + Object.prototype.hasOwnProperty.call(o, Symbol.unscopables) + ":" + ((o as any)[Symbol.unscopables] === undefined ? "undefined" : typeof (o as any)[Symbol.unscopables]));
}
carries("array_prototype", Array.prototype);
carries("object_prototype", Object.prototype);
carries("string_prototype", String.prototype);
carries("map_prototype", Map.prototype);
carries("set_prototype", Set.prototype);
carries("function_prototype", Function.prototype);
carries("number_prototype", Number.prototype);
carries("math", Math);
carries("json", JSON);
carries("array_instance", [1, 2]);
console.log("instance_inherits=" + (([] as any)[Symbol.unscopables] === u));

// --- an array subclass inherits the same object ---
class Sub extends Array<number> {}
console.log("subclass_inherits=" + ((Sub.prototype as any)[Symbol.unscopables] === u));
console.log("subclass_own=" + Object.prototype.hasOwnProperty.call(Sub.prototype, Symbol.unscopables));

// --- and the object is writable, so a program may extend it ---
u.claude2Probe = true;
console.log("added=" + (u.claude2Probe === true) + ":still_null_proto=" + (Object.getPrototypeOf(u) === null));
console.log("delete_added=" + Reflect.deleteProperty(u, "claude2Probe") + ":" + ("claude2Probe" in u));
console.log("count_restored=" + (Object.keys(u).length === keys.length));

// --- the property itself is configurable, so it can be replaced and put back ---
const replaced = { fill: false };
console.log("reflect_set=" + Reflect.set(Array.prototype, Symbol.unscopables, replaced));
console.log("after_set=" + ((Array.prototype as any)[Symbol.unscopables] === replaced));
Object.defineProperty(Array.prototype, Symbol.unscopables, { value: u, writable: false, enumerable: false, configurable: true });
console.log("restored=" + ((Array.prototype as any)[Symbol.unscopables] === u));

// --- the symbol's own identity ---
console.log("symbol_description=" + Symbol.unscopables.description);
console.log("symbol_typeof=" + typeof Symbol.unscopables);
console.log("symbol_to_string=" + String(Symbol.unscopables));
console.log("symbol_not_registered=" + (Symbol.keyFor(Symbol.unscopables) === undefined));
