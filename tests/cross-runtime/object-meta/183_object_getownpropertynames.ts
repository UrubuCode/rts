// Cross-runtime: Object.getOwnPropertyNames
const obj = { a: 1, b: 2 };
Object.defineProperty(obj, "hidden", {
  value: 3,
  enumerable: false
});

const names = Object.getOwnPropertyNames(obj).sort();
console.log("names=" + names.join(","));

// Array
const arr = [1, 2, 3];
const arrNames = Object.getOwnPropertyNames(arr).sort();
console.log("arr=" + arrNames.join(","));

// Empty
const empty = {};
console.log("empty=" + Object.getOwnPropertyNames(empty).join(","));

// String
const str = new String("hi");
const strNames = Object.getOwnPropertyNames(str).sort();
console.log("str=" + strNames.join(","));
