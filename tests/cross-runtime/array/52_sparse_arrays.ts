// Cross-runtime compatibility: sparse arrays and holes.
const arr = [1, , 3, , 5];

console.log("len=" + arr.length);
console.log("join=" + arr.join("|"));
console.log("has1=" + (1 in arr));
console.log("has3=" + (3 in arr));
console.log("keys=" + Object.keys(arr).join(","));
console.log("map=" + arr.map((x) => (x === undefined ? "u" : x * 2)).join(","));
console.log("filter=" + arr.filter(() => true).join(","));
console.log("from=" + Array.from(arr, (x) => (x === undefined ? "u" : x)).join(","));
