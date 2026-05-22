// Cross-runtime compatibility: URL and URLSearchParams.
const url = new URL("https://example.com:8080/p/a/t/h?x=1&y=two#frag");
console.log("host=" + url.host);
console.log("path=" + url.pathname);
console.log("hash=" + url.hash);
console.log("x=" + url.searchParams.get("x"));
url.searchParams.set("z", "3");
console.log("query=" + url.searchParams.toString());

const params = new URLSearchParams("a=1&b=2&a=3");
console.log("geta=" + params.get("a"));
console.log("alla=" + params.getAll("a").join(","));
params.append("c", "4");
console.log("final=" + params.toString());
