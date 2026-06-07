// Cross-runtime: Array.concat with Symbol.isConcatSpreadable.
const arr: any = [1, 2];
const like: any = { 0: "a", 1: "b", length: 2, [Symbol.isConcatSpreadable]: true };
const blocked: any = [3, 4];
blocked[Symbol.isConcatSpreadable] = false;

const out = arr.concat(like, blocked, "z");
console.log(out.length);
console.log(out.map((x: any) => Array.isArray(x) ? x.join("|") : String(x)).join(","));
console.log(Array.isArray(out[4]));
