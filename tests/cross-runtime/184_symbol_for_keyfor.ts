// Cross-runtime: Symbol.for and Symbol.keyFor
const s1 = Symbol.for("test");
const s2 = Symbol.for("test");

console.log("same=" + (s1 === s2));
console.log("key=" + Symbol.keyFor(s1));

const s3 = Symbol.for("another");
console.log("key2=" + Symbol.keyFor(s3));

// Regular symbol
const s4 = Symbol("regular");
console.log("key_regular=" + Symbol.keyFor(s4));

// Well-known symbols
console.log("iterator=" + (typeof Symbol.iterator));
console.log("toStringTag=" + (typeof Symbol.toStringTag));
