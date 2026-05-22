// Cross-runtime: Number.valueOf
const num = new Number(42);
console.log("valueOf=" + num.valueOf());
console.log("same_type=" + (typeof num.valueOf() === "number"));
console.log("obj_type=" + (typeof num === "object"));

const primitive = 123;
console.log("primitive=" + primitive.valueOf());
console.log("same=" + (primitive.valueOf() === primitive));

const zero = new Number(0);
console.log("zero=" + zero.valueOf());
