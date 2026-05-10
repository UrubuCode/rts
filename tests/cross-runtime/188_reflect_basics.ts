// Cross-runtime: Reflect basic operations
const obj = { a: 1, b: 2 };

console.log("get=" + Reflect.get(obj, "a"));
console.log("set=" + Reflect.set(obj, "c", 3));
console.log("c=" + obj.c);

console.log("has=" + Reflect.has(obj, "a"));
console.log("has_d=" + Reflect.has(obj, "d"));

console.log("deleteProperty=" + Reflect.deleteProperty(obj, "b"));
console.log("has_b=" + Reflect.has(obj, "b"));

const keys = Reflect.ownKeys(obj);
console.log("keys=" + keys.sort().join(","));
