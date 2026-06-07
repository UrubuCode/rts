// Cross-runtime: sticky regex lastIndex with unicode code points.
const re = /\u{1F642}/uy;
const s = "a🙂🙂";
re.lastIndex = 1;
console.log(re.test(s) + ":" + re.lastIndex);
console.log(re.test(s) + ":" + re.lastIndex);
console.log(re.test(s) + ":" + re.lastIndex);

const word = /(?<w>\w+)/y;
word.lastIndex = 2;
const m = word.exec("--abc");
console.log(m?.groups?.w + ":" + word.lastIndex);
