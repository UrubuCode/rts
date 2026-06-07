// Cross-runtime: Reflect.setPrototypeOf rejects prototype cycles.
const a: any = {};
const b: any = {};
console.log(Reflect.setPrototypeOf(a, b));
console.log(Object.getPrototypeOf(a) === b);
console.log(Reflect.setPrototypeOf(b, a));
console.log(Object.getPrototypeOf(b) === a);
