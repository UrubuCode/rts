// Cross-runtime: Object.create with null
const obj1 = Object.create(null);
obj1.a = 1;
console.log("a=" + obj1.a);
console.log("proto=" + Object.getPrototypeOf(obj1));
console.log("toString=" + (obj1.toString === undefined));

const obj2 = Object.create(Object.prototype);
console.log("proto2=" + (Object.getPrototypeOf(obj2) === Object.prototype));

const parent = { x: 10 };
const child = Object.create(parent);
console.log("child_x=" + child.x);
console.log("own=" + child.hasOwnProperty("x"));

const withProps = Object.create(null, {
  a: { value: 1, enumerable: true }
});
console.log("withProps=" + withProps.a);
