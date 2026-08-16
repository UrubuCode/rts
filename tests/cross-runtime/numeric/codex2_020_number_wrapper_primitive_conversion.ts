// Cross-runtime: Number wrappers expose primitives under arithmetic and equality.
const boxed = new Number(12.5);
console.log(typeof boxed, typeof boxed.valueOf(), boxed.valueOf());
console.log(boxed == 12.5, boxed === 12.5, boxed + 0);
console.log(Object.prototype.toString.call(boxed));

