// Cross-runtime: a lookaround consumes nothing, but its CAPTURES survive. A
// successful lookahead leaves its groups filled even though the cursor never
// moved; a negative lookaround that succeeds (because its body failed) leaves
// them undefined; and a lookbehind matches RIGHT TO LEFT, which reverses the
// order quantifiers fill its groups in. claude-lookbehind covers what matches,
// never what is captured.

function m(re: RegExp, s: string): string {
  const r = re.exec(s);
  if (r === null) return "null";
  const slots: string[] = [];
  for (let i = 0; i < r.length; i++) slots.push(r[i] === undefined ? "u" : JSON.stringify(r[i]));
  return "[" + slots.join(",") + "]@" + r.index;
}

// --- a lookahead's captures persist after the lookahead is left ---
console.log("la-cap=" + m(/a(?=(b))/, "ab"));
console.log("la-cap-two=" + m(/a(?=(b)(c))/, "abc"));
console.log("la-zero-width=" + m(/^(?=(\w+))\w\w$/, "ab"));
console.log("la-then-use=" + m(/(?=(a+))\1/, "aaa"));
console.log("la-nested=" + m(/a(?=(b(?=(c))))/, "abc"));

// --- a NEGATIVE lookahead resets everything inside it, even on success ---
console.log("nla-success=" + m(/a(?!(b))/, "ac"));
console.log("nla-fails=" + m(/a(?!(b))/, "ab"));
console.log("nla-double=" + m(/a(?!(?!(b)))/, "ab"));
console.log("nla-named=" + JSON.stringify(String((/a(?!(?<x>b))/.exec("ac") as any).groups.x)));

// --- a lookbehind matches backwards: the LAST alternative reached first ---
console.log("lb-cap=" + m(/(?<=(b))c/, "bc"));
console.log("lb-cap-two=" + m(/(?<=(a)(b))c/, "abc"));
console.log("lb-greedy=" + m(/(?<=^(\w+))c/, "abc"));
console.log("lb-quantified=" + m(/(?<=(\w)+)$/, "abc"));
console.log("lb-quantified-fwd=" + m(/^(?:(\w))+/, "abc"));
console.log("lb-alt-order=" + m(/(?<=(ab|b))c/, "abc"));
console.log("lb-backref-order=" + m(/(?<=(\w)\1)c/, "aac"));
console.log("fwd-backref-order=" + m(/^(\w)\1/, "aac"));

// --- a negative lookbehind that succeeds also leaves nothing behind ---
console.log("nlb-success=" + m(/(?<!(a))b/, "xb"));
console.log("nlb-fails=" + m(/(?<!(a))b/, "ab"));

// --- named groups behave the same way, and land in `groups` ---
const g1: any = /a(?=(?<ahead>b))/.exec("ab");
console.log("named-ahead=" + JSON.stringify(g1.groups.ahead) + "/" + g1[0] + "/" + g1.length);
const g2: any = /(?<=(?<behind>b))c/.exec("bc");
console.log("named-behind=" + JSON.stringify(g2.groups.behind) + "/" + g2.index);
const g3: any = /a(?!(?<never>b))/.exec("ac");
console.log("named-negative=" + String(g3.groups.never) + "/" + ("never" in g3.groups));

// --- captures from a lookaround are visible to $-tokens in replace ---
console.log("replace-la=" + "ab".replace(/a(?=(b))/, "[$1]"));
console.log("replace-lb=" + "bc".replace(/(?<=(b))c/, "[$1]"));
console.log("replace-nla=" + "ac".replace(/a(?!(b))/, "[" + "$1" + "]"));
console.log("replace-named=" + "ab".replace(/a(?=(?<x>b))/, "[$<x>]"));

// --- and to a replacer function, with the same arity ---
console.log("fn-args=" + "ab".replace(/a(?=(b))/, function () {
  const a = Array.prototype.slice.call(arguments);
  return "<" + a.length + ":" + String(a[1]) + ":" + a[2] + ">";
}));

// --- backtracking inside a lookahead can rewrite a capture before it settles ---
console.log("la-backtrack=" + m(/(?=(a*)aa)a/, "aaa"));
console.log("la-backtrack-none=" + m(/(?=(a*)b)/, "aab"));

// --- a lookahead applied to a quantifier keeps only the last successful body ---
console.log("quant-la=" + m(/^(?:(?=(\w))\w)+$/, "abc"));
console.log("quant-lb=" + m(/^(?:\w(?<=(\w)))+$/, "abc"));

// --- lookarounds consume nothing, so lastIndex advances only by the body ---
const g = /(?=(a))/g;
console.log("zero-width-g=" + JSON.stringify("aa".match(g)));
const y = /a(?=(b))/y;
y.lastIndex = 0;
y.exec("ab");
console.log("sticky-lastIndex=" + y.lastIndex);
console.log("index-of-lb=" + (/(?<=ab)c/.exec("abc") as any).index);
