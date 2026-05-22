// Cross-runtime: Boolean constructor and valueOf
const b1 = new Boolean(true);
const b2 = new Boolean(false);

console.log("valueOf_true=" + b1.valueOf());
console.log("valueOf_false=" + b2.valueOf());
console.log("toString_true=" + b1.toString());
console.log("toString_false=" + b2.toString());

// Truthy/falsy
console.log("bool_1=" + Boolean(1));
console.log("bool_0=" + Boolean(0));
console.log("bool_str=" + Boolean("hello"));
console.log("bool_empty=" + Boolean(""));
console.log("bool_null=" + Boolean(null));
console.log("bool_undef=" + Boolean(undefined));
