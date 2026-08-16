// Cross-runtime: each Map entry iteration result is a distinct mutable array.
const entries = [...new Map([["a", 1], ["b", 2]]).entries()];
console.log(Array.isArray(entries[0]), entries[0] === entries[1]);
entries[0][1] = 9;
console.log(JSON.stringify(entries));

