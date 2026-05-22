// Cross-runtime: Object.prototype.hasOwnProperty
const obj = { a: 1, b: 2 };
console.log("has_a=" + obj.hasOwnProperty("a"));
console.log("has_c=" + obj.hasOwnProperty("c"));

const child = Object.create(obj);
child.d = 4;
console.log("child_a=" + child.hasOwnProperty("a"));
console.log("child_d=" + child.hasOwnProperty("d"));

// Array
const arr = [1, 2, 3];
console.log("arr_0=" + arr.hasOwnProperty(0));
console.log("arr_5=" + arr.hasOwnProperty(5));
console.log("arr_length=" + arr.hasOwnProperty("length"));
