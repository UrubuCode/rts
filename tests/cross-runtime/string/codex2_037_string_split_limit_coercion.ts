// Cross-runtime: split coerces its unsigned limit and preserves empty fields.
const s = "a,,b,";
console.log(JSON.stringify(s.split(",", 10)));
console.log(JSON.stringify(s.split(",", 2.9)));
console.log(JSON.stringify(s.split(",", 0)));

