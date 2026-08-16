// Cross-runtime: Unicode dot consumes an astral code point as one match.
const s = "A😀B";
console.log((s.match(/./g) || []).length);
console.log((s.match(/./gu) || []).length);
console.log(JSON.stringify(s.match(/./gu)));

