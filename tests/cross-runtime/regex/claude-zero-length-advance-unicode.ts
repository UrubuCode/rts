// Cross-runtime: how a ZERO-LENGTH match advances. Without /u the cursor moves
// one code UNIT, so an empty match lands between the halves of a surrogate pair;
// with /u it moves a whole code POINT and skips that position. Pins the advance
// for matchAll, replace, split and a hand-rolled exec loop side by side.
// codex_regex_replace_zero_length_global covers only replace, and only ASCII.

const GRIN = "\u{1F600}"; // D83D DE00

function idxs(s: string, re: RegExp): string {
  return [...s.matchAll(re)].map((m: any) => m.index).join(",");
}

// --- ASCII baseline: one empty match per position, plus one at the end ---
console.log("ascii-idx=" + idxs("ab", /(?:)/g));
console.log("ascii-count=" + [..."abc".matchAll(/(?:)/g)].length);
console.log("ascii-empty-subject=" + idxs("", /(?:)/g));

// --- the same regex over an astral character, with and without /u ---
console.log("astral-nou=" + idxs(GRIN + "b", /(?:)/g));
console.log("astral-u=" + idxs(GRIN + "b", /(?:)/gu));
console.log("astral-nou-count=" + [...(GRIN + "b").matchAll(/(?:)/g)].length);
console.log("astral-u-count=" + [...(GRIN + "b").matchAll(/(?:)/gu)].length);
console.log("subject-len=" + (GRIN + "b").length);

// --- replace inserts at every visited position ---
console.log("rep-ascii=" + "ab".replace(/(?:)/g, "-"));
console.log("rep-astral-u=" + (GRIN + "b").replace(/(?:)/gu, "-"));
console.log("rep-astral-nou-len=" + (GRIN + "b").replace(/(?:)/g, "-").length);
console.log("rep-astral-u-len=" + (GRIN + "b").replace(/(?:)/gu, "-").length);

// --- a quantifier that CAN match empty behaves the same way ---
console.log("star-ascii=" + "bac".replace(/a*/g, "-"));
console.log("star-astral-u=" + GRIN.replace(/x*/gu, "-"));
console.log("star-astral-nou-len=" + GRIN.replace(/x*/g, "-").length);
console.log("star-idx=" + idxs("baac", /a*/g));
console.log("star-lens=" + [..."baac".matchAll(/a*/g)].map((m: any) => m[0].length).join(","));

// --- a hand-rolled exec loop must bump lastIndex itself ---
const re = /a*/g;
const steps: string[] = [];
let guard = 0;
let m: any;
while ((m = re.exec("baac")) !== null && guard++ < 8) {
  steps.push(m.index + ":" + m[0].length + ":" + re.lastIndex);
  if (m[0] === "") re.lastIndex++;
}
console.log("exec-loop=" + steps.join(" "));

// --- a sticky zero-length match never advances on its own ---
const y = /a*/y;
y.lastIndex = 0;
const first: any = y.exec("aab");
console.log("sticky1=" + first[0].length + ":" + y.lastIndex);
const second: any = y.exec("aab");
console.log("sticky2=" + second[0].length + ":" + y.lastIndex);

// --- split by an empty regex yields code UNITS, even with /u ---
console.log("split-ascii=" + "abc".split(/(?:)/).join("|"));
console.log("split-astral-nou=" + (GRIN + "b").split(/(?:)/).length);
console.log("split-astral-u=" + (GRIN + "b").split(/(?:)/u).length);
console.log("split-empty-subject=" + JSON.stringify("".split(/(?:)/)));
console.log("split-nomatch-empty=" + JSON.stringify("".split(/x/)));

// --- a lookahead-only pattern is zero length but position dependent ---
console.log("la-idx=" + idxs("aXbX", /(?=X)/g));
console.log("la-replace=" + "aXbX".replace(/(?=X)/g, "-"));
console.log("lb-replace=" + "aXbX".replace(/(?<=X)/g, "-"));
console.log("word-boundary=" + "ab cd".replace(/\b/g, "|"));

// --- a pattern that can match empty OR non-empty prefers the non-empty one ---
console.log("alt=" + "aab".replace(/a?/g, "[$&]"));
console.log("alt-u=" + (GRIN + "a").replace(/a?/gu, "[$&]"));

// --- the /u advance also applies to a lone surrogate, which is one point ---
console.log("lone-lead=" + idxs("\uD83Db", /(?:)/gu));
console.log("lone-trail=" + idxs("\uDE00b", /(?:)/gu));
console.log("pair-split=" + idxs("\uDE00\uD83D", /(?:)/gu));
