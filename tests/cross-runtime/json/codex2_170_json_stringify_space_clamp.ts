// Cross-runtime: numeric indentation is clamped to ten spaces.
const value = { a: { b: 1 } };
console.log(JSON.stringify(value, null, 2));
console.log(JSON.stringify(value, null, 20).split("\n")[1].length);

