// Cross-runtime: lookbehind, including the VARIABLE-length form that JavaScript
// allows and most other regex flavours do not. The interesting corner is that a
// lookbehind is matched RIGHT TO LEFT, so a greedy quantifier inside it grabs
// from the right and captures land in a different place than intuition suggests.
// Nothing in the corpus uses (?<= ... ) at all.

function ex(re: RegExp, s: string): string {
  const m: any = re.exec(s);
  if (m === null) return "null";
  const parts: string[] = [];
  for (let i = 0; i < m.length; i++) parts.push(String(m[i]));
  return "@" + m.index + " [" + parts.join("|") + "]";
}

// --- fixed-length positive lookbehind ---
console.log("fixed=" + ex(/(?<=\$)\d+/, "cost: $42"));
console.log("fixed-miss=" + ex(/(?<=\$)\d+/, "cost: 42"));
console.log("fixed-at-start=" + ex(/(?<=^)a/, "ab"));
console.log("fixed-replace=" + "$42 and 7".replace(/(?<=\$)\d+/g, "N"));

// --- negative lookbehind ---
console.log("neg=" + ex(/(?<!\$)\d+/, "$42 7"));
console.log("neg-replace=" + "$42 7".replace(/(?<!\$)\d+/g, "N"));
console.log("neg-empty-prefix=" + ex(/(?<!x)a/, "a"));
console.log("neg-hit=" + ex(/(?<!x)a/, "xa"));

// --- VARIABLE length: alternatives of different sizes ---
console.log("var-alt=" + ex(/(?<=ab|c)d/, "cd"));
console.log("var-alt2=" + ex(/(?<=ab|c)d/, "abd"));
console.log("var-quant=" + ex(/(?<=a+)b/, "xaaab"));
console.log("var-star=" + ex(/(?<=a*)b/, "b"));
console.log("var-opt=" + ex(/(?<=xa?)b/, "xb"));
console.log("var-unbounded=" + ex(/(?<=\d+px)!/, "1234px!"));

// --- matched RIGHT TO LEFT: a greedy quantifier eats leftwards ---
console.log("rtl-greedy=" + ex(/(?<=(a+))b/, "aaab"));
console.log("rtl-lazy=" + ex(/(?<=(a+?))b/, "aaab"));
console.log("rtl-order=" + ex(/(?<=(\d)(\d))x/, "12x"));
console.log("rtl-anchor=" + ex(/(?<=^(a+))b/, "aaab"));

// --- captures inside a lookbehind survive into the result ---
console.log("cap=" + ex(/(?<=(\w)(\w))c/, "abc"));
console.log("cap-named=" + String((/(?<=(?<h>\w))b/.exec("ab") as any).groups.h));
console.log("cap-in-neg=" + ex(/(?<!(z))a/, "a"));

// --- a lookbehind consumes nothing, so it can stack with a lookahead ---
console.log("both=" + ex(/(?<=a)b(?=c)/, "abc"));
console.log("both-miss=" + ex(/(?<=a)b(?=c)/, "abd"));
console.log("zero-width=" + "abcabc".replace(/(?<=b)/g, "-"));
console.log("zero-width-neg=" + "abc".replace(/(?<!^)(?=[a-z])/g, "."));

// --- thousand separators, the canonical use ---
console.log("thousands=" + "1234567".replace(/\B(?=(\d{3})+(?!\d))/g, ","));
console.log("thousands-short=" + "12".replace(/\B(?=(\d{3})+(?!\d))/g, ","));

// --- lookbehind with the u flag over an astral character ---
console.log("astral=" + ex(/(?<=\u{1F600})b/u, "\u{1F600}b"));
console.log("astral-nou=" + ex(/(?<=\uDE00)b/, "\u{1F600}b"));

// --- lookbehind at position 0 cannot look past the start ---
console.log("start=" + ex(/(?<=a)b/, "b"));
console.log("start-neg=" + ex(/(?<!a)b/, "b"));
console.log("sliced=" + ex(/(?<=a)b/, "ab".slice(1)));

// --- with a sticky regex, lastIndex bounds the match but not the lookbehind ---
const y = /(?<=a)b/y;
y.lastIndex = 1;
console.log("sticky=" + y.test("ab") + ":" + y.lastIndex);
const y2 = /(?<=a)b/y;
y2.lastIndex = 0;
console.log("sticky-fail=" + y2.test("ab") + ":" + y2.lastIndex);

// --- source and flags round-trip ---
console.log("source=" + /(?<=a+)b/.source);
console.log("ctor=" + new RegExp("(?<=a)b").test("ab"));
console.log("nested=" + ex(/(?<=a(?<=[a-z]))b/, "ab"));
console.log("in-class-literal=" + /[(?<=a)]/.test("<"));
