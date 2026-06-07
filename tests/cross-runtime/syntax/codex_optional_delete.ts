// Cross-runtime: delete with optional chaining.
const obj: any = { a: 1 };
const nil: any = null;
console.log(delete obj?.a);
console.log("a" in obj);
console.log(delete nil?.a);

const arr: any[] = [1, 2, 3];
console.log(delete arr?.[1]);
console.log(arr.length + ":" + arr.join(",") + ":" + (1 in arr));
