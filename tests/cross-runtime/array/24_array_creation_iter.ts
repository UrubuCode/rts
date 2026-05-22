// Cross-runtime: Array.from, Array.of, spread and search with fromIndex.
const chars = Array.from("hello");
console.log("from_str=" + chars.join(","));

const mapped = Array.from([1, 2, 3], (n: number) => n * 3);
console.log("from_map=" + mapped.join(","));

const ofs = Array.of(4, 5, 6);
console.log("of=" + ofs.join(","));

const spread = [...ofs, 7];
console.log("spread=" + spread.join(","));

const search = [1, 2, 3, 2, 1, 2];
console.log("indexOf=" + search.indexOf(2, 2));
console.log("lastIndexOf=" + search.lastIndexOf(2, 3));
console.log("includes=" + search.includes(2, 4));
