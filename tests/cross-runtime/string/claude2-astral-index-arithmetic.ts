// Cross-runtime: every index-taking String method counts UTF-16 code units, so a
// search for a lone surrogate half FINDS one inside an astral character, and a
// slice at an odd boundary produces an ill-formed string. The corpus pins
// iteration by code point; this pins the arithmetic that iteration hides.

const HI = String.fromCharCode(0xd83d);
const LO = String.fromCharCode(0xde00);
const GRIN = HI + LO; // U+1F600
const S = "a" + GRIN + "b" + GRIN + "c"; // units: a HI LO b HI LO c

function codes(s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(",");
}

// --- the layout the rest of the file is about ---
console.log("len=" + S.length);
console.log("cp-count=" + [...S].length);
console.log("units=" + codes(S));

// --- searching for a HALF succeeds, at the index inside the pair ---
console.log("indexOf-hi=" + S.indexOf(HI));
console.log("indexOf-lo=" + S.indexOf(LO));
console.log("lastIndexOf-hi=" + S.lastIndexOf(HI));
console.log("lastIndexOf-lo=" + S.lastIndexOf(LO));
console.log("includes-hi=" + S.includes(HI));
console.log("indexOf-pair=" + S.indexOf(GRIN));
console.log("lastIndexOf-pair=" + S.lastIndexOf(GRIN));
console.log("indexOf-lo-b=" + S.indexOf(LO + "b"));
console.log("indexOf-b-hi=" + S.indexOf("b" + HI));

// --- fromIndex is a unit index, so it can point INTO a pair ---
console.log("indexOf-from-1=" + S.indexOf(GRIN, 1));
console.log("indexOf-from-2=" + S.indexOf(GRIN, 2));
console.log("indexOf-from-3=" + S.indexOf(GRIN, 3));
console.log("startsWith-at-1=" + S.startsWith(GRIN, 1));
console.log("startsWith-at-2=" + S.startsWith(LO, 2));
console.log("endsWith-at-3=" + S.endsWith(GRIN, 3));
console.log("endsWith-at-2=" + S.endsWith(HI, 2));

// --- search/match report unit indices too ---
console.log("search=" + S.search(new RegExp(GRIN)));
console.log("search-lo=" + S.search(new RegExp(LO)));
console.log("regex-index-u=" + (new RegExp(GRIN, "u").exec(S) as any).index);
console.log("regex-dot-count=" + (S.match(/./g) as any).length);
console.log("regex-dot-u-count=" + (S.match(/./gu) as any).length);

// --- slicing at an odd boundary tears the pair ---
console.log("slice-0-2=" + codes(S.slice(0, 2)) + " wf=" + S.slice(0, 2).isWellFormed());
console.log("slice-2-4=" + codes(S.slice(2, 4)) + " wf=" + S.slice(2, 4).isWellFormed());
console.log("slice-1-3=" + codes(S.slice(1, 3)) + " wf=" + S.slice(1, 3).isWellFormed());
console.log("substring-1-2=" + codes(S.substring(1, 2)));
console.log("substr-2-1=" + codes((S as any).substr(2, 1)));
console.log("slice-neg-1=" + codes(S.slice(-1)));
console.log("slice-neg-2=" + codes(S.slice(-2)));
console.log("slice-neg-3=" + codes(S.slice(-3)));

// --- charAt / at / [] / codePointAt / charCodeAt each answer differently at 1 ---
for (let i = 0; i <= 3; i++) {
  console.log(
    "at" + i +
      " charAt=" + S.charAt(i).charCodeAt(0).toString(16) +
      " bracket=" + (S[i] as any).charCodeAt(0).toString(16) +
      " at=" + ((S.at(i) as any) as string).charCodeAt(0).toString(16) +
      " charCodeAt=" + (S.charCodeAt(i) as any).toString(16) +
      " codePointAt=" + (S.codePointAt(i) as any).toString(16),
  );
}
console.log("codePointAt-lead=" + (S.codePointAt(1) as any).toString(16));
console.log("codePointAt-trail=" + (S.codePointAt(2) as any).toString(16));
console.log("codePointAt-past-end=" + String(S.codePointAt(99)));
console.log("charCodeAt-past-end=" + S.charCodeAt(99));
console.log("charAt-past-end=" + JSON.stringify(S.charAt(99)));
console.log("at-past-end=" + String(S.at(99)));
console.log("at-neg=" + (S.at(-1) as any));

// --- replace of a half rewrites INSIDE the character ---
console.log("replace-hi=" + codes(S.replace(HI, "X")));
console.log("replaceAll-lo=" + codes(S.replaceAll(LO, "Y")));
console.log("replace-hi-wf=" + S.replace(HI, "X").isWellFormed());
console.log("split-on-hi-len=" + S.split(HI).length);
console.log("split-on-hi-wf=" + S.split(HI).map((p) => String(p.isWellFormed())).join(","));

// --- padEnd truncates its pad mid-pair, so padding can break well-formedness ---
const padded = "x".padEnd(4, GRIN);
console.log("pad-codes=" + codes(padded) + " wf=" + padded.isWellFormed());
const paddedEven = "x".padEnd(5, GRIN);
console.log("pad-even=" + codes(paddedEven) + " wf=" + paddedEven.isWellFormed());

// --- concatenation can JOIN two halves into a real character ---
const joined = ("a" + HI) + (LO + "b");
console.log("joined-len=" + joined.length + " cps=" + [...joined].length + " wf=" + joined.isWellFormed());
console.log("joined-cp=" + (joined.codePointAt(1) as any).toString(16));
console.log("halves-alone-wf=" + ("a" + HI).isWellFormed() + "/" + (LO + "b").isWellFormed());

// --- and comparison is by unit, so a torn prefix still compares less ---
console.log("cmp=" + (S.slice(0, 2) < S.slice(0, 3)));
console.log("prefix=" + S.startsWith(S.slice(0, 2)));
