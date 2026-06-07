// Cross-runtime: split includes captures and honors limit.
console.log("a1b22c".split(/(\d+)/).join("|"));
console.log("a1b22c".split(/(\d+)/, 4).join("|"));
console.log("abc".split(/(?:)/, 2).join("|"));
console.log("🙂x".split(/(?:)/u).join("|"));
