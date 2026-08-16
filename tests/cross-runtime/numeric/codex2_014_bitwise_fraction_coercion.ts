// Cross-runtime: bitwise operators truncate fractions and coerce non-numbers.
console.log([5.9 | 0, -5.9 | 0, "7" & 3, null ^ 5].join("|"));
console.log([~0, ~-1, ~4294967296].join("|"));

