// Cross-runtime: null-prototype objects have own keys without inherited methods.
const dict = Object.create(null);
dict.alpha = 1;
dict.toString = 2;
console.log(Object.getPrototypeOf(dict) === null);
console.log(Object.keys(dict).join(","), typeof dict.hasOwnProperty);
console.log(Object.hasOwn(dict, "toString"));

