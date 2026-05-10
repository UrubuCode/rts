// Cross-runtime: Array.forEach always returns undefined
const arr = [1, 2, 3];
const result = arr.forEach(x => x * 2);
console.log("result=" + result);

let sum = 0;
const result2 = arr.forEach(x => { sum += x; });
console.log("result2=" + result2);
console.log("sum=" + sum);

// Return value in callback is ignored
const result3 = arr.forEach(x => { return x * 10; });
console.log("result3=" + result3);
