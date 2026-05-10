// Cross-runtime: Array.of
console.log("of_1_2_3=" + Array.of(1, 2, 3).join(","));
console.log("of_single=" + Array.of(5).join(","));
console.log("of_empty=" + Array.of().join(","));
console.log("of_mixed=" + Array.of(1, "a", true).join(","));

// Compare with Array constructor
console.log("constructor_5=" + new Array(5).length);
console.log("of_5=" + Array.of(5).length);

console.log("constructor_1_2=" + new Array(1, 2).join(","));
console.log("of_1_2=" + Array.of(1, 2).join(","));
