// Cross-runtime: `size` on Map/Set is a PROTOTYPE accessor with a brand check —
// never an own data property of the instance, and never present on the weak
// collections at all.

const m = new Map([["a", 1], ["b", 2]]);
const s = new Set([1, 2, 3]);

// --- reads fine, but the instance owns nothing ---
console.log("map_size=" + m.size);
console.log("set_size=" + s.size);
console.log("map_own_size=" + Object.prototype.hasOwnProperty.call(m, "size"));
console.log("map_own_names=" + Object.getOwnPropertyNames(m).length);
console.log("map_own_symbols=" + Object.getOwnPropertySymbols(m).length);
console.log("map_keys_enum=" + Object.keys(m).length);
console.log("map_in=" + ("size" in m));
console.log("map_json=" + JSON.stringify(m));
console.log("set_json=" + JSON.stringify(s));

// --- the descriptor lives on the prototype ---
const md: any = Object.getOwnPropertyDescriptor(Map.prototype, "size");
console.log("map_desc_get=" + typeof md.get);
console.log("map_desc_set=" + (md.set === undefined));
console.log("map_desc_value=" + (md.value === undefined));
console.log("map_desc_enumerable=" + md.enumerable);
console.log("map_desc_configurable=" + md.configurable);
console.log("map_getter_name=" + md.get.name);
console.log("map_getter_length=" + md.get.length);

const sd: any = Object.getOwnPropertyDescriptor(Set.prototype, "size");
console.log("set_desc_get=" + typeof sd.get);
console.log("set_getter_name=" + sd.get.name);
console.log("set_desc_flags=" + sd.enumerable + ":" + sd.configurable);

// --- the getter is not shared between Map and Set ---
console.log("getters_distinct=" + (md.get === sd.get));

// --- brand check: the getter refuses a foreign receiver ---
try { md.get.call({}); console.log("map_getter_plain=no_throw"); }
catch (e: any) { console.log("map_getter_plain=" + e.constructor.name); }
try { md.get.call(s); console.log("map_getter_on_set=no_throw"); }
catch (e: any) { console.log("map_getter_on_set=" + e.constructor.name); }
try { sd.get.call(m); console.log("set_getter_on_map=no_throw"); }
catch (e: any) { console.log("set_getter_on_map=" + e.constructor.name); }
try { md.get.call(null); console.log("map_getter_null=no_throw"); }
catch (e: any) { console.log("map_getter_null=" + e.constructor.name); }

// --- Map.prototype is NOT itself a Map, so the getter refuses it too ---
try { console.log("proto_size=" + (Map.prototype as any).size); }
catch (e: any) { console.log("proto_size=" + e.constructor.name); }
try { console.log("set_proto_size=" + (Set.prototype as any).size); }
catch (e: any) { console.log("set_proto_size=" + e.constructor.name); }

// --- a subclass inherits the accessor unchanged ---
class Sub extends Map<any, any> {}
const sub = new Sub([["x", 1]]);
console.log("sub_size=" + sub.size);
console.log("sub_own=" + Object.prototype.hasOwnProperty.call(sub, "size"));
console.log("sub_desc_here=" + (Object.getOwnPropertyDescriptor(Sub.prototype, "size") === undefined));

// --- shadowing with an own property wins over the accessor ---
const shadow: any = new Map([["a", 1]]);
Object.defineProperty(shadow, "size", { value: 99, configurable: true });
console.log("shadowed=" + shadow.size);
console.log("shadowed_real=" + md.get.call(shadow));
delete shadow.size;
console.log("unshadowed=" + shadow.size);

// --- the weak collections have no size at all ---
console.log("weakmap_size_in=" + ("size" in new WeakMap()));
console.log("weakset_size_in=" + ("size" in new WeakSet()));
console.log("weakmap_desc=" + (Object.getOwnPropertyDescriptor(WeakMap.prototype, "size") === undefined));
console.log("weakmap_size=" + (new WeakMap() as any).size);
