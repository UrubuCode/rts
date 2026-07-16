// Cross-runtime: entries()/keys()/values() return INDEPENDENT, live iterators.
// One thing: iterator object semantics (independence, laziness, exhaustion).

const m = new Map([["a", 1], ["b", 2], ["c", 3]]);

// --- two iterators from the same map advance independently ---
const it1 = m.keys();
const it2 = m.keys();
console.log("it1_first=" + it1.next().value);
console.log("it2_first=" + it2.next().value);
console.log("it1_second=" + it1.next().value);
console.log("it2_still_second=" + it2.next().value);
console.log("distinct_objects=" + (it1 === it2));

// --- keys/values/entries are different iterator objects ---
const ks = m.keys(), vs = m.values(), es = m.entries();
console.log("k_v_distinct=" + (ks === vs));
console.log("k_first=" + ks.next().value);
console.log("v_first=" + vs.next().value);
const e0 = es.next().value;
console.log("e_first=" + e0[0] + ":" + e0[1]);

// --- exhaustion: done flag and value after end ---
const it3 = new Map([["x", 1]]).keys();
const r1 = it3.next();
console.log("r1=" + r1.value + ":" + r1.done);
const r2 = it3.next();
console.log("r2=" + String(r2.value) + ":" + r2.done);
const r3 = it3.next();
console.log("r3_still_done=" + String(r3.value) + ":" + r3.done);

// --- an exhausted iterator stays exhausted even if map grows ---
const m2 = new Map([["a", 1]]);
const it4 = m2.keys();
it4.next();
console.log("it4_done=" + it4.next().done);
m2.set("b", 2);
console.log("it4_after_growth=" + String(it4.next().value) + ":" + it4.next().done);

// --- a LIVE (not yet exhausted) iterator sees later insertions ---
const m3 = new Map([["a", 1]]);
const it5 = m3.keys();
console.log("it5_a=" + it5.next().value);
m3.set("b", 2);
console.log("it5_sees_b=" + it5.next().value);
console.log("it5_done=" + it5.next().done);

// --- iterator sees a delete that happens before it reaches the key ---
const m4 = new Map([["a", 1], ["b", 2], ["c", 3]]);
const it6 = m4.keys();
console.log("it6_a=" + it6.next().value);
m4.delete("b");
console.log("it6_skips_b=" + it6.next().value);

// --- iterators are themselves iterable (Symbol.iterator returns self) ---
const it7 = m.keys();
console.log("iterator_self=" + (it7[Symbol.iterator]() === it7));
console.log("spread_iterator=" + [...m.keys()].join(","));

// --- partially consumed iterator spreads only the REST ---
const it8 = m.keys();
it8.next();
console.log("rest_spread=" + [...it8].join(","));

// --- map[Symbol.iterator] === map.entries ---
console.log("map_iter_is_entries=" + (m[Symbol.iterator] === m.entries));
const de = [...m][0];
console.log("default_iter_pair=" + de[0] + ":" + de[1]);

// --- fresh iterator after full consumption restarts ---
console.log("fresh1=" + [...m.keys()].join(","));
console.log("fresh2=" + [...m.keys()].join(","));

// --- entries() pairs are fresh arrays each time ---
const p1 = m.entries().next().value;
const p2 = m.entries().next().value;
console.log("pairs_not_shared=" + (p1 === p2));
console.log("pairs_equal=" + (p1[0] === p2[0] && p1[1] === p2[1]));

// --- Set: keys() and values() are the same function; entries() gives [v,v] ---
const s = new Set([10, 20]);
console.log("set_keys_is_values=" + (s.keys === s.values));
console.log("set_iter_is_values=" + (s[Symbol.iterator] === s.values));
const se = s.entries().next().value;
console.log("set_entry=" + se[0] + ":" + se[1] + ":" + (se[0] === se[1]));
const sit1 = s.values(), sit2 = s.values();
console.log("set_indep=" + sit1.next().value + ":" + sit2.next().value + ":" + sit1.next().value);
