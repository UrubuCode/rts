// Cross-runtime: returning undefined from a reviver deletes that property.
const value = JSON.parse('{"keep":1,"drop":2,"nested":{"drop":3,"x":4}}', (key, v) => key === "drop" ? undefined : v);
console.log(JSON.stringify(value));
console.log(Object.keys(value).join(","));

