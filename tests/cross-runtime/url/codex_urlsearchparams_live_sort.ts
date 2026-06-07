// Cross-runtime: URL.searchParams is live and sort updates href.
const u = new URL("https://x.test/path?b=2&a=1&b=1");
const p = u.searchParams;
p.append("c", "hello world");
p.sort();
console.log(u.href);
console.log(p.getAll("b").join(","));
u.search = "?z=9";
console.log(p.toString());
