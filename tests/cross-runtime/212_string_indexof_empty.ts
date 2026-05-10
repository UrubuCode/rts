// Cross-runtime: String.indexOf with empty string
const str = "hello";
console.log("empty_0=" + str.indexOf(""));
console.log("empty_2=" + str.indexOf("", 2));
console.log("empty_10=" + str.indexOf("", 10));

console.log("lastIndexOf_empty=" + str.lastIndexOf(""));
console.log("lastIndexOf_empty_2=" + str.lastIndexOf("", 2));

const empty = "";
console.log("empty_in_empty=" + empty.indexOf(""));
console.log("x_in_empty=" + empty.indexOf("x"));
