// Cross-runtime: RegExp(pattern) where pattern is already a RegExp. With `new`
// it always builds a fresh object copying source and flags; WITHOUT `new` and
// with no flags argument it returns the SAME object, but supplying flags forces
// a copy. lastIndex is never carried over. Nothing in the corpus rebuilds a regex.

const src = /ab+c/gi;
src.lastIndex = 3;

// --- new RegExp(re) copies pattern and flags, and is a different object ---
const copy = new RegExp(src);
console.log("copy-source=" + copy.source);
console.log("copy-flags=" + copy.flags);
console.log("copy-identity=" + (copy === src));
console.log("copy-lastIndex=" + copy.lastIndex);
console.log("src-lastIndex=" + src.lastIndex);

// --- a flags argument REPLACES the whole flag set, it does not merge ---
const over = new RegExp(src, "m");
console.log("over-source=" + over.source);
console.log("over-flags=" + over.flags);
console.log("over-global=" + over.global);
console.log("over-empty=" + "[" + new RegExp(src, "").flags + "]");
console.log("over-add=" + new RegExp(/a/, "gy").flags);

// --- undefined flags means "keep the original's" ---
console.log("undef-flags=" + new RegExp(src, undefined).flags);

// --- without new: same object when flags are absent ---
const same = RegExp(src);
console.log("call-identity=" + (same === src));
console.log("call-lastIndex=" + same.lastIndex);
const different = RegExp(src, "i");
console.log("call-flags-identity=" + (different === src));
console.log("call-flags=" + different.flags);
console.log("call-string=" + (RegExp("a") instanceof RegExp));

// --- the "same object" shortcut only applies when the constructor matches ---
class Sub extends RegExp {}
const subInstance = new Sub("a", "g");
console.log("sub-call-identity=" + (RegExp(subInstance) === subInstance));
console.log("sub-new-ctor=" + (new RegExp(subInstance)).constructor.name);
console.log("sub-flags=" + new RegExp(subInstance, "i").flags);

// --- a plain object is NOT a regex: source/flags are ignored, ToString wins ---
const impostor: any = { source: "zz", flags: "g", toString() { return "q+"; } };
console.log("impostor-source=" + new RegExp(impostor).source);
console.log("impostor-flags=" + "[" + new RegExp(impostor).flags + "]");
console.log("impostor-match=" + new RegExp(impostor).test("qqq"));

// --- unless it declares Symbol.match, which makes IsRegExp true ---
const matchy: any = { source: "z+", flags: "i", toString() { return "NEVER"; } };
matchy[Symbol.match] = true;
console.log("matchy-source=" + new RegExp(matchy).source);
console.log("matchy-flags=" + new RegExp(matchy).flags);
console.log("matchy-override=" + new RegExp(matchy, "g").flags);
console.log("matchy-call-identity=" + (RegExp(matchy) === matchy));

// --- missing pattern and nullish pattern ---
console.log("noarg=" + new RegExp(undefined).source + "|" + "[" + new RegExp(undefined).flags + "]");
console.log("noargs=" + new (RegExp as any)().source);
console.log("null=" + new RegExp(null as any).source);
console.log("null-matches=" + new RegExp(null as any).test("null"));
console.log("number=" + new RegExp(12 as any).source);
console.log("empty-string=" + new RegExp("").source);

// --- flags are coerced with ToString ---
console.log("flags-obj=" + new RegExp("a", { toString() { return "gi"; } } as any).flags);
try {
  console.log("flags-num=" + new RegExp("a", 1 as any).flags);
} catch (e: any) {
  console.log("flags-num!" + e.constructor.name);
}
try {
  console.log("flags-symbol=" + new RegExp("a", Symbol("g") as any).flags);
} catch (e: any) {
  console.log("flags-symbol!" + e.constructor.name);
}

// --- the copy is functionally independent of the original ---
const orig = /a/g;
const clone = new RegExp(orig);
orig.lastIndex = 1;
console.log("indep=" + clone.test("aa") + ":" + clone.lastIndex + ":" + orig.lastIndex);
console.log("indep2=" + orig.test("aa") + ":" + orig.lastIndex);

// --- a source string with a slash survives the round trip ---
const slashed = /a\/b/;
console.log("slash-roundtrip=" + new RegExp(slashed).source);
console.log("slash-test=" + new RegExp(slashed).test("a/b"));
console.log("empty-roundtrip=" + new RegExp(new RegExp("")).source);
