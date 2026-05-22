// Cross-runtime: Array.prototype.findLast and findLastIndex.
const xs = [1, 4, 7, 4, 2];
console.log("last=" + String(xs.findLast((n) => n % 2 === 0)));
console.log("idx=" + String(xs.findLastIndex((n) => n % 2 === 0)));
console.log("none=" + String(xs.findLast((n) => n > 100)));
console.log("noneIdx=" + String(xs.findLastIndex((n) => n > 100)));
