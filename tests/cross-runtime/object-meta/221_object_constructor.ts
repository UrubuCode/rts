// Cross-runtime: Object constructor
const obj1 = new Object();
console.log("empty=" + (Object.keys(obj1).length === 0));

const obj2 = new Object({ a: 1 });
console.log("with_obj=" + obj2.a);

const obj3 = Object({ b: 2 });
console.log("func=" + obj3.b);

// Primitives
const num = new Object(5);
console.log("num=" + (typeof num === "object"));

const str = new Object("hello");
console.log("str=" + (typeof str === "object"));

const bool = new Object(true);
console.log("bool=" + (typeof bool === "object"));
