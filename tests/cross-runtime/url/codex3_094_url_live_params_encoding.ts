// Cross-runtime: URL and URLSearchParams stay live across encoded mutations and sorting.
const url = new URL("https://user:pass@example.com/a%20b?z=3&a=hello+world&a=%2B#frag");
console.log(url.searchParams.getAll("a").join("|"));
url.searchParams.append("é", "x y+z");
url.searchParams.sort();
url.searchParams.set("a", "last value");
console.log(url.search);
console.log(url.href);

