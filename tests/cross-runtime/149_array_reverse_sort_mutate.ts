// Cross-runtime: Array.reverse and sort (mutating)
const arr1 = [1, 2, 3, 4, 5];
const rev = arr1.reverse();
console.log("reversed=" + rev.join(","));
console.log("mutated=" + arr1.join(","));
console.log("same=" + (rev === arr1));

const arr2 = [3, 1, 4, 1, 5];
const sorted = arr2.sort();
console.log("sorted=" + sorted.join(","));
console.log("mutated2=" + arr2.join(","));

const arr3 = [10, 5, 40, 25, 1000, 1];
arr3.sort((a, b) => a - b);
console.log("numeric=" + arr3.join(","));

const strings = ["banana", "apple", "cherry"];
strings.sort();
console.log("strings=" + strings.join(","));
