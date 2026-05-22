// Cross-runtime: Iterator helpers chain.
const it = Iterator.from([1, 2, 3, 4]).map((x) => x * 2).filter((x) => x > 4).take(2);
console.log("arr=" + it.toArray().join(","));
console.log("red=" + Iterator.from([1, 2, 3]).drop(1).reduce((a, b) => a + b, 0));
