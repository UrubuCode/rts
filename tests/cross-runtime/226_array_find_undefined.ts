// Cross-runtime: Array.find returns undefined when not found
const arr = [1, 2, 3, 4, 5];
const found = arr.find(x => x > 10);
console.log("not_found=" + found);

const found2 = arr.find(x => x === 3);
console.log("found=" + found2);

const idx = arr.findIndex(x => x > 10);
console.log("idx_not_found=" + idx);

const idx2 = arr.findIndex(x => x === 3);
console.log("idx_found=" + idx2);

// Empty array
const empty: number[] = [];
const emptyResult = empty.find(x => true);
console.log("empty=" + emptyResult);
