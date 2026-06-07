// Cross-runtime: replacement string dollar tokens.
const s = "abc123def";
console.log(s.replace(/([a-z]+)(\d+)([a-z]+)/, "$3-$2-$1"));
console.log(s.replace(/(\d+)/, "[$`][$&][$']"));
console.log("x".replace(/(x)/, "$$-$1-$2"));
console.log("abc".replace(/(?<first>a)(b)(c)/, "$<first>-$2"));
