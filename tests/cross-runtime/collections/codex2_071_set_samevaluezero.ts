// Cross-runtime: Set uses SameValueZero for NaN and signed zero.
const s = new Set<any>([NaN, NaN, -0, 0, "0"]);
console.log(s.size, s.has(Number("bad")), s.has(-0));
console.log([...s].map((x) => typeof x + ":" + String(x)).join("|"));

