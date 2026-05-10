// Cross-runtime: String.substring and substr
const str = "hello world";
console.log("substring_0_5=" + str.substring(0, 5));
console.log("substring_6=" + str.substring(6));
console.log("substring_6_11=" + str.substring(6, 11));

// Negative indices
console.log("substring_-5=" + str.substring(-5));
console.log("substring_5_2=" + str.substring(5, 2));

// substr (deprecated but still used)
console.log("substr_0_5=" + str.substr(0, 5));
console.log("substr_6=" + str.substr(6));
console.log("substr_6_5=" + str.substr(6, 5));
console.log("substr_-5=" + str.substr(-5));
console.log("substr_-5_2=" + str.substr(-5, 2));
