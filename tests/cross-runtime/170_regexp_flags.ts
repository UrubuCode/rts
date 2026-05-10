// Cross-runtime: RegExp flags and properties
const re1 = /abc/gi;
console.log("source=" + re1.source);
console.log("flags=" + re1.flags);
console.log("global=" + re1.global);
console.log("ignoreCase=" + re1.ignoreCase);
console.log("multiline=" + re1.multiline);

const re2 = /test/m;
console.log("multiline2=" + re2.multiline);
console.log("global2=" + re2.global);

const re3 = new RegExp("hello", "gi");
console.log("flags3=" + re3.flags);
console.log("source3=" + re3.source);
