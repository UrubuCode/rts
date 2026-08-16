// Cross-runtime: trim variants remove ECMAScript whitespace at selected ends.
const s = "\u00a0\u2003 left middle right \ufeff\n";
console.log(JSON.stringify(s.trim()));
console.log(JSON.stringify(s.trimStart()));
console.log(JSON.stringify(s.trimEnd()));

