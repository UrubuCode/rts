// Cross-runtime: String.prototype.replaceAll variants.
console.log("str=" + "a-b-c".replaceAll("-", "/"));
console.log("re=" + "foo1foo2".replaceAll(/foo/g, "x"));
console.log("fn=" + "a1b2".replaceAll(/\d/g, (m) => String(Number(m) * 3)));
