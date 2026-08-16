// Cross-runtime: RegExp.prototype[Symbol.split] does not use the receiver — it
// builds a CLONE through Symbol.species with `y` forced into the flags, and it
// stops the moment the output reaches `limit`, before the next exec. The
// existing subclass fixture only checks that species is consulted; this pins the
// flags the clone is built with, the limit's ToUint32, and the captures.

function j(a: any[]): string {
  return "[" + a.map((x) => (x === undefined ? "u" : JSON.stringify(x))).join(",") + "]";
}

// --- the clone always carries y, and drops nothing else ---
const seen: string[] = [];
class Spy extends RegExp {
  constructor(p: any, f: any) {
    super(p, f);
    seen.push(String(p) + "|" + String(f));
  }
  static get [Symbol.species]() {
    return Spy;
  }
}
seen.length = 0;
console.log("split-gi=" + j("a1b2c".split(new Spy("\\d", "gi") as any)));
console.log("ctor-args=" + seen.join(" ; "));

seen.length = 0;
console.log("split-plain=" + j("a1b".split(new Spy("\\d", "") as any)));
console.log("ctor-plain=" + seen.join(" ; "));

seen.length = 0;
console.log("split-y=" + j("a1b".split(new Spy("\\d", "y") as any)));
console.log("ctor-y=" + seen.join(" ; "));

seen.length = 0;
console.log("split-u=" + j("a1b".split(new Spy("\\d", "u") as any)));
console.log("ctor-u=" + seen.join(" ; "));

// --- the ORIGINAL's lastIndex is never touched, because it is never used ---
const orig = /,/g;
orig.lastIndex = 3;
console.log("orig-result=" + j("a,b,c".split(orig)));
console.log("orig-lastIndex=" + orig.lastIndex);

// --- species may hand back a constructor that is not a RegExp subclass at all ---
class Wrapped extends RegExp {
  static get [Symbol.species]() {
    return RegExp;
  }
}
console.log("species-regexp=" + j("a1b".split(new Wrapped("\\d", "") as any)));

// --- limit: the split stops immediately, and the last exec's captures are cut off ---
console.log("limit-0=" + j("a1b2c".split(/\d/, 0)));
console.log("limit-1=" + j("a1b2c".split(/\d/, 1)));
console.log("limit-2=" + j("a1b2c".split(/\d/, 2)));
console.log("limit-big=" + j("a1b2c".split(/\d/, 99)));
console.log("limit-undef=" + j("a1b2c".split(/\d/, undefined)));

// --- captures count toward the limit, so a limit can land INSIDE a match ---
console.log("cap-limit-1=" + j("a1b".split(/(\d)/, 1)));
console.log("cap-limit-2=" + j("a1b".split(/(\d)/, 2)));
console.log("cap-limit-3=" + j("a1b".split(/(\d)/, 3)));
console.log("cap-two-1=" + j("a1b".split(/(\d)(x)?/, 2)));
console.log("cap-two-3=" + j("a1b".split(/(\d)(x)?/, 3)));

// --- ToUint32 on the limit, exactly as for a string separator ---
console.log("limit-neg-count=" + "a1b2c".split(/\d/, -1).length);
console.log("limit-neg-head=" + "a1b2c".split(/\d/, -1)[0]);
console.log("limit-frac=" + j("a1b2c".split(/\d/, 2.9)));
console.log("limit-str=" + j("a1b2c".split(/\d/, "2" as any)));
console.log("limit-nan=" + j("a1b2c".split(/\d/, NaN as any)));
console.log("limit-null=" + j("a1b2c".split(/\d/, null as any)));
console.log("limit-2p32=" + "a1b2c".split(/\d/, 4294967296 as any).length);

// --- degenerate subjects ---
console.log("empty-subject-nomatch=" + j("".split(/x/)));
console.log("empty-subject-match=" + j("".split(/(?:)/)));
console.log("whole-match=" + j("ab".split(/ab/)));
console.log("edges=" + j(",a,".split(/,/)));
console.log("adjacent=" + j("a,,b".split(/,/)));

// --- a zero-width separator advances one code UNIT, tearing an astral char ---
const torn = "a\u{1F600}b".split(/(?:)/);
console.log("zero-width-len=" + torn.length);
console.log("zero-width-codes=" + torn.map((c) => c.charCodeAt(0).toString(16)).join(","));
const wholeCp = "a\u{1F600}b".split(/(?:)/u);
console.log("zero-width-u-len=" + wholeCp.length);
console.log("zero-width-u-codes=" + wholeCp.map((c) => (c.codePointAt(0) as any).toString(16)).join(","));

// --- calling Symbol.split directly is the same operation ---
console.log("direct=" + j(/,/[Symbol.split]("a,b,c")));
console.log("direct-limit=" + j(/,/[Symbol.split]("a,b,c", 2)));
