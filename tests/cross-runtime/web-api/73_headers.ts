// Cross-runtime compatibility: Headers normalization and append semantics.
const h = new Headers([
  ["x-a", "1"],
  ["x-b", "2"],
]);
h.append("x-a", "3");

console.log("get=" + String(h.get("x-a")));
console.log("entries=" + Array.from(h.entries()).map((p: any) => p.join("=")).join("|"));
