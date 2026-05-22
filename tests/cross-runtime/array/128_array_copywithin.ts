// Cross-runtime: Array.copyWithin
const arr1 = [1, 2, 3, 4, 5];
arr1.copyWithin(0, 3);
console.log("copy1=" + arr1.join(","));

const arr2 = [1, 2, 3, 4, 5];
arr2.copyWithin(1, 3);
console.log("copy2=" + arr2.join(","));

const arr3 = [1, 2, 3, 4, 5];
arr3.copyWithin(0, 2, 4);
console.log("copy3=" + arr3.join(","));

const arr4 = [1, 2, 3, 4, 5];
arr4.copyWithin(-2, -3, -1);
console.log("negative=" + arr4.join(","));
