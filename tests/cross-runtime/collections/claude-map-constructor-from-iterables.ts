// Cross-runtime: Map/Set constructor accepting varied iterables.
// One thing: what the constructor does with the argument it is handed.

// --- from array of pairs ---
const m1 = new Map([["a", 1], ["b", 2]]);
console.log("pairs=" + [...m1.entries()].map(e => e[0] + ":" + e[1]).join("|"));

// --- duplicate keys: last wins, first position kept ---
const m2 = new Map([["a", 1], ["b", 2], ["a", 3]]);
console.log("dup_size=" + m2.size);
console.log("dup_keys=" + [...m2.keys()].join(","));
console.log("dup_get_a=" + m2.get("a"));

// --- from another Map (copy) ---
const src = new Map([["x", 10], ["y", 20]]);
const m3 = new Map(src);
console.log("copy=" + [...m3.entries()].map(e => e[0] + ":" + e[1]).join("|"));
console.log("copy_is_distinct=" + (m3 === src));
m3.set("z", 30);
console.log("src_unaffected=" + src.size + ":" + m3.size);

// --- mutating copied source does not touch the copy ---
src.set("x", 999);
console.log("copy_x_stable=" + m3.get("x"));

// --- from map.entries() iterator ---
const m4 = new Map(new Map([["p", 1], ["q", 2]]).entries());
console.log("from_entries=" + [...m4.keys()].join(","));

// --- from empty / no arg / null / undefined ---
console.log("empty_arr=" + new Map([]).size);
console.log("no_arg=" + new Map().size);
console.log("null_arg=" + new Map(null).size);
console.log("undef_arg=" + new Map(undefined).size);

// --- pairs with extra elements: only [0] and [1] used ---
const m5 = new Map([["k", "v", "ignored"] as any]);
console.log("extra_ignored=" + m5.get("k") + ":" + m5.size);

// --- pair with missing value -> undefined ---
const m6 = new Map([["only"] as any]);
console.log("missing_val=" + m6.has("only") + ":" + String(m6.get("only")));

// --- from a Set of pairs ---
const m7 = new Map(new Set([["s", 1], ["t", 2]] as any));
console.log("from_set=" + [...m7.keys()].join(","));

// --- from a generator of pairs ---
function* gen(): any {
  yield ["g1", 1];
  yield ["g2", 2];
}
const m8 = new Map(gen());
console.log("from_gen=" + [...m8.entries()].map(e => e[0] + ":" + e[1]).join("|"));

// --- from Object.entries ---
const m9 = new Map(Object.entries({ o1: 1, o2: 2 }));
console.log("from_obj_entries=" + [...m9.entries()].map(e => e[0] + ":" + e[1]).join("|"));

// --- mixed key types preserved ---
const objKey = { id: 1 };
const m10 = new Map<any, string>([[1, "num"], ["1", "str"], [true, "bool"], [objKey, "obj"], [null, "null"]]);
console.log("mixed_size=" + m10.size);
console.log("mixed_1=" + m10.get(1) + ":" + m10.get("1"));
console.log("mixed_bool=" + m10.get(true));
console.log("mixed_obj=" + m10.get(objKey));
console.log("mixed_null=" + m10.get(null));

// --- Set constructor ---
console.log("set_arr=" + [...new Set([3, 1, 3, 2])].join(","));
console.log("set_str=" + [...new Set("hello")].join(","));
console.log("set_from_set=" + [...new Set(new Set([1, 2]))].join(","));
console.log("set_from_map_keys=" + [...new Set(new Map([["a", 1], ["b", 2]]).keys())].join(","));
console.log("set_empty=" + new Set().size + ":" + new Set([]).size + ":" + new Set(null).size);

// --- Set from generator ---
function* sgen(): any { yield 1; yield 2; yield 1; }
console.log("set_from_gen=" + [...new Set(sgen())].join(","));
