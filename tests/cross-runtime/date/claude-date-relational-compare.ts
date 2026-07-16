// Cross-runtime: relational operators on Date use valueOf (number hint),
// but == / === compare object identity, NOT the instant.

const a = new Date(Date.UTC(2024, 0, 1));
const b = new Date(Date.UTC(2024, 0, 2));
const aCopy = new Date(Date.UTC(2024, 0, 1));

// Relational operators coerce to numbers.
console.log("lt=" + (a < b));
console.log("gt=" + (a > b));
console.log("lte=" + (a <= b));
console.log("gte=" + (a >= b));
console.log("b_gt_a=" + (b > a));

// Equal instants, different objects: <= and >= are BOTH true...
console.log("copy_lte=" + (a <= aCopy));
console.log("copy_gte=" + (a >= aCopy));
console.log("copy_lt=" + (a < aCopy));
console.log("copy_gt=" + (a > aCopy));

// ...but == and === are false (identity, no numeric coercion).
console.log("copy_eqeq=" + ((a as any) == aCopy));
console.log("copy_eqeqeq=" + (a === aCopy));
console.log("self_eqeqeq=" + (a === a));

// The idiomatic instant comparison.
console.log("time_eq=" + (a.getTime() === aCopy.getTime()));
console.log("valueOf_eq=" + (a.valueOf() === aCopy.valueOf()));
console.log("plus_eq=" + (+a === +aCopy));

// != mirrors ==.
console.log("copy_noteq=" + ((a as any) != aCopy));

// Comparison against a raw number coerces the Date.
console.log("vs_number_lt=" + (a < Date.UTC(2024, 0, 2)));
console.log("vs_number_gte=" + (a >= Date.UTC(2024, 0, 1)));
console.log("eqeq_number=" + ((a as any) == Date.UTC(2024, 0, 1)));

// Invalid Date: every relational comparison is false (NaN semantics).
const bad = new Date(NaN);
console.log("bad_lt=" + (bad < a));
console.log("bad_gt=" + (bad > a));
console.log("bad_lte=" + (bad <= a));
console.log("bad_gte=" + (bad >= a));
console.log("bad_lte_self=" + (bad <= bad));
console.log("bad_eqeqeq_self=" + (bad === bad));

// Sorting by valueOf is stable and deterministic.
const list = [
  new Date(Date.UTC(2024, 0, 3)),
  new Date(Date.UTC(2024, 0, 1)),
  new Date(Date.UTC(2024, 0, 2)),
];
list.sort((x, y) => x.getTime() - y.getTime());
console.log("sorted=" + list.map((x) => x.toISOString().slice(0, 10)).join(","));

// Math.min/Math.max coerce Dates to numbers.
console.log("min=" + new Date(Math.min(+a, +b)).toISOString());
console.log("max=" + new Date(Math.max(+a, +b)).toISOString());
