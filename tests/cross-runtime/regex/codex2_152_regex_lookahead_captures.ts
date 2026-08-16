// Cross-runtime: lookahead captures text without consuming it.
const re = /(?=(\w{2}))/g;
const matches = [..."abcd".matchAll(re)];
console.log(matches.map((m) => m[1] + "@" + m.index).join("|"));
console.log(matches.length);

