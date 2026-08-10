// `String.prototype`, and the two statics on the constructor.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

let s = "hello";
check("length", s.length === 5);
check("index", s[1] === "e");
check("char-at", s.charAt(0) === "h");
check("char-code-at", s.charCodeAt(0) === 104);
check("code-point-at", s.codePointAt(0) === 104);
check("at-negative", s.at(-1) === "o");
check("index-of", s.indexOf("l") === 2);
check("last-index-of", s.lastIndexOf("l") === 3);
check("includes", s.includes("ell"));
check("starts-with", s.startsWith("he"));
check("ends-with", s.endsWith("lo"));
check("slice", s.slice(1, 3) === "el");
check("slice-negative", s.slice(-2) === "lo");
check("substring", s.substring(1, 3) === "el");
check("substr", s.substr(1, 2) === "el");
check("upper", s.toUpperCase() === "HELLO");
check("lower", "HELLO".toLowerCase() === "hello");
check("repeat", "ab".repeat(2) === "abab");
check("concat", "a".concat("b") === "ab");
check("pad-start", "5".padStart(3, "0") === "005");
check("pad-end", "5".padEnd(3, "0") === "500");
check("to-string", s.toString() === "hello");
check("value-of", s.valueOf() === "hello");

check("trim", "  a  ".trim() === "a");
check("trim-start", "  a  ".trimStart() === "a  ");
check("trim-end", "  a  ".trimEnd() === "  a");

// Code-unit order, because there is no collation table and one dressed as all
// locales is the version that is right only in testing.
check("locale-compare-less", "a".localeCompare("b") < 0);
check("locale-compare-equal", "a".localeCompare("a") === 0);
check("normalize", "a".normalize() === "a");

check("split", "a-b-c".split("-").length === 3);
check("split-empty", "ab".split("").length === 2);
check("replace", "a-b".replace("-", "+") === "a+b");
check("replace-all", "a-b-c".replaceAll("-", "+") === "a+b+c");
check("replace-regex", "a1b2".replace(/[0-9]/g, "") === "ab");
check("search", "abc".search(/b/) === 1);
check("match", "a1b2".match(/[0-9]/g).length === 2);
check("split-regex", "a1b".split(/[0-9]/).length === 2);

check("from-char-code", String.fromCharCode(104, 105) === "hi");
check("from-code-point", String.fromCodePoint(104) === "h");
check("convert", String(12) === "12");
check("convert-bool", String(true) === "true");

// A string is a sequence of UTF-16 code units, and `for-of` walks code POINTS.
// The pair is what makes the difference visible.
let astral = "\u{1F600}a";
check("astral-length", astral.length === 3);
let points = 0;
for (let c of astral) { points = points + 1; }
check("astral-for-of", points === 2);
check("astral-spread", [...astral].length === 2);

// A method added to the prototype is found by every string, which is what
// makes extending a built-in ordinary rather than special-cased.
String.prototype.mine = function () { return "m"; };
check("extensible", "x".mine() === "m");

// `matchAll` keeps the groups every match had, which the global form of
// `match` throws away.
let all = "a1b2".matchAll(/([a-z])(\d)/g);
check("match-all-count", all.length === 2);
check("match-all-group", all[1][1] === "b");
check("match-all-index", all[1].index === 2);
check("match-all-input", all[0].input === "a1b2");
check("match-all-none", "zz".matchAll(/q/g).length === 0);

check("locale-upper", "aß".toLocaleUpperCase() === "ASS");
check("locale-lower", "AB".toLocaleLowerCase() === "ab");

// Built with `fromCharCode` rather than written as an escape: a lone surrogate
// in SOURCE does not survive being read, so a literal would test nothing.
let lone = "a" + String.fromCharCode(0xD800);
check("well-formed", "ab".isWellFormed());
check("well-formed-pair", "\u{1F600}".isWellFormed());
check("well-formed-lone", lone.isWellFormed() === false);
check("to-well-formed-replaces", lone.toWellFormed().isWellFormed());
check("to-well-formed-length", lone.toWellFormed().length === 2);
check("to-well-formed-keeps-pairs", "\u{1F600}".toWellFormed().length === 2);

// `String.raw` reads `raw` off its first argument, and a tagged template is
// what builds one: the escapes reach the tag unresolved.
check("raw", String.raw`a\nb` === "a\\nb");
check("raw-substitution", String.raw`x${1}y` === "x1y");
check("raw-plain-call", String.raw({ raw: ["x", "y"] }, 1) === "x1y");

// A tag receives the PIECES, not the joined string.
function pieces(strings, a, b) { return strings.length; }
check("tag-piece-count", pieces`one${1}two${2}three` === 3);
function cooked(strings) { return strings[0]; }
check("tag-cooked", cooked`a\nb` === "a\nb");
function bothTexts(strings) { return strings[0] + "|" + strings.raw[0]; }
check("tag-cooked-and-raw-differ", bothTexts`a\nb` === "a\nb|a\\nb");
function substitutions(strings, a, b) { return a + b; }
check("tag-substitutions", substitutions`${2}-${3}` === 5);
// A tag written as a member call receives the object, exactly as `o.m()` does.
let holder = { tag: function (strings) { return this.n; }, n: 7 };
check("tag-receiver", holder.tag`x` === 7);

// A SITE has one strings object for the life of the program, which is what
// makes a tagged template usable as a cache key. Same site, twice: identical.
function identity(strings) { return strings; }
function twice() { return identity`same`; }
check("tag-site-identity", twice() === twice());
// Two sites spelling the same template are two objects, which is also the
// specification: a site is a place in the program, not a piece of text.
function elsewhere() { return identity`same`; }
check("tag-sites-are-distinct", twice() !== elsewhere());
// The object survives being written to, because it is the same one.
function marked() { let s = identity`m`; s.seen = (s.seen || 0) + 1; return s.seen; }
check("tag-site-keeps-writes", marked() === 1 && marked() === 2);

check("plus-concatenates", "a" + 1 === "a1");
check("equal-by-text", "ab" === "a" + "b");
check("typeof", typeof s === "string");

return failed;
