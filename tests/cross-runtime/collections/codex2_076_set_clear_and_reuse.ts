// Cross-runtime: a cleared Set can be reused and chained.
const s = new Set([1, 2, 3]);
s.clear();
console.log(s.size, [...s].length, s.has(1));
const returned = s.add(4).add(5);
console.log(returned === s, [...s].join(","));

