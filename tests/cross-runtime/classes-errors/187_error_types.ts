// Cross-runtime: Error types
const err1 = new Error("generic");
console.log("Error=" + err1.name);
console.log("message=" + err1.message);

const err2 = new TypeError("type error");
console.log("TypeError=" + err2.name);
console.log("type_msg=" + err2.message);

const err3 = new RangeError("range error");
console.log("RangeError=" + err3.name);

const err4 = new ReferenceError("ref error");
console.log("ReferenceError=" + err4.name);

const err5 = new SyntaxError("syntax error");
console.log("SyntaxError=" + err5.name);

const err6 = new URIError("uri error");
console.log("URIError=" + err6.name);
