// Cross-runtime: Array constructor with spread
const arr1 = new Array(...[1, 2, 3]);
console.log("spread=" + arr1.join(","));

const arr2 = Array(...[4, 5]);
console.log("func=" + arr2.join(","));

const single = new Array(...[10]);
console.log("single=" + single.join(","));
console.log("length=" + single.length);

const empty = new Array(...[]);
console.log("empty=" + empty.length);
