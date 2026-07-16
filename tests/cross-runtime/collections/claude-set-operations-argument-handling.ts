// Cross-runtime: the Set operation family — result ORDER, result identity, and
// how the argument (Set vs set-like) is handled. One thing: the 7 new Set ops.

const a = new Set([1, 2, 3, 4]);
const b = new Set([3, 4, 5, 6]);

// --- union: this-order first, then the argument's new elements ---
const u = a.union(b);
console.log("union=" + [...u].join(","));
console.log("union_reverse=" + [...b.union(a)].join(","));
console.log("union_is_new=" + (u === a) + ":" + (u === b));
console.log("union_is_set=" + (u instanceof Set));
console.log("union_sources_intact=" + a.size + ":" + b.size);

// --- intersection: order follows THIS when this is smaller/equal ---
console.log("inter=" + [...a.intersection(b)].join(","));
console.log("inter_reverse=" + [...b.intersection(a)].join(","));
console.log("inter_empty=" + [...a.intersection(new Set([99]))].join(",") + ":size=" + a.intersection(new Set([99])).size);

// --- difference: this minus argument ---
console.log("diff=" + [...a.difference(b)].join(","));
console.log("diff_reverse=" + [...b.difference(a)].join(","));
console.log("diff_all=" + a.difference(a).size);

// --- symmetricDifference: this-only then argument-only ---
console.log("symdiff=" + [...a.symmetricDifference(b)].join(","));
console.log("symdiff_reverse=" + [...b.symmetricDifference(a)].join(","));
console.log("symdiff_self=" + a.symmetricDifference(a).size);

// --- predicates ---
console.log("subset_true=" + new Set([2, 3]).isSubsetOf(a));
console.log("subset_false=" + new Set([2, 9]).isSubsetOf(a));
console.log("subset_self=" + a.isSubsetOf(a));
console.log("subset_empty=" + new Set().isSubsetOf(a));
console.log("superset_true=" + a.isSupersetOf(new Set([1, 2])));
console.log("superset_false=" + a.isSupersetOf(new Set([1, 99])));
console.log("superset_empty=" + a.isSupersetOf(new Set()));
console.log("disjoint_true=" + a.isDisjointFrom(new Set([7, 8])));
console.log("disjoint_false=" + a.isDisjointFrom(b));
console.log("disjoint_empty=" + a.isDisjointFrom(new Set()));

// --- empty receiver ---
const e = new Set<number>();
console.log("empty_union=" + [...e.union(b)].join(","));
console.log("empty_inter=" + e.intersection(b).size);
console.log("empty_diff=" + e.difference(b).size);
console.log("empty_symdiff=" + [...e.symmetricDifference(b)].join(","));

// --- ops accept a Map (set-like: size + has + keys) ---
const mapArg = new Map([[3, "x"], [4, "y"], [9, "z"]]);
console.log("union_with_map=" + [...a.union(mapArg as any)].join(","));
console.log("inter_with_map=" + [...a.intersection(mapArg as any)].join(","));
console.log("diff_with_map=" + [...a.difference(mapArg as any)].join(","));
console.log("subset_with_map=" + new Set([3, 4]).isSubsetOf(mapArg as any));
console.log("disjoint_with_map=" + new Set([1, 2]).isDisjointFrom(mapArg as any));

// --- custom set-like object works ---
const setLike = {
  size: 2,
  has: (v: number) => v === 2 || v === 3,
  keys: function* () { yield 2; yield 3; },
};
console.log("union_setlike=" + [...a.union(setLike as any)].join(","));
console.log("inter_setlike=" + [...a.intersection(setLike as any)].join(","));
console.log("subset_setlike=" + new Set([2]).isSubsetOf(setLike as any));

// --- a plain Array is NOT set-like: throws ---
try {
  (a as any).union([5, 6]);
  console.log("array_arg=no_throw");
} catch (err: any) {
  console.log("array_arg_throws=" + (err instanceof TypeError));
}

// --- SameValueZero inside the ops ---
const nz = new Set([NaN, -0]);
console.log("nan_inter=" + [...nz.intersection(new Set([NaN]))].length);
console.log("zero_inter_1div=" + (1 / [...nz.intersection(new Set([0]))][0]));
console.log("nan_diff=" + nz.difference(new Set([NaN])).size);

// --- chaining ops ---
console.log("chained=" + [...a.union(b).difference(new Set([1, 6]))].join(","));
console.log("chained2=" + [...a.intersection(b).union(new Set([0]))].join(","));

// --- results are independent copies ---
const base = new Set([1, 2]);
const derived = base.union(new Set([3]));
base.add(99);
console.log("derived_stable=" + [...derived].join(",") + ":base=" + [...base].join(","));
