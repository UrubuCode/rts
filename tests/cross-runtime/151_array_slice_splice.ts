// Cross-runtime: Array.slice and splice
const arr1 = [1, 2, 3, 4, 5];
console.log("slice_2=" + arr1.slice(2).join(","));
console.log("slice_1_3=" + arr1.slice(1, 3).join(","));
console.log("slice_neg=" + arr1.slice(-2).join(","));
console.log("original=" + arr1.join(","));

const arr2 = [1, 2, 3, 4, 5];
const removed = arr2.splice(2, 2);
console.log("splice_removed=" + removed.join(","));
console.log("splice_arr=" + arr2.join(","));

const arr3 = [1, 2, 5];
arr3.splice(2, 0, 3, 4);
console.log("splice_insert=" + arr3.join(","));

const arr4 = [1, 2, 3, 4, 5];
arr4.splice(1, 2, 8, 9);
console.log("splice_replace=" + arr4.join(","));
