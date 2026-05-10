// Cross-runtime: String.raw
const path = String.raw`C:\Users\name\file.txt`;
console.log("path=" + path);

const multiline = String.raw`line1
line2`;
console.log("lines=" + multiline.includes("\n"));

// With substitutions
const name = "test";
const str = String.raw`Hello ${name}!`;
console.log("sub=" + str);

// Compare with regular template
const regular = `C:\Users\name\file.txt`;
console.log("regular=" + regular);
