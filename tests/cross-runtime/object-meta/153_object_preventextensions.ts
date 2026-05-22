// Cross-runtime: Object.preventExtensions
const obj = { a: 1 };
console.log("extensible=" + Object.isExtensible(obj));

Object.preventExtensions(obj);
console.log("after=" + Object.isExtensible(obj));

obj.a = 10;
console.log("modify=" + obj.a);

const obj2 = { x: 1 };
Object.preventExtensions(obj2);
console.log("sealed=" + Object.isSealed(obj2));
console.log("frozen=" + Object.isFrozen(obj2));
