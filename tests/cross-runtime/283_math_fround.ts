// Cross-runtime: Math.fround precision edges.
console.log("a=" + String(Math.fround(1.337)));
console.log("b=" + String(Math.fround(NaN)));
console.log("c=" + String(Math.fround(Infinity)));
console.log("d=" + String(Math.fround(1 / 3)));
