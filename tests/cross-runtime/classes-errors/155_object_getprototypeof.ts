// Cross-runtime: Object.getPrototypeOf and setPrototypeOf
const obj = { a: 1 };
const proto = Object.getPrototypeOf(obj);
console.log("is_object_proto=" + (proto === Object.prototype));

const arr = [1, 2, 3];
const arrProto = Object.getPrototypeOf(arr);
console.log("is_array_proto=" + (arrProto === Array.prototype));

const parent = { x: 10 };
const child = Object.create(parent);
console.log("child_proto=" + (Object.getPrototypeOf(child) === parent));
console.log("child_x=" + child.x);

// setPrototypeOf
const obj2 = { a: 1 };
const newProto = { b: 2 };
Object.setPrototypeOf(obj2, newProto);
console.log("new_proto=" + (Object.getPrototypeOf(obj2) === newProto));
console.log("has_b=" + obj2.b);
