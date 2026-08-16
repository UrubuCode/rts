// Cross-runtime: negative lookahead excludes forbidden suffixes.
const words = "cat car can dog".match(/ca(?!t)\w/g);
console.log(JSON.stringify(words));
console.log(/foo(?!bar)/.test("foobaz"), /foo(?!bar)/.test("foobar"));

