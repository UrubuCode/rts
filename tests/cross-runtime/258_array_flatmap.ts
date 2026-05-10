// Cross-runtime: Array.prototype.flatMap basics and edges.
const base = [1, 2, 3];
console.log("basic=" + base.flatMap((n) => [n, n * 10]).join(","));
console.log("empty=" + base.flatMap((n) => (n % 2 ? [] : [n])).join(","));
console.log("scalar=" + base.flatMap((n) => n + 1).join(","));
console.log("nested=" + [1, 2].flatMap((n) => [[n, n + 1]]).map((x) => JSON.stringify(x)).join("|"));
