// Cross-runtime compatibility: FormData ordering and repeated keys.
const fd = new FormData();
fd.append("a", "1");
fd.append("a", "2");
fd.append("b", "x");

console.log("get=" + fd.get("a"));
console.log("all=" + fd.getAll("a").join(","));
console.log("entries=" + Array.from(fd.entries()).map((p: any) => p[0] + "=" + p[1]).join("|"));
