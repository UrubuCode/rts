// Cross-runtime: RegExp.test with global flag
const re = /\d/g;
const str = "a1b2c3";

console.log("test1=" + re.test(str));
console.log("lastIndex1=" + re.lastIndex);

console.log("test2=" + re.test(str));
console.log("lastIndex2=" + re.lastIndex);

console.log("test3=" + re.test(str));
console.log("lastIndex3=" + re.lastIndex);

console.log("test4=" + re.test(str));
console.log("lastIndex4=" + re.lastIndex);
