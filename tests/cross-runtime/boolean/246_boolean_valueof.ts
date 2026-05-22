// Cross-runtime: Boolean.valueOf
const bool = new Boolean(true);
console.log("valueOf=" + bool.valueOf());
console.log("same_type=" + (typeof bool.valueOf() === "boolean"));
console.log("obj_type=" + (typeof bool === "object"));

const boolFalse = new Boolean(false);
console.log("false=" + boolFalse.valueOf());

const primitive = true;
console.log("primitive=" + primitive.valueOf());
console.log("same=" + (primitive.valueOf() === primitive));
