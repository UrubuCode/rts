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

check("plus-concatenates", "a" + 1 === "a1");
check("equal-by-text", "ab" === "a" + "b");
check("typeof", typeof s === "string");

return failed;
