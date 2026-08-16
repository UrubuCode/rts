// Cross-runtime: the metadata properties of the four collection prototypes —
// Symbol.toStringTag, Symbol.species, and which named method Symbol.iterator
// is an ALIAS of.

// --- Object.prototype.toString reads the tag ---
console.log("map_tag=" + Object.prototype.toString.call(new Map()));
console.log("set_tag=" + Object.prototype.toString.call(new Set()));
console.log("weakmap_tag=" + Object.prototype.toString.call(new WeakMap()));
console.log("weakset_tag=" + Object.prototype.toString.call(new WeakSet()));
console.log("proto_tag=" + Object.prototype.toString.call(Map.prototype));

// --- the tag is a non-writable, non-enumerable, configurable own string ---
const td: any = Object.getOwnPropertyDescriptor(Map.prototype, Symbol.toStringTag);
console.log("tag_value=" + td.value);
console.log("tag_writable=" + td.writable);
console.log("tag_enumerable=" + td.enumerable);
console.log("tag_configurable=" + td.configurable);
console.log("set_tag_value=" + (Object.getOwnPropertyDescriptor(Set.prototype, Symbol.toStringTag) as any).value);
console.log("weakset_tag_value=" + (Object.getOwnPropertyDescriptor(WeakSet.prototype, Symbol.toStringTag) as any).value);

// --- an own tag defined on the instance shadows the prototype's ---
const tagged: any = new Map([["k", 1]]);
Object.defineProperty(tagged, Symbol.toStringTag, { value: "Custom", configurable: true });
console.log("overridden_tag=" + Object.prototype.toString.call(tagged));
console.log("overridden_still_map=" + (tagged instanceof Map) + ":" + tagged.size);
console.log("own_symbols_after=" + Object.getOwnPropertySymbols(tagged).length);

// --- Symbol.species is a getter returning the constructor itself ---
console.log("map_species=" + (Map[Symbol.species] === Map));
console.log("set_species=" + (Set[Symbol.species] === Set));
const spd: any = Object.getOwnPropertyDescriptor(Map, Symbol.species);
console.log("species_get=" + typeof spd.get);
console.log("species_set=" + (spd.set === undefined));
console.log("species_name=" + spd.get.name);
console.log("species_flags=" + spd.enumerable + ":" + spd.configurable);

// --- the weak collections have no species ---
console.log("weakmap_species=" + (Object.getOwnPropertyDescriptor(WeakMap, Symbol.species) === undefined));
console.log("weakset_species=" + (Object.getOwnPropertyDescriptor(WeakSet, Symbol.species) === undefined));

// --- Symbol.iterator is an alias, not a separate function ---
console.log("map_iter_is_entries=" + ((Map.prototype as any)[Symbol.iterator] === Map.prototype.entries));
console.log("set_iter_is_values=" + ((Set.prototype as any)[Symbol.iterator] === Set.prototype.values));
console.log("set_keys_is_values=" + (Set.prototype.keys === Set.prototype.values));
console.log("map_keys_is_values=" + ((Map.prototype.keys as any) === (Map.prototype.values as any)));
console.log("iter_name=" + (Map.prototype as any)[Symbol.iterator].name);
console.log("set_iter_name=" + (Set.prototype as any)[Symbol.iterator].name);

// --- the weak collections are not iterable ---
console.log("weakmap_iterable=" + ((WeakMap.prototype as any)[Symbol.iterator] === undefined));
console.log("weakset_iterable=" + ((WeakSet.prototype as any)[Symbol.iterator] === undefined));

// --- constructor arity and name ---
console.log("map_length=" + Map.length + ":" + Map.name);
console.log("set_length=" + Set.length + ":" + Set.name);
console.log("weakmap_length=" + WeakMap.length + ":" + WeakMap.name);
console.log("map_prototype_writable=" + (Object.getOwnPropertyDescriptor(Map, "prototype") as any).writable);

// --- constructor back-pointer ---
console.log("map_ctor=" + (Map.prototype.constructor === Map));
console.log("instance_ctor_name=" + new Map().constructor.name);
console.log("proto_of_map=" + (Object.getPrototypeOf(Map) === Function.prototype));
