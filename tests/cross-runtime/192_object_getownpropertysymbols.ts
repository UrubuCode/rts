// Cross-runtime: Object.getOwnPropertySymbols
const sym1 = Symbol("a");
const sym2 = Symbol("b");

const obj = {
  normal: 1,
  [sym1]: "value1",
  [sym2]: "value2"
};

const symbols = Object.getOwnPropertySymbols(obj);
console.log("count=" + symbols.length);
console.log("has_sym1=" + symbols.includes(sym1));
console.log("has_sym2=" + symbols.includes(sym2));

// Regular keys don't include symbols
const keys = Object.keys(obj);
console.log("keys=" + keys.join(","));

// getOwnPropertyNames also doesn't include symbols
const names = Object.getOwnPropertyNames(obj);
console.log("names=" + names.join(","));
