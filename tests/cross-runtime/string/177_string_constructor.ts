// Cross-runtime: String constructor
const s1 = new String("hello");
console.log("valueOf=" + s1.valueOf());
console.log("toString=" + s1.toString());
console.log("length=" + s1.length);
console.log("charAt=" + s1.charAt(0));

// String function (not constructor)
console.log("func_5=" + String(5));
console.log("func_true=" + String(true));
console.log("func_null=" + String(null));
console.log("func_undef=" + String(undefined));
console.log("func_obj=" + String({ a: 1 }));
console.log("func_arr=" + String([1, 2, 3]));
