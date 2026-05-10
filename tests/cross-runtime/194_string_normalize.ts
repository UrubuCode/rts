// Cross-runtime: String.normalize
const str1 = "café";
const str2 = "café"; // Different composition

console.log("length1=" + str1.length);
console.log("nfc=" + str1.normalize("NFC").length);
console.log("nfd=" + str1.normalize("NFD").length);

// Default is NFC
console.log("default=" + str1.normalize().length);

const simple = "hello";
console.log("simple=" + simple.normalize());
