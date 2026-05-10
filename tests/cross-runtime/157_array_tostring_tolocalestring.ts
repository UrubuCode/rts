// Cross-runtime: Array.toString and toLocaleString
const arr = [1, 2, 3];
console.log("toString=" + arr.toString());

const mixed = [1, "hello", true];
console.log("mixed=" + mixed.toString());

const nested = [1, [2, 3], 4];
console.log("nested=" + nested.toString());

const empty: number[] = [];
console.log("empty=" + empty.toString());

// toLocaleString
const nums = [1, 2, 3];
console.log("locale=" + nums.toLocaleString());
