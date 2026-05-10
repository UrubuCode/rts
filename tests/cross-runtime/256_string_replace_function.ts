// Cross-runtime: String.replace with function
const str = "hello world";
const replaced = str.replace(/o/g, (match) => match.toUpperCase());
console.log("replaced=" + replaced);

const nums = "a1b2c3";
const doubled = nums.replace(/\d/g, (match) => String(Number(match) * 2));
console.log("doubled=" + doubled);

// With capture groups
const text = "John Doe";
const swapped = text.replace(/(\w+) (\w+)/, (match, p1, p2) => p2 + " " + p1);
console.log("swapped=" + swapped);
