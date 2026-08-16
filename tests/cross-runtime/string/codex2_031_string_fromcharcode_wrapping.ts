// Cross-runtime: fromCharCode wraps inputs to unsigned UTF-16 code units.
const s = String.fromCharCode(65, 0x10041, -1, 3.9);
console.log(s.length);
console.log(Array.from(s).map((c) => c.charCodeAt(0)).join(","));

