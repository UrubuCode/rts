// Cross-runtime: JavaScript regexes are BACKTRACKING, not leftmost-longest. The
// first alternative that lets the WHOLE pattern succeed wins even when a later
// one would match more, and a greedy quantifier gives characters back one at a
// time rather than choosing the best split. Nothing in the corpus pins the
// ordering rule itself — only greedy-vs-lazy on a single quantifier.

function m(re: RegExp, s: string): string {
  const r = re.exec(s);
  return r === null ? "null" : JSON.stringify(Array.prototype.slice.call(r)) + "@" + r.index;
}

// --- alternation takes the FIRST alternative that works, not the longest ---
console.log("first-wins=" + m(/ab|abcd/, "abcd"));
console.log("longest-if-first=" + m(/abcd|ab/, "abcd"));
console.log("first-fails-then=" + m(/abx|abcd/, "abcd"));
console.log("anchored-forces-long=" + m(/^(?:ab|abcd)$/, "abcd"));
console.log("empty-alt-first=" + m(/|a/, "a"));
console.log("empty-alt-last=" + m(/a|/, "a"));
console.log("nested-alt=" + m(/(a|ab)(c|bcd)/, "abcd"));
console.log("nested-alt-swap=" + m(/(ab|a)(bcd|c)/, "abcd"));

// --- the match is also LEFTMOST: an earlier short match beats a later long one ---
console.log("leftmost=" + m(/a+/, "ba aaa"));
console.log("leftmost-alt=" + m(/x|aaa/, "aaax"));

// --- greedy gives back one character at a time; lazy takes one at a time ---
console.log("greedy=" + m(/^(a+)(a*)$/, "aaa"));
console.log("lazy=" + m(/^(a+?)(a*)$/, "aaa"));
console.log("greedy-backoff=" + m(/^(a+)a$/, "aaa"));
console.log("greedy-dot=" + m(/"(.*)"/, 'say "a" and "b" now'));
console.log("lazy-dot=" + m(/"(.*?)"/, 'say "a" and "b" now'));
console.log("greedy-then-fixed=" + m(/^(.*)(\d)$/, "a1b2c3"));
console.log("lazy-then-fixed=" + m(/^(.*?)(\d)/, "a1b2c3"));

// --- nested quantifiers: the inner one is satisfied before the outer repeats ---
console.log("nested-quant=" + m(/^(?:(a*))*$/, "aaa"));
console.log("nested-groups=" + m(/^((a)(b)?)+$/, "aab"));
console.log("nested-star-plus=" + m(/^(a+)+$/, "aaa"));
console.log("nested-alt-quant=" + m(/^(?:a|aa)+$/, "aaaa"));

// --- a quantified group keeps only the LAST iteration's capture ---
console.log("last-iteration=" + m(/^(?:(\w)-)+$/, "a-b-c-"));
console.log("last-iteration-two=" + m(/^(?:(\w)(\d))+$/, "a1b2"));
console.log("zero-iterations=" + m(/^(?:(x))*$/, ""));

// --- an OPTIONAL group that matched then backtracked out is undefined, not "" ---
console.log("backtracked-out=" + m(/^(a)?a$/, "a"));
console.log("kept-in=" + m(/^(a)?a$/, "aa"));
console.log("empty-vs-undefined=" + m(/^(a*)a?$/, "a"));

// --- a zero-width group inside * must not loop forever: it stops after one ---
console.log("empty-loop=" + m(/^(?:)*$/, ""));
console.log("empty-loop-group=" + m(/^(a?)*$/, "aa"));
console.log("empty-loop-b=" + m(/(?:a*)*b/, "aaab"));

// --- catastrophic shapes, kept small enough to finish instantly ---
console.log("small-nested=" + m(/^(?:a+)+$/, "aaaaaaaa"));
console.log("small-nested-fail=" + m(/^(?:a+)+b$/, "aaaaaaaa"));

// --- global iteration takes successive first-matches, never a best cover ---
console.log("global-first=" + JSON.stringify("abcd".match(/ab|abcd/g)));
console.log("global-long=" + JSON.stringify("abcdabcd".match(/abcd|ab/g)));
console.log("global-overlap=" + JSON.stringify("aaaa".match(/aa/g)));

// --- replace inherits exactly the same choice ---
console.log("replace-first=" + "abcd".replace(/ab|abcd/, "<$&>"));
console.log("replace-long=" + "abcd".replace(/abcd|ab/, "<$&>"));

// --- lookahead can force the longest alternative without consuming ---
console.log("lookahead-force=" + m(/(?=(abcd))(?:ab|abcd)/, "abcd"));
console.log("possessive-emulation=" + m(/(?=(a+))\1b/, "aaab"));
console.log("possessive-fails=" + m(/(?=(a+))\1a/, "aaa"));
