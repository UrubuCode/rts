// Cross-runtime: structuredClone with primitives and nested objects.
console.log(structuredClone(42));
console.log(structuredClone("hello"));
console.log(structuredClone(true));
console.log(structuredClone(null));

const obj = { a: 1, b: { c: 2 } };
const cloned = structuredClone(obj);
cloned.b.c = 99;
console.log(obj.b.c);     // 2 — original unchanged
console.log(cloned.b.c);  // 99

const arr = [1, [2, 3]];
const clonedArr = structuredClone(arr);
clonedArr[1][0] = 99;
console.log(arr[1][0]);     // 2
console.log(clonedArr[1][0]); // 99
