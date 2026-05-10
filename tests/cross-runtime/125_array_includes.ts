// Cross-runtime: Array.includes with edge cases
const arr = [1, 2, 3, NaN, 0, -0];
console.log("has2=" + arr.includes(2));
console.log("has5=" + arr.includes(5));
console.log("hasNaN=" + arr.includes(NaN));
console.log("from2=" + arr.includes(2, 2));
console.log("from0=" + arr.includes(2, 0));
console.log("negIdx=" + arr.includes(3, -3));

const str = "hello";
console.log("str_e=" + str.includes("e"));
console.log("str_x=" + str.includes("x"));
console.log("str_from=" + str.includes("l", 3));
