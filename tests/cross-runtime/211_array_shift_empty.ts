// Cross-runtime: Array.shift on empty array
const empty: number[] = [];
const result = empty.shift();
console.log("result=" + result);
console.log("length=" + empty.length);

const arr = [1];
const val = arr.shift();
console.log("val=" + val);
console.log("empty_after=" + arr.length);

// Pop on empty
const empty2: number[] = [];
const popped = empty2.pop();
console.log("popped=" + popped);
