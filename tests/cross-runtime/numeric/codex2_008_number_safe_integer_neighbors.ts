// Cross-runtime: safe-integer checks distinguish adjacent representable values.
const max = Number.MAX_SAFE_INTEGER;
console.log([max - 1, max, max + 1].map(Number.isSafeInteger).join(","));
console.log(max + 1 === max + 2, max === max + 1);

