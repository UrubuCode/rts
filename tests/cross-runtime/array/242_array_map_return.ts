// Cross-runtime: Array.map requires return value
const arr = [1, 2, 3];
const mapped = arr.map(x => x * 2);
console.log("mapped=" + mapped.join(","));

const noReturn = arr.map(x => { x * 2; });
console.log("noReturn=" + noReturn.join(","));

const explicit = arr.map(x => { return x * 2; });
console.log("explicit=" + explicit.join(","));

const mixed = arr.map(x => x > 1 ? x * 2 : x);
console.log("mixed=" + mixed.join(","));
