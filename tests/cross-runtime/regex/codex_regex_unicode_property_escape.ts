// Cross-runtime: Unicode property escapes with global iteration.
const s = "AΩЖ1_🙂";
const letters = Array.from(s.matchAll(/\p{Letter}/gu)).map(m => m[0]);
const nonAscii = Array.from(s.matchAll(/\P{ASCII}/gu)).map(m => m[0]);
console.log(letters.join("|"));
console.log(nonAscii.join("|"));
