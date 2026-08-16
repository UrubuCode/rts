// Cross-runtime: fromCodePoint combines BMP and astral scalar values.
const s = String.fromCodePoint(0x41, 0x1f600, 0x20ac);
console.log(s, s.length, Array.from(s).length);
console.log(Array.from(s).map((c) => c.codePointAt(0)).join(","));

