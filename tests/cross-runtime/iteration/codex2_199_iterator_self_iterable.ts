// Cross-runtime: built-in iterator objects return themselves from Symbol.iterator.
const arrayIt = [1, 2].values();
const mapIt = new Map([["x", 1]]).keys();
const stringIt = "ab"[Symbol.iterator]();
console.log(arrayIt[Symbol.iterator]() === arrayIt);
console.log(mapIt[Symbol.iterator]() === mapIt);
console.log(stringIt[Symbol.iterator]() === stringIt);

