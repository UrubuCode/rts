// Cross-runtime compatibility: File basic behavior.
const file = new File(["abc"], "x.txt", { lastModified: 1700000000000 });
console.log("name=" + file.name);
console.log("lm=" + file.lastModified);
console.log("text=" + (await file.text()));
