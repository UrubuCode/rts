// Cross-runtime: the JSON namespace object itself — what it holds, what shape
// those properties have, and the fact that it is an ordinary object rather than
// a constructor or a callable.

// --- it is a plain object, not a function ---
console.log("typeof=" + typeof JSON);
console.log("tag=" + Object.prototype.toString.call(JSON));
console.log("proto_is_object=" + (Object.getPrototypeOf(JSON) === Object.prototype));
console.log("is_extensible=" + Object.isExtensible(JSON));
console.log("is_frozen=" + Object.isFrozen(JSON));

// --- own keys: only the two methods and the tag are enumerable-free data ---
console.log("own_names=" + Object.getOwnPropertyNames(JSON).sort().join(","));
console.log("own_symbols=" + Object.getOwnPropertySymbols(JSON).map(String).join(","));
console.log("enumerable_keys=" + JSON.stringify(Object.keys(JSON)));
let forIn = "";
for (const k in JSON) forIn += k + ",";
console.log("for_in=" + JSON.stringify(forIn));
console.log("json_of_json=" + JSON.stringify(JSON));

// --- the two methods ---
function fnShape(label: string, f: any): void {
  console.log(label + "=" + typeof f + ":name=" + f.name + ":length=" + f.length +
    ":has_prototype=" + Object.prototype.hasOwnProperty.call(f, "prototype"));
}
fnShape("stringify", JSON.stringify);
fnShape("parse", JSON.parse);

function flags(label: string, o: any, k: any): void {
  const d: any = Object.getOwnPropertyDescriptor(o, k);
  if (d === undefined) { console.log(label + "=absent"); return; }
  console.log(label + "=" + d.writable + ":" + d.enumerable + ":" + d.configurable);
}
flags("stringify_flags", JSON, "stringify");
flags("parse_flags", JSON, "parse");
flags("tag_flags", JSON, Symbol.toStringTag);
console.log("tag_value=" + JSON.stringify((JSON as any)[Symbol.toStringTag]));

// --- neither method is a constructor, and JSON is not callable ---
function probe(label: string, fn: () => any): void {
  try { console.log(label + "=ok:" + String(fn())); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
probe("new_stringify", () => new (JSON.stringify as any)({}));
probe("new_parse", () => new (JSON.parse as any)("1"));
probe("call_json", () => (JSON as any)());
probe("new_json", () => new (JSON as any)());

// --- the methods do not care about their `this` ---
const detachedStringify = JSON.stringify;
const detachedParse = JSON.parse;
console.log("detached_stringify=" + detachedStringify({ a: 1 }));
console.log("detached_parse=" + detachedParse('{"a":1}').a);
console.log("stringify_call_null=" + JSON.stringify.call(null, { a: 1 }));
console.log("parse_call_number=" + JSON.parse.call(42 as any, "[1]").length);
console.log("stringify_apply_other=" + JSON.stringify.apply({ unrelated: true }, [{ b: 2 }] as any));

// --- and their behaviour survives being lifted onto another object ---
const host: any = { stringify: JSON.stringify, parse: JSON.parse };
console.log("hosted=" + host.stringify(host.parse('{"n":[1,2]}')));

// --- argument arity: extra arguments are ignored, missing ones default ---
console.log("stringify_extra_args=" + (JSON.stringify as any)({ a: 1 }, null, 0, "ignored", 99));
console.log("parse_extra_args=" + (JSON.parse as any)("[1]", undefined, "ignored").length);
console.log("stringify_no_args=" + String((JSON.stringify as any)()));
probe("parse_no_args", () => (JSON.parse as any)());

// --- the property is writable and configurable, so a program may wrap it ---
const original = JSON.stringify;
let wraps = 0;
(JSON as any).stringify = function (...args: any[]) { wraps++; return original.apply(null, args as any); };
console.log("wrapped=" + JSON.stringify({ w: 1 }) + ":" + wraps);
(JSON as any).stringify = original;
console.log("restored=" + (JSON.stringify === original));

// --- and the namespace can be extended, then cleaned up again ---
console.log("define_extra=" + Reflect.defineProperty(JSON, "claude2Probe", { value: 1, configurable: true }));
console.log("extra_visible=" + ("claude2Probe" in JSON) + ":enumerable=" + Object.keys(JSON).length);
console.log("delete_extra=" + Reflect.deleteProperty(JSON, "claude2Probe"));
console.log("names_after=" + Object.getOwnPropertyNames(JSON).sort().join(","));

// --- globalThis holds it exactly once, non-enumerably ---
console.log("global_same=" + ((globalThis as any).JSON === JSON));
flags("global_flags", globalThis, "JSON");
console.log("global_typeof=" + typeof (globalThis as any).JSON);
