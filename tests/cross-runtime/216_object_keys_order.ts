// Cross-runtime: Object.keys order
const obj = { c: 3, a: 1, b: 2 };
console.log("keys=" + Object.keys(obj).join(","));

const numeric = { 2: "b", 1: "a", 3: "c" };
console.log("numeric=" + Object.keys(numeric).join(","));

const mixed = { b: 2, 1: "one", a: 1, 2: "two" };
console.log("mixed=" + Object.keys(mixed).join(","));

// Symbol keys don't appear
const sym = Symbol("test");
const withSym = { a: 1, [sym]: "hidden", b: 2 };
console.log("withSym=" + Object.keys(withSym).join(","));
