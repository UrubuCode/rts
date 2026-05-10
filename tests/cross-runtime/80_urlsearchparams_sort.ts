// Cross-runtime compatibility: URLSearchParams sort/encoding.
const usp = new URLSearchParams();
usp.append("x", "1 2");
usp.append("x", "3");
usp.sort();
console.log("query=" + usp.toString());
