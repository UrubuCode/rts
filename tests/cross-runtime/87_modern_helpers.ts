// Cross-runtime compatibility: modern static helpers.
console.log("has=" + Object.hasOwn({ a: 1 }, "a"));
console.log("can=" + URL.canParse("/x", "https://example.com"));
