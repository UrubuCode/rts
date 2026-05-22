// Cross-runtime: Object.keys/Object.values on plain objects.
const obj = { a: 1, b: 2, c: 3 };
console.log("keys=" + Object.keys(obj).join(","));
console.log("values=" + Object.values(obj).join(","));
