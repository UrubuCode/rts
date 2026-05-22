// Cross-runtime: String.split with regex
const str = "a1b2c3";
console.log("digits=" + str.split(/\d/).join("|"));

const text = "hello  world   foo";
console.log("spaces=" + text.split(/\s+/).join("|"));

const csv = "a,b;c,d";
console.log("delims=" + csv.split(/[,;]/).join("|"));

// With limit
console.log("limit=" + str.split(/\d/, 2).join("|"));
