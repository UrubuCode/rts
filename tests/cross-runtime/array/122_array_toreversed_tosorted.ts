// Cross-runtime: Array.toReversed, toSorted, toSpliced
const arr = [3, 1, 4, 1, 5];

const reversed = arr.toReversed();
console.log("reversed=" + reversed.join(","));
console.log("original=" + arr.join(","));

const sorted = arr.toSorted();
console.log("sorted=" + sorted.join(","));
console.log("original2=" + arr.join(","));

const sortedDesc = arr.toSorted((a, b) => b - a);
console.log("sorteddesc=" + sortedDesc.join(","));

const spliced = arr.toSpliced(1, 2, 9, 9);
console.log("spliced=" + spliced.join(","));
console.log("original3=" + arr.join(","));
