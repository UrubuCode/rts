// Cross-runtime: Object.propertyIsEnumerable
const obj = { a: 1, b: 2 };
console.log("a=" + obj.propertyIsEnumerable("a"));
console.log("b=" + obj.propertyIsEnumerable("b"));
console.log("c=" + obj.propertyIsEnumerable("c"));

Object.defineProperty(obj, "hidden", {
  value: 3,
  enumerable: false
});

console.log("hidden=" + obj.propertyIsEnumerable("hidden"));

// Inherited
const child = Object.create(obj);
console.log("inherited=" + child.propertyIsEnumerable("a"));

// Array
const arr = [1, 2, 3];
console.log("arr_0=" + arr.propertyIsEnumerable(0));
console.log("arr_length=" + arr.propertyIsEnumerable("length"));
