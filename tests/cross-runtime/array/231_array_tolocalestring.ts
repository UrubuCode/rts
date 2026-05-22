// Cross-runtime: Array.toLocaleString basic
const arr = [1, 2, 3];
const str = arr.toLocaleString();
console.log("has_commas=" + str.includes(","));

const mixed = [1, "hello", true];
const mixedStr = mixed.toLocaleString();
console.log("mixed=" + mixedStr.includes("hello"));

const empty: number[] = [];
console.log("empty=" + empty.toLocaleString());
