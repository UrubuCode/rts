// Cross-runtime: toReversed / toSorted / toSpliced / with (non-mutating array methods).
const a = [3, 1, 4, 1, 5];

const rev = a.toReversed();
console.log(rev.join(","));
console.log(a.join(","));  // original unchanged

const sorted = a.toSorted((x, y) => x - y);
console.log(sorted.join(","));
console.log(a.join(","));  // original unchanged

const spliced = a.toSpliced(1, 2, 99);
console.log(spliced.join(","));
console.log(a.join(","));  // original unchanged

const withVal = a.with(2, 99);
console.log(withVal.join(","));
console.log(a.join(","));  // original unchanged
