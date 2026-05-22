// Cross-runtime: Array.length setter
const arr = [1, 2, 3, 4, 5];
console.log("initial=" + arr.length);

arr.length = 3;
console.log("truncated=" + arr.join(","));
console.log("new_length=" + arr.length);

arr.length = 5;
console.log("extended=" + arr.length);
console.log("undef=" + (arr[4] === undefined));

arr.length = 0;
console.log("cleared=" + arr.length);
console.log("empty=" + arr.join(","));
