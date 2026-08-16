// Cross-runtime: direct global exec of an empty match leaves lastIndex unchanged.
const re = /(?:)/g;
re.lastIndex = 1;
const a = re.exec("abc")!;
const afterA = re.lastIndex;
const b = re.exec("abc")!;
console.log(a.index, afterA, b.index, re.lastIndex);
re.lastIndex = 4;
console.log(re.exec("abc"), re.lastIndex);

