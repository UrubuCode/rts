// Cross-runtime compatibility: Symbol basics and registry.
const s1 = Symbol("alpha");
const s2 = Symbol.for("shared-key");
const s3 = Symbol.for("shared-key");
const obj = { [s1]: 7, [s2]: 9 };

console.log("desc=" + String(s1.description));
console.log("shared=" + (s2 === s3));
console.log("keyFor=" + String(Symbol.keyFor(s2)));
console.log("symbols=" + Object.getOwnPropertySymbols(obj).length);
console.log("value=" + obj[s2]);
