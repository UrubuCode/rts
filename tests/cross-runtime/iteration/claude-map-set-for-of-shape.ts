// Cross-runtime: for-of over Map/Set and the SHAPE of what each yields.
// Focus: Map yields [k,v] pairs, Set yields values; insertion order; the
// default Symbol.iterator of each.

const m = new Map<string, number>();
m.set("a", 1);
m.set("b", 2);
m.set("c", 3);

// 1) for-of over a Map yields 2-element arrays
const pairs: string[] = [];
for (const p of m) pairs.push(String(Array.isArray(p)) + ":" + p.length + ":" + p[0] + "=" + p[1]);
console.log("mapPairs=" + pairs.join("|"));

// 2) destructuring the pair in the for-of head
const kv: string[] = [];
for (const [k, v] of m) kv.push(k + "=" + v);
console.log("mapDestructured=" + kv.join(","));

// 3) Map's default iterator IS entries()
console.log("mapDefaultIsEntries=" + (m[Symbol.iterator] === m.entries));

// 4) keys() / values() / entries()
console.log("mapKeys=" + [...m.keys()].join(","));
console.log("mapValues=" + [...m.values()].join(","));
console.log("mapEntries=" + [...m.entries()].map((e) => e[0] + e[1]).join(","));

// 5) insertion order survives an overwrite (does NOT move to the end)
const m2 = new Map<string, number>();
m2.set("x", 1);
m2.set("y", 2);
m2.set("x", 99);
console.log("overwriteOrder=" + [...m2.keys()].join(","));

// 6) delete + re-add DOES move to the end
const m3 = new Map<string, number>();
m3.set("p", 1);
m3.set("q", 2);
m3.delete("p");
m3.set("p", 3);
console.log("readdOrder=" + [...m3.keys()].join(","));

// 7) an empty Map iterates zero times
let emptyMapN = 0;
for (const _p of new Map()) emptyMapN++;
console.log("emptyMap=" + emptyMapN);

const s = new Set<number>([10, 20, 30]);

// 8) for-of over a Set yields the VALUES (not pairs)
const svals: number[] = [];
for (const v of s) svals.push(v);
console.log("setValues=" + svals.join(","));

// 9) Set's default iterator IS values()
console.log("setDefaultIsValues=" + (s[Symbol.iterator] === s.values));

// 10) Set's keys() === values()
console.log("setKeysIsValues=" + (s.keys === s.values));

// 11) Set entries() yields [v, v]
console.log("setEntries=" + [...s.entries()].map((e) => e[0] + ":" + e[1]).join(","));

// 12) Set dedups, and keeps first-insertion order
const s2 = new Set([3, 1, 3, 2, 1]);
console.log("setDedup=" + [...s2].join(",") + "|size=" + s2.size);

// 13) manual drive of a Map iterator past done
const mi = m.entries();
console.log("mi1=" + JSON.stringify(mi.next().value));
console.log("mi2=" + JSON.stringify(mi.next().value));
console.log("mi3=" + JSON.stringify(mi.next().value));
const miDone = mi.next();
console.log("mi4=" + String(miDone.value) + "|done=" + miDone.done);

// 14) the iterator object is itself iterable (returns this)
const si = s.values();
console.log("iterSelfIterable=" + (si[Symbol.iterator]() === si));

// 15) spread of a Map gives an array of pairs
const spreadPairs = [...m];
console.log("spreadPairsLen=" + spreadPairs.length + "|first=" + spreadPairs[0].join("="));

// 16) Array.from(map) matches spread
console.log("fromMatchesSpread=" + (JSON.stringify(Array.from(m)) === JSON.stringify([...m])));
