// Cross-runtime: a symbol-keyed property is INVISIBLE to the string-key
// reflection surface (keys/entries/for-in/JSON) but is carried by the
// copying operations (assign, spread, ownKeys, defineProperties).

const s1 = Symbol("one");
const s2 = Symbol.for("claude-vis-two");
const src: any = { plain: 1, other: 2 };
src[s1] = "sym1";
src[s2] = "sym2";

// --- hidden from the string-key surface ---
console.log("keys=" + Object.keys(src).join(","));
console.log("values=" + Object.values(src).join(","));
console.log("entries=" + Object.entries(src).map((e: any) => e[0]).join(","));
console.log("names=" + Object.getOwnPropertyNames(src).join(","));
console.log("json=" + JSON.stringify(src));
let forIn = "";
for (const k in src) forIn += k + ";";
console.log("for_in=" + forIn);
console.log("stringify_symbol_value=" + JSON.stringify({ a: s1 }));
console.log("stringify_symbol_in_array=" + JSON.stringify([s1]));

// --- visible to the symbol surface ---
const syms = Object.getOwnPropertySymbols(src);
console.log("symbols_count=" + syms.length);
console.log("symbols_descriptions=" + syms.map((s: any) => String(s.description)).join(","));
console.log("symbol_read=" + src[s1] + ":" + src[s2]);
console.log("in_operator=" + (s1 in src));
console.log("hasOwn=" + Object.prototype.hasOwnProperty.call(src, s1));

// --- Reflect.ownKeys: integer indices, then strings in insertion order,
//     then symbols in insertion order ---
const ordered: any = { b: 1 };
ordered[s1] = 1;
ordered.a = 1;
ordered[2] = 1;
ordered[s2] = 1;
ordered[0] = 1;
const ok = Reflect.ownKeys(ordered);
console.log("ownkeys_kinds=" + ok.map((k: any) => typeof k).join(","));
console.log("ownkeys_strings=" + ok.filter((k: any) => typeof k === "string").join(","));
console.log("ownkeys_symbols=" + ok.filter((k: any) => typeof k === "symbol").map((k: any) => String(k.description)).join(","));

// --- Object.assign copies enumerable symbol keys ---
const assigned: any = Object.assign({}, src);
console.log("assign_string_keys=" + Object.keys(assigned).join(","));
console.log("assign_symbols=" + Object.getOwnPropertySymbols(assigned).length);
console.log("assign_values=" + assigned[s1] + ":" + assigned[s2]);

// --- spread copies them too ---
const spread: any = { ...src };
console.log("spread_symbols=" + Object.getOwnPropertySymbols(spread).length);
console.log("spread_values=" + spread[s1] + ":" + spread[s2]);

// --- a NON-enumerable symbol key is skipped by both ---
const hidden: any = {};
Object.defineProperty(hidden, s1, { value: "nope", enumerable: false });
console.log("hidden_read=" + hidden[s1]);
console.log("hidden_own_symbols=" + Object.getOwnPropertySymbols(hidden).length);
console.log("hidden_assign=" + Object.getOwnPropertySymbols(Object.assign({}, hidden)).length);
console.log("hidden_spread=" + Object.getOwnPropertySymbols({ ...hidden }).length);

// --- getOwnPropertyDescriptors keeps it, and defineProperties restores it ---
const descs: any = Object.getOwnPropertyDescriptors(hidden);
console.log("descs_symbols=" + Object.getOwnPropertySymbols(descs).length);
console.log("descs_enumerable=" + descs[s1].enumerable + ":" + descs[s1].value);
const restored: any = Object.defineProperties({}, descs);
console.log("restored=" + restored[s1] + ":ownsyms=" + Object.getOwnPropertySymbols(restored).length);
console.log("restored_enumerable=" + (Object.getOwnPropertyDescriptor(restored, s1) as any).enumerable);

// --- Object.create with a symbol-keyed descriptor map ---
const created: any = Object.create(null, { [s1]: { value: "created", enumerable: true } } as any);
console.log("created=" + created[s1]);

// --- deleting a symbol key ---
const del: any = { [s1]: 1 };
console.log("delete=" + delete del[s1] + ":left=" + Object.getOwnPropertySymbols(del).length);

// --- symbol keys in a class body and in a computed method ---
class WithSym {
  [s1]() { return "method"; }
  static [s2] = "static";
}
console.log("class_method=" + new WithSym()[s1]());
console.log("class_proto_symbols=" + Object.getOwnPropertySymbols(WithSym.prototype).length);
console.log("class_static=" + (WithSym as any)[s2]);
console.log("method_enumerable=" + (Object.getOwnPropertyDescriptor(WithSym.prototype, s1) as any).enumerable);
