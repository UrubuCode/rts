// Cross-runtime: Array.prototype.at with positive and negative indices.
const a = [10, 20, 30, 40, 50];
console.log(a.at(0));
console.log(a.at(2));
console.log(a.at(-1));
console.log(a.at(-2));
console.log(a.at(10));
console.log(a.at(-10));
console.log([].at(0));
console.log([].at(-1));
