// Cross-runtime: replaceAll's own rules — it REFUSES a non-global regex with a
// TypeError (the only String method that inspects `flags`), treats a string
// pattern as literal text including regex metacharacters, and still expands the
// dollar tokens. 262_replaceall.ts only covers the happy path.

// --- a non-global regex is refused ---
try {
  console.log("nonglobal=" + "aa".replaceAll(/a/, "-"));
} catch (e: any) {
  console.log("nonglobal!" + e.constructor.name);
}
try {
  console.log("sticky-only=" + "aa".replaceAll(/a/y, "-"));
} catch (e: any) {
  console.log("sticky-only!" + e.constructor.name);
}
try {
  console.log("ignorecase-only=" + "aA".replaceAll(/a/i, "-"));
} catch (e: any) {
  console.log("ignorecase-only!" + e.constructor.name);
}
console.log("global=" + "aa".replaceAll(/a/g, "-"));
console.log("global-sticky=" + "aa".replaceAll(/a/gy, "-"));
console.log("global-i=" + "aA".replaceAll(/a/gi, "-"));

// --- replace (singular) accepts a non-global regex and stops after one ---
console.log("replace-nonglobal=" + "aa".replace(/a/, "-"));

// --- a STRING pattern is literal: metacharacters are not special ---
console.log("dot=" + "a.b.c".replaceAll(".", "-"));
console.log("star=" + "a*b*".replaceAll("*", "+"));
console.log("class=" + "x[a]y[a]".replaceAll("[a]", "Z"));
console.log("backslash-d=" + "a\\db".replaceAll("\\d", "!"));
console.log("pipe=" + "a|b".replaceAll("|", "/"));
console.log("paren=" + "f(x)f(x)".replaceAll("(x)", "()"));

// --- the empty string pattern matches at every position, plus the end ---
console.log("empty-pat=" + "abc".replaceAll("", "-"));
console.log("empty-pat-empty-str=" + "[" + "".replaceAll("", "-") + "]");
console.log("empty-subject=" + "[" + "".replaceAll("x", "-") + "]");

// --- non-overlapping left-to-right scan ---
console.log("overlap=" + "aaaa".replaceAll("aa", "X"));
console.log("overlap3=" + "aaaaa".replaceAll("aa", "X"));

// --- dollar tokens with a STRING pattern ---
console.log("amp=" + "a-b-c".replaceAll("-", "[$&]"));
console.log("prefix=" + "a-b".replaceAll("-", "[$`]"));
console.log("suffix=" + "a-b".replaceAll("-", "[$']"));
console.log("dollar-dollar=" + "a-b".replaceAll("-", "$$"));
console.log("group-literal=" + "a-b".replaceAll("-", "$1"));
console.log("named-literal=" + "a-b".replaceAll("-", "$<n>"));
console.log("lone-dollar=" + "a-b".replaceAll("-", "$"));

// --- dollar tokens with a GLOBAL regex, including named groups ---
console.log("re-group=" + "a1b2".replaceAll(/([a-z])(\d)/g, "$2$1"));
console.log("re-named=" + "a1b2".replaceAll(/(?<l>[a-z])(?<d>\d)/g, "$<d>$<l>"));
console.log("re-amp=" + "a1b2".replaceAll(/\d/g, "<$&>"));
console.log("re-unknown-named=" + "a1".replaceAll(/(?<l>[a-z])/g, "[$<zz>]"));
console.log("re-oob-group=" + "ab".replaceAll(/(a)/g, "$2"));

// --- a nullish or numeric pattern is coerced to a string ---
console.log("num-pat=" + "1a1".replaceAll(1 as any, "-"));
console.log("undef-pat=" + "aundefinedb".replaceAll(undefined as any, "-"));
console.log("null-pat=" + "anullb".replaceAll(null as any, "-"));

// --- the replacement is coerced too ---
console.log("num-repl=" + "a-b".replaceAll("-", 0 as any));
console.log("undef-repl=" + "a-b".replaceAll("-", undefined as any));

// --- a global regex's lastIndex is reset to 0 before and after ---
const re = /a/g;
re.lastIndex = 2;
const out = "aaa".replaceAll(re, "-");
console.log("lastIndex=" + out + ":" + re.lastIndex);
