// Cross-runtime: JSON.stringify with space
const obj = { a: 1, b: { c: 2 } };

const compact = JSON.stringify(obj);
console.log("compact=" + compact);

const spaced = JSON.stringify(obj, null, 2);
const lines = spaced.split("\n").length;
console.log("lines=" + lines);
console.log("has_indent=" + spaced.includes("  "));

const tabbed = JSON.stringify(obj, null, "\t");
console.log("has_tab=" + tabbed.includes("\t"));

// Array
const arr = [1, 2, 3];
const arrStr = JSON.stringify(arr, null, 2);
console.log("arr_lines=" + arrStr.split("\n").length);
