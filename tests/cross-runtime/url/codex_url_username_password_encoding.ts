// Cross-runtime: URL username/password percent encoding.
const u = new URL("https://example.com/");
u.username = "a b";
u.password = "p@ss:word";
console.log(u.href);
console.log(u.username);
console.log(u.password);
