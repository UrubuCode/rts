// Cross-runtime: padStart / padEnd edge cases.
console.log("a".padStart(5));
console.log("a".padStart(5, "0"));
console.log("abc".padStart(2));
console.log("a".padEnd(5));
console.log("a".padEnd(5, "-"));
console.log("abc".padEnd(2));
console.log("".padStart(3, "x"));
console.log("".padEnd(3, "y"));
console.log("ab".padStart(6, "123"));
