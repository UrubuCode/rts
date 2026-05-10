// Cross-runtime: Array constructor
const arr1 = new Array(5);
console.log("length=" + arr1.length);
console.log("empty=" + (arr1[0] === undefined));

const arr2 = new Array(1, 2, 3);
console.log("values=" + arr2.join(","));

const arr3 = new Array();
console.log("empty_length=" + arr3.length);

// Array function
const arr4 = Array(3);
console.log("func_length=" + arr4.length);

const arr5 = Array(1, 2);
console.log("func_values=" + arr5.join(","));
