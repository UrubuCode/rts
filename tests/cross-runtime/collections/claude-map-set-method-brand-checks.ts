// Cross-runtime: every Map/Set method carries a BRAND CHECK — it needs the real
// internal slot, so a plain object, the prototype itself, or the sibling
// collection is refused with a TypeError.

function probe(label: string, fn: () => any): void {
  try {
    const r = fn();
    console.log(label + "=ok:" + String(r));
  } catch (e: any) {
    console.log(label + "=" + e.constructor.name);
  }
}

const m = new Map([["a", 1]]);
const s = new Set([1]);

// --- a plain object has no slot ---
probe("get_plain", () => Map.prototype.get.call({} as any, "a"));
probe("set_plain", () => Map.prototype.set.call({} as any, "a", 1));
probe("has_plain", () => Map.prototype.has.call({} as any, "a"));
probe("delete_plain", () => Map.prototype.delete.call({} as any, "a"));
probe("clear_plain", () => Map.prototype.clear.call({} as any));
probe("forEach_plain", () => Map.prototype.forEach.call({} as any, () => {}));
probe("entries_plain", () => Map.prototype.entries.call({} as any));
probe("setadd_plain", () => Set.prototype.add.call({} as any, 1));
probe("sethas_plain", () => Set.prototype.has.call({} as any, 1));
probe("setvalues_plain", () => Set.prototype.values.call({} as any));

// --- primitives are refused too ---
probe("get_null", () => Map.prototype.get.call(null as any, "a"));
probe("get_number", () => Map.prototype.get.call(7 as any, "a"));
probe("get_string", () => Map.prototype.get.call("xy" as any, "a"));

// --- the sibling collection is not the same brand ---
probe("map_get_on_set", () => Map.prototype.get.call(s as any, 1));
probe("set_has_on_map", () => Set.prototype.has.call(m as any, "a"));
probe("map_foreach_on_set", () => Map.prototype.forEach.call(s as any, () => {}));
probe("weakmap_get_on_map", () => WeakMap.prototype.get.call(m as any, {}));
probe("map_get_on_weakmap", () => Map.prototype.get.call(new WeakMap() as any, {}));

// --- the prototype object is not an instance ---
probe("map_proto_get", () => Map.prototype.get.call(Map.prototype as any, "a"));
probe("set_proto_has", () => Set.prototype.has.call(Set.prototype as any, 1));

// --- but the real instance works through .call ---
probe("get_real", () => Map.prototype.get.call(m, "a"));
probe("has_real", () => Set.prototype.has.call(s, 1));

// --- an object whose PROTOTYPE is a real Map still has no slot of its own ---
const inherited: any = Object.create(m);
probe("inherited_size", () => inherited.size);
probe("inherited_get", () => inherited.get("a"));

// --- the constructors demand `new` ---
probe("map_no_new", () => (Map as any)("x"));
probe("set_no_new", () => (Set as any)([1]));
probe("weakmap_no_new", () => (WeakMap as any)());
probe("weakset_no_new", () => (WeakSet as any)());

// --- the constructor refuses a non-iterable, and accepts null/undefined ---
probe("map_from_number", () => new Map(7 as any));
probe("map_from_object", () => new Map({} as any).size);
probe("map_from_null", () => new Map(null as any).size);
probe("map_from_undefined", () => new Map(undefined as any).size);
probe("set_from_number", () => new Set(7 as any));
probe("set_from_string", () => [...new Set("aab")].join(","));

// --- entries that are not objects are refused by the Map constructor ---
probe("map_bad_entry_number", () => new Map([1, 2] as any).size);
probe("map_bad_entry_string", () => new Map(["ab"] as any).size);
probe("map_bad_entry_null", () => new Map([null] as any).size);
probe("map_short_entry", () => {
  const built = new Map([["k"]] as any);
  return built.size + ":" + String(built.get("k" as any));
});
probe("map_long_entry", () => {
  const built = new Map([["k", 1, 2]] as any);
  return built.size + ":" + String(built.get("k" as any));
});
