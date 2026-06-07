// Cross-runtime: URL keeps invalid percent escapes in path normalization.
const u = new URL("https://x.test/a/%zz/%E0%A4%A?x=%zz");
console.log(u.pathname);
console.log(u.search);
console.log(u.href);
