// Cross-runtime: the declared shape of the collection prototypes — each
// method's `length` and `name`, the flags on the property that holds it, and
// which names live on which prototype. Nothing here calls a collection method
// for its effect; the subject is the API surface itself.

function arity(o: any, names: string[]): string {
  let out = "";
  for (const n of names) {
    const f = o[n];
    out += n + ":" + (typeof f === "function" ? f.length + "/" + f.name : typeof f) + " ";
  }
  return out.trim();
}

function flags(o: any, key: any): string {
  const d: any = Object.getOwnPropertyDescriptor(o, key);
  if (d === undefined) return "absent";
  if (d.get !== undefined || d.set !== undefined) {
    return "accessor:get=" + typeof d.get + ",set=" + typeof d.set + "," + d.enumerable + "," + d.configurable;
  }
  return "data:" + typeof d.value + "," + d.writable + "," + d.enumerable + "," + d.configurable;
}

// --- Map ---
console.log("map_ctor=" + Map.length + "/" + Map.name);
console.log("map_methods=" + arity(Map.prototype, ["get", "set", "has", "delete", "clear", "forEach", "keys", "values", "entries"]));
console.log("map_size=" + flags(Map.prototype, "size"));
console.log("map_size_getter_name=" + (Object.getOwnPropertyDescriptor(Map.prototype, "size") as any).get.name);
console.log("map_size_getter_length=" + (Object.getOwnPropertyDescriptor(Map.prototype, "size") as any).get.length);
console.log("map_get_flags=" + flags(Map.prototype, "get"));
console.log("map_ctor_prop=" + flags(Map.prototype, "constructor"));
console.log("map_tag=" + flags(Map.prototype, Symbol.toStringTag));
console.log("map_iterator_is_entries=" + ((Map.prototype as any)[Symbol.iterator] === Map.prototype.entries));
console.log("map_prototype_flags=" + flags(Map, "prototype"));
console.log("map_species=" + flags(Map, Symbol.species));
console.log("map_groupBy=" + arity(Map, ["groupBy"]));

// --- Set, including the seven ES2025 operations ---
console.log("set_ctor=" + Set.length + "/" + Set.name);
console.log("set_methods=" + arity(Set.prototype, ["add", "has", "delete", "clear", "forEach", "keys", "values", "entries"]));
console.log("set_ops=" + arity(Set.prototype, ["union", "intersection", "difference", "symmetricDifference", "isSubsetOf", "isSupersetOf", "isDisjointFrom"]));
console.log("set_size=" + flags(Set.prototype, "size"));
console.log("set_tag=" + flags(Set.prototype, Symbol.toStringTag));
console.log("set_union_flags=" + flags(Set.prototype, "union"));
console.log("set_keys_is_values=" + (Set.prototype.keys === Set.prototype.values));
console.log("set_iterator_is_values=" + ((Set.prototype as any)[Symbol.iterator] === Set.prototype.values));
console.log("set_species=" + flags(Set, Symbol.species));

// --- WeakMap and WeakSet are deliberately smaller ---
console.log("weakmap_ctor=" + WeakMap.length + "/" + WeakMap.name);
console.log("weakmap_methods=" + arity(WeakMap.prototype, ["get", "set", "has", "delete"]));
console.log("weakmap_absent=" + arity(WeakMap.prototype, ["clear", "forEach", "keys", "values", "entries"]));
console.log("weakmap_size=" + flags(WeakMap.prototype, "size"));
console.log("weakmap_iterator=" + typeof (WeakMap.prototype as any)[Symbol.iterator]);
console.log("weakmap_tag=" + flags(WeakMap.prototype, Symbol.toStringTag));
console.log("weakset_ctor=" + WeakSet.length + "/" + WeakSet.name);
console.log("weakset_methods=" + arity(WeakSet.prototype, ["add", "has", "delete"]));
console.log("weakset_tag=" + flags(WeakSet.prototype, Symbol.toStringTag));

// --- the four constructors sit directly on Function.prototype ---
console.log("ctor_protos=" +
  (Object.getPrototypeOf(Map) === Function.prototype) + ":" +
  (Object.getPrototypeOf(Set) === Function.prototype) + ":" +
  (Object.getPrototypeOf(WeakMap) === Function.prototype) + ":" +
  (Object.getPrototypeOf(WeakSet) === Function.prototype));
console.log("proto_parents=" +
  (Object.getPrototypeOf(Map.prototype) === Object.prototype) + ":" +
  (Object.getPrototypeOf(Set.prototype) === Object.prototype));
// the prototype is NOT itself a Map — reading `size` off it fails the brand check
try { void (Map.prototype as any).size; console.log("map_proto_is_a_map=yes"); }
catch (e: any) { console.log("map_proto_is_not_a_map=" + e.constructor.name); }

// --- every one of them refuses a plain call ---
function callNoNew(label: string, C: any): void {
  try { C(); console.log(label + "=no_throw"); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
callNoNew("map_no_new", Map);
callNoNew("set_no_new", Set);
callNoNew("weakmap_no_new", WeakMap);
callNoNew("weakset_no_new", WeakSet);

// --- the methods are writable and configurable, so a patch is possible ---
const originalHas = Set.prototype.has;
let patched = 0;
(Set.prototype as any).has = function (this: any, v: any) { patched++; return originalHas.call(this, v); };
const s = new Set([1]);
console.log("patched_has=" + s.has(1) + ":" + patched);
(Set.prototype as any).has = originalHas;
console.log("restored_has=" + (Set.prototype.has === originalHas));

// --- and non-enumerable, so nothing shows up in for-in over an instance ---
let seen = "";
for (const k in new Map([["a", 1]])) seen += k + ",";
console.log("for_in_instance=" + JSON.stringify(seen));
console.log("instance_own_keys=" + Reflect.ownKeys(new Map([["a", 1]])).length);
console.log("proto_enumerable_keys=" + Object.keys(Map.prototype).length + ":" + Object.keys(Set.prototype).length);

// --- the well-known names are all present on the prototype ---
const mapNames = ["get", "set", "has", "delete", "clear", "forEach", "keys", "values", "entries", "size", "constructor"];
let allPresent = true;
for (const n of mapNames) if (!Object.prototype.hasOwnProperty.call(Map.prototype, n)) allPresent = false;
console.log("map_all_present=" + allPresent);
console.log("map_symbol_keys=" + Object.getOwnPropertySymbols(Map.prototype).map(String).sort().join(","));
console.log("set_symbol_keys=" + Object.getOwnPropertySymbols(Set.prototype).map(String).sort().join(","));
console.log("weakmap_symbol_keys=" + Object.getOwnPropertySymbols(WeakMap.prototype).map(String).sort().join(","));
