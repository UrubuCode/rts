// Cross-runtime: a replacer array filters and orders object properties.
const value = { a: 1, b: 2, c: { a: 3, b: 4 } };
console.log(JSON.stringify(value, ["b", "c", "a"]));
console.log(JSON.stringify(value, ["a", "a", 7 as any]));

