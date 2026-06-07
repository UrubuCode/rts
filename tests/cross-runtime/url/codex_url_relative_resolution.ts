// Cross-runtime: URL relative resolution and search param encoding.
const base = new URL("https://example.com/a/b/c?x=1#old");
const u = new URL("../d/./e?space=a b&plus=a+b#frag", base);
console.log(u.href);
console.log(u.pathname);
console.log(u.searchParams.get("space"));
console.log(u.searchParams.get("plus"));
u.searchParams.append("emoji", "🙂");
console.log(u.search);
