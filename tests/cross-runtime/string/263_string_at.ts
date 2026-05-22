// Cross-runtime: String.prototype.at with negatives and bounds.
const s = "abc";
console.log("p1=" + String(s.at(1)));
console.log("n1=" + String(s.at(-1)));
console.log("n2=" + String(s.at(-2)));
console.log("oor=" + String(s.at(9)));
console.log("empty=" + String("".at(0)));
