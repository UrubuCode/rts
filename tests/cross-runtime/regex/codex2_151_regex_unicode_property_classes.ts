// Cross-runtime: Unicode property escapes distinguish letters and numbers.
const s = "Aé中-12-😀";
console.log(JSON.stringify(s.match(/\p{Letter}+/gu)));
console.log(JSON.stringify(s.match(/\p{Number}+/gu)));
console.log(JSON.stringify(s.match(/\P{ASCII}+/gu)));

