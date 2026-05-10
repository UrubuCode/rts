// Cross-runtime: Object.assign
const target = { a: 1, b: 2 };
const source = { b: 3, c: 4 };
const result = Object.assign(target, source);

console.log("same=" + (result === target));
console.log("a=" + result.a);
console.log("b=" + result.b);
console.log("c=" + result.c);

// Multiple sources
const obj = Object.assign({}, { x: 1 }, { y: 2 }, { z: 3 });
console.log("multi=" + JSON.stringify(obj));

// With symbols
const sym = Symbol("test");
const withSym = Object.assign({}, { [sym]: "value", normal: "data" });
console.log("normal=" + withSym.normal);
console.log("hasSym=" + (sym in withSym));
