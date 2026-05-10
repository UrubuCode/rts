// Cross-runtime: Object.seal and freeze
const obj1 = { a: 1, b: 2 };
Object.seal(obj1);
console.log("sealed=" + Object.isSealed(obj1));
console.log("frozen=" + Object.isFrozen(obj1));
obj1.a = 10;
console.log("modified=" + obj1.a);

const obj2 = { x: 1, y: 2 };
Object.freeze(obj2);
console.log("frozen2=" + Object.isFrozen(obj2));
console.log("sealed2=" + Object.isSealed(obj2));
obj2.x = 10;
console.log("not_modified=" + obj2.x);

const obj3 = { p: 1 };
console.log("normal_sealed=" + Object.isSealed(obj3));
console.log("normal_frozen=" + Object.isFrozen(obj3));
