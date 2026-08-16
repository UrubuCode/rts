// Cross-runtime: integer toString respects non-decimal radices.
const n = 123456789;
console.log([n.toString(2), n.toString(8), n.toString(16), n.toString(36)].join("|"));
console.log(parseInt(n.toString(36), 36) === n);

