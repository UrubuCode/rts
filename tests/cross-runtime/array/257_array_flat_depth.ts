// Cross-runtime: Array.flat with different depths
const nested = [1, [2, [3, [4, [5]]]]];
console.log("depth0=" + nested.flat(0).join(","));
console.log("depth1=" + nested.flat(1).join(","));
console.log("depth2=" + nested.flat(2).join(","));
console.log("depth3=" + nested.flat(3).join(","));
console.log("depthInf=" + nested.flat(Infinity).join(","));

const simple = [1, 2, 3];
console.log("simple=" + simple.flat().join(","));
