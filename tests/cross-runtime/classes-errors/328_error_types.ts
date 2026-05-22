// Cross-runtime: error constructors and instanceof.
const e1 = new Error("base");
const e2 = new TypeError("type");
const e3 = new RangeError("range");
const e4 = new ReferenceError("ref");
const e5 = new SyntaxError("syntax");
const e6 = new URIError("uri");

console.log(e1.message);
console.log(e1.name);
console.log(e2.name);
console.log(e3.name);
console.log(e4.name);
console.log(e5.name);
console.log(e6.name);

console.log(e2 instanceof Error);
console.log(e2 instanceof TypeError);
console.log(e3 instanceof RangeError);
console.log(e3 instanceof TypeError);

// Error cause (ES2022)
const withCause = new Error("outer", { cause: new Error("inner") });
console.log(withCause.message);
console.log((withCause.cause as Error).message);
