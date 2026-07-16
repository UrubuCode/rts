// Cross-runtime: return values of the mutators — Set.add / Map.set return the
// collection itself (chainable); delete returns boolean; clear returns undefined.

// --- Set.add returns the same set instance ---
const s = new Set<number>();
const r = s.add(1);
console.log("add_returns_self=" + (r === s));
console.log("add_returns_set=" + (r instanceof Set));

// --- chaining adds ---
const s2 = new Set<number>();
s2.add(1).add(2).add(3);
console.log("chain=" + [...s2].join(","));
console.log("chain_size=" + s2.size);

// --- chaining with duplicates still returns self ---
const s3 = new Set<number>();
s3.add(1).add(1).add(2).add(1);
console.log("chain_dup=" + [...s3].join(",") + ":size=" + s3.size);

// --- add on a fresh set inline, then chain into spread ---
console.log("inline=" + [...new Set<string>().add("a").add("b")].join(","));

// --- size read off the chain result ---
console.log("chain_size_inline=" + new Set<number>().add(9).add(8).size);

// --- Map.set returns the same map instance ---
const m = new Map<string, number>();
const mr = m.set("a", 1);
console.log("set_returns_self=" + (mr === m));
console.log("set_returns_map=" + (mr instanceof Map));

// --- chaining sets ---
const m2 = new Map<string, number>();
m2.set("a", 1).set("b", 2).set("c", 3);
console.log("map_chain=" + [...m2.entries()].map(e => e[0] + ":" + e[1]).join("|"));

// --- chained overwrite keeps position ---
const m3 = new Map<string, number>();
m3.set("a", 1).set("b", 2).set("a", 99);
console.log("chain_overwrite=" + [...m3.keys()].join(",") + ":a=" + m3.get("a"));

// --- get off the chain result ---
console.log("chain_get=" + new Map<string, number>().set("k", 5).get("k"));

// --- delete returns boolean, NOT the collection ---
const s4 = new Set([1, 2]);
console.log("set_delete_hit=" + s4.delete(1));
console.log("set_delete_miss=" + s4.delete(99));
const m4 = new Map([["a", 1]]);
console.log("map_delete_hit=" + m4.delete("a"));
console.log("map_delete_miss=" + m4.delete("zz"));

// --- clear returns undefined ---
const s5 = new Set([1, 2]);
console.log("set_clear=" + String(s5.clear()) + ":size=" + s5.size);
const m5 = new Map([["a", 1]]);
console.log("map_clear=" + String(m5.clear()) + ":size=" + m5.size);

// --- has returns boolean ---
const s6 = new Set([1]);
console.log("set_has=" + s6.has(1) + ":" + s6.has(2));
const m6 = new Map([["a", 1]]);
console.log("map_has=" + m6.has("a") + ":" + m6.has("b"));

// --- long chain, mixed values ---
const s7 = new Set<any>();
s7.add(0).add("0").add(false).add(null).add(undefined).add(NaN);
console.log("mixed_chain_size=" + s7.size);
console.log("mixed_has=" + s7.has(0) + s7.has("0") + s7.has(false) + s7.has(null) + s7.has(undefined) + s7.has(NaN));

// --- chain result feeds another collection ---
const built = new Map<string, number>().set("x", 1).set("y", 2);
console.log("feed_keys=" + [...new Set(built.keys())].join(","));

// --- chained add on a set built from a set ---
console.log("nested=" + [...new Set(new Set([1, 2])).add(3)].join(","));
