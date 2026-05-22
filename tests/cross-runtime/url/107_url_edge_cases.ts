// Cross-runtime compatibility: more URL edge cases.
const u = new URL("https://user:pass@example.com:8443/a/../b/?q=%20#frag");
console.log("origin=" + u.origin);
console.log("user=" + u.username + ":" + u.password);
console.log("path=" + u.pathname);
console.log("query=" + u.searchParams.get("q"));
console.log("href=" + u.href);
