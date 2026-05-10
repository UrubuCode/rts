// Cross-runtime: Array.unshift return value
const arr = [3, 4];
const len1 = arr.unshift(2);
console.log("len1=" + len1);
console.log("arr1=" + arr.join(","));

const len2 = arr.unshift(0, 1);
console.log("len2=" + len2);
console.log("arr2=" + arr.join(","));

const empty: number[] = [];
const len3 = empty.unshift(1);
console.log("len3=" + len3);
console.log("arr3=" + empty.join(","));
