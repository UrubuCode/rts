// Cross-runtime: global test shares the RegExp lastIndex state.
const re = /x/g;
const s = "x-x";
console.log(re.test(s), re.lastIndex);
console.log(re.test(s), re.lastIndex);
console.log(re.test(s), re.lastIndex);
console.log(re.test(s), re.lastIndex);

