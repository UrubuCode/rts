// Cross-runtime: Symbol.isConcatSpreadable overrides default concat flattening.
const array: any = [1, 2];
array[Symbol.isConcatSpreadable] = false;
const arrayLike: any = { 0: "a", 1: "b", length: 2, [Symbol.isConcatSpreadable]: true };
const out = [0].concat(array, arrayLike, 3);
console.log(out.length);
console.log(out[0], Array.isArray(out[1]), out[2], out[3], out[4]);

