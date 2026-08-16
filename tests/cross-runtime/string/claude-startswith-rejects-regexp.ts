// Cross-runtime: startsWith / endsWith / includes throw a TypeError on a RegExp
// argument — and the test is IsRegExp, i.e. "Symbol.match is truthy", not
// "instanceof RegExp". So a regex with Symbol.match set to false is accepted and
// coerced with toString, while a plain object with Symbol.match true is refused.
// 129/37/236 only pass plain strings.

// --- a real regex is refused by all three ---
try {
  console.log("sw-re=" + "abc".startsWith(/a/ as any));
} catch (e: any) {
  console.log("sw-re!" + e.constructor.name);
}
try {
  console.log("ew-re=" + "abc".endsWith(/c/ as any));
} catch (e: any) {
  console.log("ew-re!" + e.constructor.name);
}
try {
  console.log("inc-re=" + "abc".includes(/b/ as any));
} catch (e: any) {
  console.log("inc-re!" + e.constructor.name);
}

// --- but indexOf / lastIndexOf / split / replace accept one happily ---
console.log("indexOf-re=" + "a/b/c".indexOf(/b/ as any));
console.log("lastIndexOf-re=" + "x/b/".lastIndexOf(/b/ as any));
console.log("split-re=" + "a1b".split(/\d/).join("|"));
console.log("replace-re=" + "abc".replace(/b/, "-"));

// --- a regex whose Symbol.match is FALSE is no longer "a regexp" ---
const tame: any = /b/;
tame[Symbol.match] = false;
console.log("tame-source=" + tame.source);
console.log("tame-tostring=" + String(tame));
console.log("sw-tame=" + "/b/xyz".startsWith(tame));
console.log("inc-tame=" + "xx/b/yy".includes(tame));
console.log("ew-tame=" + "xx/b/".endsWith(tame));
console.log("inc-tame-miss=" + "abc".includes(tame));

// --- and String.match / replace still route through the regexp path for it ---
console.log("tame-replace=" + "abc".replace(tame, "-"));

// --- a plain object with a truthy Symbol.match IS "a regexp" and is refused ---
const impostor: any = { toString() { return "ab"; } };
impostor[Symbol.match] = true;
try {
  console.log("sw-impostor=" + "abc".startsWith(impostor));
} catch (e: any) {
  console.log("sw-impostor!" + e.constructor.name);
}
try {
  console.log("inc-impostor=" + "abc".includes(impostor));
} catch (e: any) {
  console.log("inc-impostor!" + e.constructor.name);
}

// --- a FALSY Symbol.match on a plain object leaves it an ordinary value ---
const meek: any = { toString() { return "ab"; } };
meek[Symbol.match] = 0;
console.log("sw-meek=" + "abc".startsWith(meek));
const absent: any = { toString() { return "bc"; } };
console.log("ew-absent=" + "abc".endsWith(absent));
console.log("inc-absent=" + "abc".includes(absent));

// --- a Symbol.match getter is consulted, and only once per call ---
let reads = 0;
const probed: any = { toString() { return "a"; } };
Object.defineProperty(probed, Symbol.match, {
  get() { reads++; return undefined; },
});
console.log("probed=" + "abc".startsWith(probed) + ":" + reads);

// --- other argument types coerce normally ---
console.log("num=" + "12ab".startsWith(1 as any));
console.log("undef=" + "undefinedx".startsWith(undefined as any));
console.log("null=" + "xnull".endsWith(null as any));
console.log("empty=" + "abc".startsWith(""));
console.log("empty-end=" + "abc".endsWith(""));
console.log("empty-inc=" + "abc".includes(""));

// --- position arguments, for the record ---
console.log("sw-pos=" + "abcabc".startsWith("abc", 3));
console.log("ew-pos=" + "abcabc".endsWith("abc", 3));
console.log("inc-pos=" + "abcabc".includes("abc", 4));
console.log("ew-pos-nan=" + "abc".endsWith("c", NaN));
console.log("sw-pos-neg=" + "abc".startsWith("a", -5));
console.log("inc-pos-big=" + "abc".includes("", 99));
