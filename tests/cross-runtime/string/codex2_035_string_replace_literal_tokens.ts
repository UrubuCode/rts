// Cross-runtime: replacement strings expand match, prefix, suffix, and dollar tokens.
console.log("abc".replace("b", "[$&][$`][$'][$$]"));
console.log("aaaa".replace("aa", "<$&>"));

