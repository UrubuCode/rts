// Cross-runtime: String.valueOf
const str = new String("hello");
console.log("valueOf=" + str.valueOf());
console.log("same_type=" + (typeof str.valueOf() === "string"));
console.log("obj_type=" + (typeof str === "object"));

const primitive = "world";
console.log("primitive=" + primitive.valueOf());
console.log("same=" + (primitive.valueOf() === primitive));
