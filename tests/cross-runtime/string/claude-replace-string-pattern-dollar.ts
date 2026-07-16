// Cross-runtime: String.prototype.replace with a STRING pattern (not regex).
// A string pattern replaces ONLY the first occurrence, and dollar tokens in the
// replacement still apply: $& (match), $` (prefix), $' (suffix), $$ (literal $).
// Numbered groups like $1 have NO meaning for a string pattern (stay literal).

// --- only the first occurrence is replaced ---
console.log("first=" + "aXbXcX".replace("X", "-"));
console.log("first-word=" + "cat cat cat".replace("cat", "dog"));
console.log("no-match=" + "abc".replace("z", "!"));
console.log("empty-pattern=" + "abc".replace("", "-"));
console.log("empty-repl=" + "aXbX".replace("X", ""));
console.log("whole=" + "abc".replace("abc", "Z"));

// --- $& = the matched substring ---
console.log("amp=" + "hello".replace("ll", "[$&]"));
console.log("amp-twice=" + "hello".replace("ll", "$&$&"));
console.log("amp-empty-pat=" + "abc".replace("", "[$&]"));

// --- $` = portion before the match ---
console.log("prefix=" + "abcdef".replace("cd", "[$`]"));
console.log("prefix-at-start=" + "abcdef".replace("ab", "[$`]"));

// --- $' = portion after the match ---
console.log("suffix=" + "abcdef".replace("cd", "[$']"));
console.log("suffix-at-end=" + "abcdef".replace("ef", "[$']"));

// --- $$ = a literal dollar sign ---
console.log("dollar=" + "price".replace("price", "$$"));
console.log("dollar-amount=" + "cost".replace("cost", "$$100"));
console.log("dollar-then-amp=" + "xy".replace("x", "$$$&"));

// --- $1 has no meaning with a string pattern: stays literal ---
console.log("group1-literal=" + "abc".replace("b", "$1"));
console.log("group0-literal=" + "abc".replace("b", "$0"));
console.log("named-literal=" + "abc".replace("b", "$<n>"));

// --- unknown / dangling dollar sequences stay literal ---
console.log("lone-dollar=" + "abc".replace("b", "$"));
console.log("dollar-x=" + "abc".replace("b", "$x"));
console.log("trailing-dollar=" + "abc".replace("b", "z$"));

// --- combinations, in one replacement ---
console.log("combo=" + "abcdef".replace("cd", "<$`|$&|$'>"));
console.log("combo2=" + "12345".replace("3", "$`$&$'"));

// --- replacement is coerced to a string ---
console.log("num-repl=" + "abc".replace("b", String(42)));

// --- pattern is coerced to a string too ---
console.log("num-pattern=" + "a1b1".replace(String(1), "-"));

// --- function replacer with a string pattern gets (match, offset, whole) ---
console.log("fn=" + "abcabc".replace("bc", (m, off, whole) => `${m}@${off}/${whole.length}`));
console.log("fn-empty=" + "abc".replace("", (m, off) => `[${m.length}@${off}]`));
// dollar tokens returned BY a function are NOT expanded
console.log("fn-no-expand=" + "abc".replace("b", () => "$&"));
