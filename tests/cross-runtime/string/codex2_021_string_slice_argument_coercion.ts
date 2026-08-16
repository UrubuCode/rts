// Cross-runtime: slice clamps and coerces fractional and infinite indexes.
const s = "abcdefgh";
console.log([s.slice(2.9, 6.2), s.slice(-5, -1), s.slice(-99, 99)].join("|"));
console.log([s.slice(Infinity), s.slice(-Infinity, 3)].join("|"));

