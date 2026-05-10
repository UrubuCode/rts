// Cross-runtime: URL.canParse valid and invalid cases.
console.log("abs=" + URL.canParse("https://example.com/x"));
console.log("rel=" + URL.canParse("/x", "https://example.com"));
console.log("bad=" + URL.canParse("http://[:::1]"));
