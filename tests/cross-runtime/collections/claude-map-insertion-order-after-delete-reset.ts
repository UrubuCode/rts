// Cross-runtime: Map/Set insertion order — a re-set after delete moves the key
// to the END; a plain overwrite of an existing key keeps its ORIGINAL position.

// --- overwrite keeps position ---
const m1 = new Map([["a", 1], ["b", 2], ["c", 3]]);
m1.set("a", 99);
console.log("overwrite_keys=" + [...m1.keys()].join(","));
console.log("overwrite_vals=" + [...m1.values()].join(","));

// --- delete + re-set moves to end ---
const m2 = new Map([["a", 1], ["b", 2], ["c", 3]]);
m2.delete("a");
m2.set("a", 1);
console.log("readd_keys=" + [...m2.keys()].join(","));
console.log("readd_vals=" + [...m2.values()].join(","));

// --- delete middle, re-add ---
const m3 = new Map([["a", 1], ["b", 2], ["c", 3]]);
m3.delete("b");
console.log("mid_deleted=" + [...m3.keys()].join(","));
m3.set("b", 20);
console.log("mid_readd=" + [...m3.keys()].join(","));

// --- delete last, re-add stays last ---
const m4 = new Map([["a", 1], ["b", 2], ["c", 3]]);
m4.delete("c");
m4.set("c", 30);
console.log("last_readd=" + [...m4.keys()].join(","));

// --- delete all then re-add in reverse ---
const m5 = new Map([["a", 1], ["b", 2], ["c", 3]]);
m5.delete("a"); m5.delete("b"); m5.delete("c");
console.log("all_gone=" + m5.size);
m5.set("c", 3); m5.set("b", 2); m5.set("a", 1);
console.log("reverse_readd=" + [...m5.keys()].join(","));

// --- clear() then re-add ---
const m6 = new Map([["a", 1], ["b", 2]]);
m6.clear();
m6.set("b", 2); m6.set("a", 1);
console.log("after_clear=" + [...m6.keys()].join(","));

// --- repeated delete/re-set cycling ---
const m7 = new Map([["x", 1], ["y", 2], ["z", 3]]);
m7.delete("x"); m7.set("x", 1);
m7.delete("y"); m7.set("y", 2);
console.log("cycle=" + [...m7.keys()].join(","));

// --- entries() reflects the same order ---
const m8 = new Map([["a", 1], ["b", 2], ["c", 3]]);
m8.delete("a");
m8.set("a", 100);
console.log("entries=" + [...m8.entries()].map(e => e[0] + ":" + e[1]).join("|"));
const fe: string[] = [];
m8.forEach((v, k) => fe.push(k));
console.log("foreach_order=" + fe.join(","));

// --- Set: same rules ---
const s1 = new Set([1, 2, 3]);
s1.add(1); // no-op, keeps position
console.log("set_readd_noop=" + [...s1].join(","));

const s2 = new Set([1, 2, 3]);
s2.delete(1);
s2.add(1);
console.log("set_delete_readd=" + [...s2].join(","));

const s3 = new Set(["a", "b", "c"]);
s3.delete("b");
s3.add("b");
s3.delete("a");
s3.add("a");
console.log("set_cycle=" + [...s3].join(","));

// --- object keys keep identity-based order ---
const ka = { n: "a" }, kb = { n: "b" };
const m9 = new Map<any, number>([[ka, 1], [kb, 2]]);
m9.delete(ka);
m9.set(ka, 1);
console.log("obj_order=" + [...m9.keys()].map((k: any) => k.n).join(","));
