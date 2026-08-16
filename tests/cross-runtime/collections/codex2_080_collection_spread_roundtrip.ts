// Cross-runtime: spreading through Set and Map preserves collection iteration order.
const unique = [...new Set([3, 1, 3, 2, 1])];
const indexed = new Map(unique.map((v, i) => [v, i]));
console.log(unique.join(","));
console.log(JSON.stringify([...indexed.entries()]));
console.log([...indexed.keys()].join(","));

