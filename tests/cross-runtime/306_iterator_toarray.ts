// Cross-runtime: Iterator.toArray consumes iterator.
const it = Iterator.from([1, 2, 3]);
console.log("a=" + it.toArray().join(","));
console.log("b=" + it.toArray().join(","));
