// Cross-runtime: null-prototype objects with JSON and key APIs.
const obj: any = Object.create(null);
obj.b = 2;
obj.a = 1;
obj.toString = "own";

console.log(Object.getPrototypeOf(obj) === null);
console.log(Object.keys(obj).join(","));
console.log(JSON.stringify(obj));
console.log(Object.prototype.toString.call(obj));
