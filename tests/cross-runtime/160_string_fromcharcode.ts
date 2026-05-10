// Cross-runtime: String.fromCharCode
console.log("A=" + String.fromCharCode(65));
console.log("ABC=" + String.fromCharCode(65, 66, 67));
console.log("hello=" + String.fromCharCode(104, 101, 108, 108, 111));

console.log("space=" + String.fromCharCode(32));
console.log("newline=" + String.fromCharCode(10));

// Numbers
const codes = [72, 105];
console.log("Hi=" + String.fromCharCode(...codes));

// Zero
console.log("zero=" + String.fromCharCode(0));
