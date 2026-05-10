// Cross-runtime: Object.fromEntries from various sources.
const map = new Map([["a", 1], ["b", 2]]);
console.log("map=" + JSON.stringify(Object.fromEntries(map)));
console.log("arr=" + JSON.stringify(Object.fromEntries(Array.from(map))));
console.log("tuples=" + JSON.stringify(Object.fromEntries([["x", 7], ["y", 8]])));
const rt = Object.fromEntries(Object.entries({ p: 1, q: 2 }));
console.log("round=" + JSON.stringify(rt));
