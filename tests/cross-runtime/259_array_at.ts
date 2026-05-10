// Cross-runtime: Array.prototype.at positive and negative indexes.
const arr = ["a", "b", "c"];
console.log("p1=" + String(arr.at(1)));
console.log("n1=" + String(arr.at(-1)));
console.log("n2=" + String(arr.at(-2)));
console.log("z=" + String(arr.at(0)));
console.log("oor=" + String(arr.at(9)));
