// Cross-runtime: Map keys use SameValueZero for NaN and signed zero.
const m = new Map<any, string>();
m.set(NaN, "first");
m.set(Number("x"), "second");
m.set(-0, "negative");
m.set(0, "positive");
console.log(m.size, m.get(NaN), m.get(-0));
console.log([...m.keys()].map(String).join(","));

