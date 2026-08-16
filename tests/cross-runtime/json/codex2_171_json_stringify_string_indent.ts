// Cross-runtime: string indentation truncates to ten code units.
const value = { a: { b: 1 } };
const out = JSON.stringify(value, null, "abcdefghijklm");
console.log(out);
console.log(out.split("\n")[1].slice(0, 12));

