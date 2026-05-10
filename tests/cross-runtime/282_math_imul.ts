// Cross-runtime: Math.imul wraparound behavior.
console.log("a=" + Math.imul(2, 4));
console.log("b=" + Math.imul(-1, 8));
console.log("c=" + Math.imul(0xffffffff, 5));
console.log("d=" + Math.imul(0x7fffffff, 2));
