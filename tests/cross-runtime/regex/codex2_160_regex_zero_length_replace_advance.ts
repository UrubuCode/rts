// Cross-runtime: global zero-length replacement advances across the input.
console.log("abc".replace(/(?:)/g, "-"));
console.log("ab".replace(/^|$/g, "|"));
console.log("aaa".replace(/(?=a)/g, "X"));

