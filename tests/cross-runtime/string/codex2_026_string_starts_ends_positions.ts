// Cross-runtime: startsWith and endsWith apply their position semantics.
const s = "foobarbaz";
console.log([s.startsWith("foo"), s.startsWith("bar", 3), s.startsWith("baz", -1)].join(","));
console.log([s.endsWith("baz"), s.endsWith("bar", 6), s.endsWith("foo", 99)].join(","));

