// Cross-runtime: the EXACT argument list a replacer function receives, and how
// its return value is used. Shape is (match, p1..pn, offset, string[, groups]) —
// the groups object appears ONLY when the pattern has named captures, which
// shifts nothing else. Existing fixtures call a replacer but never pin its arity.

function show(v: any): string {
  if (v === undefined) return "undefined";
  if (v === null) return "null";
  return String(v);
}

function dump(args: any[]): string {
  return args.length + "[" + args.map(show).join(",") + "]";
}

// --- string pattern: exactly three arguments ---
console.log("str-pat=" + "abc".replace("b", (...a: any[]) => dump(a)));
console.log("str-pat-empty=" + "abc".replace("", (...a: any[]) => dump(a)));

// --- regex with no captures: three arguments ---
console.log("no-cap=" + "abc".replace(/b/, (...a: any[]) => dump(a)));

// --- one numbered capture: four arguments ---
console.log("one-cap=" + "abc".replace(/(b)/, (...a: any[]) => dump(a)));

// --- three captures, one of them not participating -> undefined, not "" ---
console.log("nonpart=" + "abc".replace(/(a)(z)?(b)/, (...a: any[]) => dump(a)));

// --- named captures append a groups OBJECT as the last argument ---
console.log(
  "named=" +
    "abc".replace(/(?<x>b)/, (...a: any[]) => {
      const g = a[a.length - 1];
      return a.length + "|" + show(a[0]) + "|" + show(a[1]) + "|" + show(a[2]) +
        "|" + show(a[3]) + "|" + typeof g + "|" + JSON.stringify(g);
    }),
);

// --- a non-participating NAMED group is undefined inside groups ---
console.log(
  "named-nonpart=" +
    "b".replace(/(?<p>a)|(?<q>b)/, (...a: any[]) => {
      const g = a[a.length - 1];
      return show(g.p) + "/" + show(g.q) + "/" + Object.keys(g).join("+");
    }),
);

// --- the groups object has a NULL prototype ---
console.log(
  "groups-proto=" +
    "b".replace(/(?<q>b)/, (...a: any[]) =>
      show(Object.getPrototypeOf(a[a.length - 1]))),
);

// --- offset and whole-string arguments over a global match ---
const offsets: string[] = [];
"aXbXXc".replace(/X+/g, (m: string, off: number, whole: string) => {
  offsets.push(off + ":" + m.length + ":" + whole.length);
  return "-";
});
console.log("offsets=" + offsets.join(","));

// --- replaceAll with a STRING pattern reports the same offsets ---
const allOffsets: number[] = [];
"aXbXc".replaceAll("X", (...a: any[]) => {
  allOffsets.push(a[a.length - 2]);
  return "-";
});
console.log("all-offsets=" + allOffsets.join(","));

// --- zero-length global matches: one call per position, including the end ---
const zero: number[] = [];
"ab".replace(/(?:)/g, (...a: any[]) => {
  zero.push(a[a.length - 2]);
  return "-";
});
console.log("zero-offsets=" + zero.join(","));

// --- the RETURN value is coerced with ToString, and never re-scanned for $ ---
console.log("ret-num=" + "a".replace(/a/, (() => 42) as any));
console.log("ret-undef=" + "a".replace(/a/, (() => undefined) as any));
console.log("ret-null=" + "a".replace(/a/, (() => null) as any));
console.log("ret-bool=" + "a".replace(/a/, (() => true) as any));
console.log("ret-arr=" + "a".replace(/a/, (() => [1, 2]) as any));
console.log(
  "ret-obj=" + "a".replace(/a/, (() => ({ toString() { return "O"; } })) as any),
);
console.log("ret-dollar-amp=" + "abc".replace(/b/, () => "$&"));
console.log("ret-dollar-1=" + "abc".replace(/(b)/, () => "$1"));
console.log("ret-dollar-dollar=" + "abc".replace(/b/, () => "$$"));

// --- a Symbol return is a TypeError, not "Symbol(x)" ---
try {
  console.log("ret-symbol=" + "a".replace(/a/, (() => Symbol("s")) as any));
} catch (e: any) {
  console.log("ret-symbol!" + e.constructor.name);
}

// --- the replacer is called once per match, in left-to-right order ---
const order: string[] = [];
"a1b2c3".replace(/\d/g, (m: string) => {
  order.push(m);
  return "";
});
console.log("order=" + order.join(""));

// --- a replacer that mutates lastIndex of the regex mid-run ---
const re = /\d/g;
let calls = 0;
const mutated = "1234".replace(re, (m: string) => {
  calls++;
  re.lastIndex = 0;
  return "<" + m + ">";
});
console.log("mutating=" + mutated + ":" + calls + ":" + re.lastIndex);

// --- matchAll and replace agree about the number of matches ---
console.log("matchall-count=" + [..."aXbXXc".matchAll(/X+/g)].length);
