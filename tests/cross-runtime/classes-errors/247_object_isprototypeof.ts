// Cross-runtime: Object.prototype.isPrototypeOf
const parent = { x: 1 };
const child = Object.create(parent);

console.log("is_proto=" + parent.isPrototypeOf(child));
console.log("not_proto=" + child.isPrototypeOf(parent));

const obj = {};
console.log("object_proto=" + Object.prototype.isPrototypeOf(obj));

const arr = [];
console.log("array_proto=" + Array.prototype.isPrototypeOf(arr));
console.log("object_array=" + Object.prototype.isPrototypeOf(arr));
