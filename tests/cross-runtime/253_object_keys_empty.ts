// Cross-runtime: Object.keys on various types
const empty = {};
console.log("empty=" + Object.keys(empty).join(","));

const arr = [1, 2, 3];
console.log("arr=" + Object.keys(arr).join(","));

const str = new String("hi");
console.log("str=" + Object.keys(str).join(","));

const num = new Number(42);
console.log("num=" + Object.keys(num).join(","));
