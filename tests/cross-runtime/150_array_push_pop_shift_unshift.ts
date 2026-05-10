// Cross-runtime: Array stack/queue operations
const arr = [1, 2, 3];
const len1 = arr.push(4);
console.log("push=" + arr.join(",") + ",len=" + len1);

const popped = arr.pop();
console.log("pop=" + popped + ",arr=" + arr.join(","));

const shifted = arr.shift();
console.log("shift=" + shifted + ",arr=" + arr.join(","));

const len2 = arr.unshift(0);
console.log("unshift=" + arr.join(",") + ",len=" + len2);

// Multiple values
arr.push(5, 6);
console.log("push_multi=" + arr.join(","));

arr.unshift(-2, -1);
console.log("unshift_multi=" + arr.join(","));
