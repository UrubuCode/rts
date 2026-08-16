// Cross-runtime: fromEntries keeps the last duplicate while retaining key order.
const o = Object.fromEntries([["a", 1], ["b", 2], ["a", 3], ["2", "x"], ["1", "y"]]);
console.log(JSON.stringify(o));
console.log(Object.keys(o).join(","));

