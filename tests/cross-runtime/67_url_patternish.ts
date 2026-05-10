// Cross-runtime compatibility: URL resolution and percent encoding.
const url = new URL("../d/e?x=1 2", "https://example.com/a/b/c/");
console.log("href=" + url.href);
console.log("pathname=" + url.pathname);
console.log("search=" + url.search);

const built = new URL("https://example.com");
built.pathname = "/space here";
built.searchParams.set("q", "a+b c");
console.log("built=" + built.toString());
