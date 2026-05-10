// Cross-runtime: Array.map with sparse arrays
const sparse = [1, , 3];
const mapped = sparse.map(x => x * 2);
console.log("length=" + mapped.length);
console.log("0=" + mapped[0]);
console.log("1=" + mapped[1]);
console.log("2=" + mapped[2]);

// Filter removes holes
const filtered = sparse.filter(x => true);
console.log("filtered=" + filtered.join(","));

// forEach skips holes
let count = 0;
sparse.forEach(() => count++);
console.log("count=" + count);
