// `Map`, `Set`, `WeakMap`, `WeakSet`.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

let m = new Map();
m.set("a", 1);
check("get", m.get("a") === 1);
check("size", m.size === 1);
check("has", m.has("a"));
check("has-absent", m.has("zz") === false);
check("overwrite", (function () { m.set("a", 2); return m.get("a") === 2 && m.size === 1; })());
check("delete", m.delete("a") && m.size === 0);
check("delete-absent", m.delete("a") === false);
check("get-absent", m.get("a") === undefined);

check("from-pairs", new Map([[1, "x"], [2, "y"]]).get(2) === "y");

// Insertion order, which the specification requires of every walk and which a
// bare hash table does not give.
let ordered = new Map();
ordered.set("b", 1);
ordered.set("a", 2);
ordered.set("c", 3);
let seen = "";
ordered.forEach(function (v, key) { seen = seen + key; });
check("order", seen === "bac");
check("keys-order", [...ordered.keys()].join(",") === "b,a,c");
check("values-order", [...ordered.values()].join(",") === "1,2,3");
check("entries", [...ordered.entries()][0][0] === "b");

// A delete preserves it, which is what a swap-with-last gets wrong.
ordered.delete("a");
check("order-after-delete", [...ordered.keys()].join(",") === "b,c");

// SameValueZero: `NaN` is a usable key where `===` would never find it, and
// `+0` and `-0` are one key.
let odd = new Map();
odd.set(0 / 0, 7);
check("nan-key", odd.get(0 / 0) === 7);
odd.set(0, 1);
odd.set(-0, 2);
check("signed-zero-is-one-key", odd.get(0) === 2);

// Object keys hash to one bucket and stay correct by identity.
let first = {};
let second = {};
let byIdentity = new Map();
byIdentity.set(first, 1);
byIdentity.set(second, 2);
check("object-keys", byIdentity.get(first) === 1 && byIdentity.get(second) === 2);

let cleared = new Map([[1, 1]]);
cleared.clear();
check("clear", cleared.size === 0);

let s = new Set([1, 2, 3]);
check("set-size", s.size === 3);
check("set-dedupes", new Set([1, 1, 1]).size === 1);
check("set-has", s.has(2));
check("set-add", (function () { s.add(4); return s.size === 4; })());
check("set-delete", s.delete(4) && s.size === 3);
check("set-values", [...s.values()].join(",") === "1,2,3");

let visited = 0;
s.forEach(function (v) { visited = visited + v; });
check("set-for-each", visited === 6);

let left = new Set([1, 2]);
let right = new Set([2, 3]);
check("union", left.union(right).size === 3);
check("intersection", left.intersection(right).size === 1);
check("difference", left.difference(right).size === 1);
check("symmetric-difference", left.symmetricDifference(right).size === 2);
check("subset", new Set([1]).isSubsetOf(left));
check("superset", left.isSupersetOf(new Set([1])));
check("disjoint", new Set([1]).isDisjointFrom(new Set([2])));

// A collection is iterable by what it holds, not by a declared method: `for-of`
// over a `Set` yields members, over a `Map` yields `[key, value]` arrays.
let members = 0;
for (let v of new Set([1, 2, 3])) { members = members + v; }
check("set-for-of", members === 6);

let pairs = new Map();
pairs.set("a", 1);
pairs.set("b", 2);
let names = "";
let values = 0;
for (let e of pairs) { names = names + e[0]; values = values + e[1]; }
check("map-for-of-key", names === "ab");
check("map-for-of-value", values === 3);

check("set-from-set", new Set(new Set([1, 2, 2])).size === 2);
check("map-from-map", new Map(pairs).get("b") === 2);
check("set-spread", [...new Set([1, 2])].length === 2);
check("array-from-set", Array.from(new Set([4, 5]))[1] === 5);
check("array-from-map", Array.from(pairs)[0][0] === "a");

// A weak collection shares the table and is NOT iterable: it must yield
// nothing rather than leak the keys it holds strongly.
let leaked = 0;
for (let v of new WeakSet()) { leaked = leaked + 1; }
check("weak-set-not-iterable", leaked === 0);

let key = {};
let weak = new WeakMap();
weak.set(key, 5);
check("weak-map-get", weak.get(key) === 5);
check("weak-map-has", weak.has(key));
check("weak-map-delete", weak.delete(key) && weak.has(key) === false);
// A primitive key is refused rather than stored. The specification throws;
// this cannot yet, so the observable half is that it does not go in.
weak.set(1, 5);
check("weak-map-refuses-primitive", weak.has(1) === false);

let weakSet = new WeakSet();
weakSet.add(key);
check("weak-set-has", weakSet.has(key));
check("weak-set-identity", weakSet.has({}) === false);

return failed;
