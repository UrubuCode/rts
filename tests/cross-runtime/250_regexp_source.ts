// Cross-runtime: RegExp.source property
const re1 = /abc/;
console.log("source1=" + re1.source);

const re2 = /test/gi;
console.log("source2=" + re2.source);

const re3 = new RegExp("hello");
console.log("source3=" + re3.source);

const re4 = new RegExp("\\d+");
console.log("source4=" + re4.source);

const empty = new RegExp("");
console.log("empty=" + empty.source);
