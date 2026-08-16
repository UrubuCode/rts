// Cross-runtime: String.raw interleaves raw segments with coerced substitutions.
const value = { toString() { return "OBJ"; } };
console.log(String.raw({ raw: ["a\\n", "b\\t", "c"] }, 7, value));
console.log(String.raw({ raw: ["only"] }, "ignored"));

