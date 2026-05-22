// Cross-runtime: String.localeCompare
const a = "a";
const b = "b";
const c = "a";

console.log("a_b=" + (a.localeCompare(b) < 0));
console.log("b_a=" + (b.localeCompare(a) > 0));
console.log("a_a=" + a.localeCompare(c));

const str1 = "apple";
const str2 = "banana";
console.log("apple_banana=" + (str1.localeCompare(str2) < 0));

// Case sensitivity
console.log("A_a=" + "A".localeCompare("a"));
console.log("a_A=" + "a".localeCompare("A"));
