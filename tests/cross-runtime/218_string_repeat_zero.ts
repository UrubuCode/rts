// Cross-runtime: String.repeat edge cases
console.log("repeat_0=" + "abc".repeat(0));
console.log("repeat_1=" + "abc".repeat(1));
console.log("empty_5=" + "".repeat(5));
console.log("space_3=" + " ".repeat(3) + "x");

// Large repeat
const large = "x".repeat(100);
console.log("large=" + large.length);

const pattern = "ab".repeat(5);
console.log("pattern=" + pattern);
