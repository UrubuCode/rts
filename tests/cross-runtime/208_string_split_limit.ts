// Cross-runtime: String.split with limit
const str = "a,b,c,d,e";
console.log("no_limit=" + str.split(",").join("|"));
console.log("limit_2=" + str.split(",", 2).join("|"));
console.log("limit_0=" + str.split(",", 0).join("|"));
console.log("limit_10=" + str.split(",", 10).join("|"));

const text = "hello world foo bar";
console.log("space=" + text.split(" ").join("|"));
console.log("space_2=" + text.split(" ", 2).join("|"));

// Empty separator
console.log("chars=" + "abc".split("").join("|"));
console.log("chars_2=" + "abc".split("", 2).join("|"));
