// Cross-runtime: Array.fill
const arr1 = [1, 2, 3, 4, 5];
arr1.fill(0);
console.log("all=" + arr1.join(","));

const arr2 = [1, 2, 3, 4, 5];
arr2.fill(9, 2);
console.log("from2=" + arr2.join(","));

const arr3 = [1, 2, 3, 4, 5];
arr3.fill(7, 1, 3);
console.log("range=" + arr3.join(","));

const arr4 = [1, 2, 3, 4, 5];
arr4.fill(8, -2);
console.log("negative=" + arr4.join(","));

const arr5 = new Array(5).fill(3);
console.log("new=" + arr5.join(","));
