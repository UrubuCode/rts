// Cross-runtime: RegExp.lastIndex with exec
const re = /\d+/g;
const str = "a1b2c3";

console.log("lastIndex_init=" + re.lastIndex);

const match1 = re.exec(str);
console.log("match1=" + (match1 ? match1[0] : "null"));
console.log("lastIndex1=" + re.lastIndex);

const match2 = re.exec(str);
console.log("match2=" + (match2 ? match2[0] : "null"));
console.log("lastIndex2=" + re.lastIndex);

const match3 = re.exec(str);
console.log("match3=" + (match3 ? match3[0] : "null"));
console.log("lastIndex3=" + re.lastIndex);

const match4 = re.exec(str);
console.log("match4=" + match4);
console.log("lastIndex4=" + re.lastIndex);
