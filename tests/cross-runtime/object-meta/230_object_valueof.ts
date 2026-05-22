// Cross-runtime: Object.prototype.valueOf
const obj = { a: 1 };
console.log("same=" + (obj.valueOf() === obj));

const num = new Number(42);
console.log("num=" + num.valueOf());

const str = new String("hello");
console.log("str=" + str.valueOf());

const bool = new Boolean(true);
console.log("bool=" + bool.valueOf());

const date = new Date(2024, 0, 1);
console.log("date=" + date.valueOf());
