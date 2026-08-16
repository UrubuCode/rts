// Cross-runtime: String.prototype.{replace,replaceAll,search,split,match,matchAll}
// dispatch to Symbol.replace/search/split/match/matchAll on ANY object carrying
// them — the argument never reaches string coercion. Pins the exact arguments
// each protocol method receives and how the String method treats the return.
// Existing fixtures only exercise these methods with strings and real regexes.

function show(v: any): string {
  if (v === undefined) return "undefined";
  if (v === null) return "null";
  return String(v);
}

// --- Symbol.replace wins over ToString of the argument ---
const rep: any = { toString() { return "SHOULD-NOT-BE-USED"; } };
rep[Symbol.replace] = function (str: string, repl: any) {
  return "R(" + str + "|" + show(repl) + "|" + typeof repl + ")";
};
console.log("replace=" + "abc".replace(rep, "z"));
console.log("replace-fn=" + "abc".replace(rep, () => "z"));
console.log("replaceAll=" + "abc".replaceAll(rep, "z"));

// --- Symbol.search receives only the string ---
const sea: any = {};
sea[Symbol.search] = function (...args: any[]) {
  return args.length * 100 + args[0].length;
};
console.log("search=" + "abcd".search(sea));

// --- Symbol.split receives (string, limit) with limit passed through raw ---
const spl: any = {};
spl[Symbol.split] = function (str: string, lim: any) {
  return [str, typeof lim, show(lim)];
};
console.log("split=" + "abc".split(spl).join(","));
console.log("split-lim=" + "abc".split(spl, 3).join(","));
console.log("split-lim0=" + "abc".split(spl, 0).join(","));

// --- Symbol.match / Symbol.matchAll ---
const mat: any = {};
mat[Symbol.match] = function (...args: any[]) { return ["M", args[0], "argc" + args.length]; };
console.log("match=" + ("abc".match(mat) as any).join("|"));

const mall: any = {};
mall[Symbol.matchAll] = function (str: string) { return [str, "x", "y"]; };
console.log("matchAll=" + [...("abc".matchAll(mall) as any)].join("|"));

// --- an inherited protocol method is found too ---
const proto: any = {};
proto[Symbol.replace] = function (s: string) { return "INHERITED:" + s; };
const child: any = Object.create(proto);
console.log("inherited=" + "q".replace(child, "-"));

// --- a NULL or undefined Symbol.replace falls back to string coercion ---
const nulled: any = { toString() { return "b"; } };
nulled[Symbol.replace] = null;
console.log("null-protocol=" + "abc".replace(nulled, "-"));
const undefd: any = { toString() { return "c"; } };
undefd[Symbol.replace] = undefined;
console.log("undef-protocol=" + "abc".replace(undefd, "-"));

// --- a non-callable, non-nullish protocol slot is a TypeError ---
const bad: any = {};
bad[Symbol.replace] = 42;
try {
  console.log("bad=" + "abc".replace(bad, "-"));
} catch (e: any) {
  console.log("bad!" + e.constructor.name);
}

// --- replaceAll's global check applies only to REGEXP-LIKE arguments ---
// IsRegExp is "Symbol.match is truthy", so a Symbol.replace-only object skips it.
const regexpLike: any = {};
regexpLike[Symbol.match] = true;
regexpLike[Symbol.replace] = function (s: string) { return "NEVER:" + s; };
try {
  console.log("regexplike=" + "abc".replaceAll(regexpLike, "-"));
} catch (e: any) {
  console.log("regexplike!" + e.constructor.name);
}
regexpLike.flags = "g";
console.log("regexplike-g=" + "abc".replaceAll(regexpLike, "-"));

// --- a nullish searchValue is coerced to a string, not treated as absent ---
console.log("null-search=" + "a null b".replace(null as any, "-"));
console.log("undef-search=" + "a undefined b".replace(undefined as any, "-"));
console.log("num-search=" + "a1b".replace(1 as any, "-"));

// --- Symbol.split is consulted by split even when the object is array-like ---
const arrayish: any = { length: 2, 0: "x", 1: "y" };
arrayish[Symbol.split] = function () { return ["FROM-PROTOCOL"]; };
console.log("arrayish=" + "abc".split(arrayish).join(","));

// --- the protocol is NOT consulted by methods that do not define one ---
const strish: any = { toString() { return "b"; } };
strish[Symbol.replace] = function () { return "NEVER"; };
console.log("indexOf=" + "abc".indexOf(strish));
console.log("includes=" + "abc".includes(strish));
console.log("concat=" + "a".concat(strish));

// --- a real RegExp reaches the SAME protocol methods ---
console.log("re-direct-replace=" + /b/[Symbol.replace]("abc", "X"));
console.log("re-direct-search=" + /c/[Symbol.search]("abc"));
console.log("re-direct-split=" + /,/[Symbol.split]("a,b,c", 2).join("|"));
console.log("re-direct-match=" + (/b/[Symbol.match]("abc") as any)[0]);
console.log("re-has-replace=" + (typeof RegExp.prototype[Symbol.replace]));
console.log("re-has-matchAll=" + (typeof RegExp.prototype[Symbol.matchAll]));
