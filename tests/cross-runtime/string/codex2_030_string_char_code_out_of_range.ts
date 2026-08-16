// Cross-runtime: character access methods differ outside the string range.
const s = "A😀Z";
console.log([s.charAt(-1), s.charAt(99), s.charAt(1)].map(JSON.stringify).join("|"));
console.log([s.charCodeAt(-1), s.charCodeAt(1), s.charCodeAt(2)].join("|"));
console.log(s.codePointAt(1), s.codePointAt(2), s.codePointAt(99));

