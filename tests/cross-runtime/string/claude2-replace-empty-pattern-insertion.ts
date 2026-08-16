// Cross-runtime: an EMPTY search string matches at position 0 and nowhere else
// for `replace`, but replaceAll matches at every code-UNIT boundary — including
// the one inside a surrogate pair, which tears it — and it emits one extra
// insertion after the last character. The $-token table applies to an empty
// match too, where `$&` is "" and $` / $' split the whole string.
// claude-replaceall-* cover the token table on non-empty matches only.

const HI = String.fromCharCode(0xd83d);
const LO = String.fromCharCode(0xde00);
const GRIN = HI + LO;

function codes(s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(",");
}

// --- replace with "" inserts once, at the front ---
console.log("replace-empty=" + JSON.stringify("abc".replace("", "-")));
console.log("replace-empty-on-empty=" + JSON.stringify("".replace("", "-")));
console.log("replace-empty-fn=" + JSON.stringify("abc".replace("", () => "-")));
console.log("replace-empty-index=" + "abc".replace("", function (m, i, s) { return "[" + JSON.stringify(m) + "," + i + "," + s + "]"; }));

// --- replaceAll with "" inserts between every UNIT, and once at each end ---
console.log("replaceAll-empty=" + JSON.stringify("abc".replaceAll("", "-")));
console.log("replaceAll-empty-count=" + "abc".replaceAll("", "-").split("-").length);
console.log("replaceAll-empty-on-empty=" + JSON.stringify("".replaceAll("", "-")));
console.log("replaceAll-empty-one=" + JSON.stringify("a".replaceAll("", "-")));

// --- and "every unit" means it cuts an astral character in half ---
const torn = ("a" + GRIN + "b").replaceAll("", "-");
console.log("astral-torn=" + codes(torn));
console.log("astral-torn-wf=" + torn.isWellFormed());
console.log("astral-torn-len=" + torn.length);
console.log("astral-points=" + [...("a" + GRIN + "b")].length + " insertions=" + (torn.split("-").length - 1));

// --- the regex form advances by a full code point under /u, and by a unit without ---
console.log("regex-empty-g=" + JSON.stringify(("a" + GRIN + "b").replace(/(?:)/g, "-")));
console.log("regex-empty-gu-codes=" + codes(("a" + GRIN + "b").replace(/(?:)/gu, "-")));
console.log("regex-empty-gu-wf=" + ("a" + GRIN + "b").replace(/(?:)/gu, "-").isWellFormed());
console.log("regex-empty-nog=" + JSON.stringify("abc".replace(/(?:)/, "-")));
console.log("regex-count-g=" + (("a" + GRIN + "b").replace(/(?:)/g, "-").split("-").length - 1));
console.log("regex-count-gu=" + (("a" + GRIN + "b").replace(/(?:)/gu, "-").split("-").length - 1));

// --- a regex that CAN match empty mixes zero-length and real matches ---
console.log("star=" + JSON.stringify("aXbXc".replace(/X*/g, "-")));
console.log("star-count=" + ("aXbXc".replace(/X*/g, "-").split("-").length - 1));
console.log("opt=" + JSON.stringify("abc".replace(/x?/g, "-")));
console.log("boundary=" + JSON.stringify("ab cd".replace(/\b/g, "|")));
console.log("lookahead=" + JSON.stringify("abc".replace(/(?=b)/g, "-")));

// --- the $-token table on a zero-length match ---
console.log("amp=" + JSON.stringify("abc".replace("", "[$&]")));
console.log("backtick=" + JSON.stringify("abc".replace("b", "[$`]")));
console.log("quote=" + JSON.stringify("abc".replace("b", "[$']")));
console.log("empty-backtick=" + JSON.stringify("abc".replace("", "[$`]")));
console.log("empty-quote=" + JSON.stringify("abc".replace("", "[$']")));
console.log("dollar-dollar=" + JSON.stringify("abc".replace("", "$$")));
console.log("dollar-1-no-group=" + JSON.stringify("abc".replace("", "$1")));
console.log("dollar-0=" + JSON.stringify("abc".replace("b", "$0")));
console.log("dollar-name-no-groups=" + JSON.stringify("abc".replace("b", "$<x>")));
console.log("dollar-name-regex-no-groups=" + JSON.stringify("abc".replace(/b/, "$<x>")));
console.log("dollar-name-regex-with-groups=" + JSON.stringify("abc".replace(/(?<y>b)/, "[$<x>]")));
console.log("dollar-name-unterminated=" + JSON.stringify("abc".replace(/(?<y>b)/, "[$<y]")));
console.log("dollar-trailing=" + JSON.stringify("abc".replace("b", "end$")));

// --- replaceAll with a non-global regex is the one TypeError here ---
function attempt(f: () => any): string {
  try {
    return JSON.stringify(f());
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}
console.log("replaceAll-nonglobal=" + attempt(() => "aa".replaceAll(/a/ as any, "-")));
console.log("replaceAll-sticky-only=" + attempt(() => "aa".replaceAll(/a/y as any, "-")));
console.log("replaceAll-gy=" + attempt(() => "aa".replaceAll(/a/gy, "-")));
console.log("replaceAll-global=" + attempt(() => "aa".replaceAll(/a/g, "-")));
console.log("replaceAll-string-that-looks-global=" + attempt(() => "a/g/a".replaceAll("/g/", "-")));

// --- the search string is ToString'd, and so is the replacement ---
console.log("num-search=" + JSON.stringify("a1b".replaceAll(1 as any, "-")));
console.log("num-replacement=" + JSON.stringify("aXb".replaceAll("X", 9 as any)));
console.log("obj-search=" + JSON.stringify("aXb".replaceAll({ toString: () => "X" } as any, "-")));
console.log("undefined-search=" + JSON.stringify("aundefinedb".replaceAll(undefined as any, "-")));
console.log("regexp-like-search=" + attempt(() => "a".replaceAll({ [Symbol.match]: false, toString: () => "a" } as any, "-")));

// --- a replacer FUNCTION on an empty match sees "" and the right index ---
const seen: string[] = [];
"ab".replaceAll("", function (m: string, i: number) {
  seen.push(JSON.stringify(m) + "@" + i);
  return "-";
} as any);
console.log("fn-calls=" + seen.join(" "));
const seenU: string[] = [];
("a" + GRIN).replace(/(?:)/gu, function (m: string, i: number) {
  seenU.push(String(i));
  return "";
} as any);
console.log("fn-u-indices=" + seenU.join(","));
