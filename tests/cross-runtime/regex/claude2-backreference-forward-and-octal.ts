// Cross-runtime: a backreference to a group that has not been reached yet always
// matches the EMPTY STRING rather than failing, a group can refer to itself
// across iterations, and `\1` with no group at all is an OCTAL escape in Annex B
// but a SyntaxError under `u`. claude-backreference-nonparticipating covers a
// group that was skipped; this covers one that does not exist yet, or at all.

function m(re: RegExp, s: string): string {
  const r = re.exec(s);
  if (r === null) return "null";
  const slots: string[] = [];
  for (let i = 0; i < r.length; i++) slots.push(r[i] === undefined ? "u" : JSON.stringify(r[i]));
  return "[" + slots.join(",") + "]@" + r.index;
}

function syn(src: string, flags: string): string {
  try {
    new RegExp(src, flags);
    return "ok";
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

function codes(s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(",");
}

// --- a FORWARD reference matches empty, it does not fail ---
console.log("forward=" + m(/(\1a)/, "a"));
console.log("forward-anchored=" + m(/^\1a$/, "a"));
console.log("forward-then-group=" + m(/^\1(a)$/, "a"));
console.log("forward-quantified=" + m(/^\1*a(b)$/, "ab"));
console.log("forward-named=" + m(/\k<x>(?<x>a)/, "a"));
console.log("forward-named-anchored=" + m(/^\k<x>(?<x>ab)$/, "ab"));

// --- a group referring to ITSELF: empty on the first iteration, filled after ---
console.log("self-ref=" + m(/(a\1)+/, "aa"));
console.log("self-ref-anchored=" + m(/^(a\1?)+$/, "aaa"));
console.log("self-ref-nested=" + m(/^((a)\2)+$/, "aaaa"));
console.log("self-ref-fails=" + m(/^(a\1)$/, "aa"));

// --- a reference to a group in a FAILED alternative is empty, not undefined ---
console.log("other-alt=" + m(/^(?:(a)|b)\1$/, "b"));
console.log("other-alt-taken=" + m(/^(?:(a)|b)\1$/, "aa"));
console.log("after-reset=" + m(/^(?:(a)x)?\1b$/, "b"));

// --- a reference inside a lookahead to a group AFTER it ---
console.log("la-forward=" + m(/^(?=\1)(a)$/, "a"));
console.log("lb-forward=" + m(/^(a)(?<=\2)(b)$/, "ab"));

// --- \1 with NO group: an octal escape without `u`, a SyntaxError with it ---
console.log("no-group-1=" + syn("\\1", "") + "/" + syn("\\1", "u") + "/" + syn("\\1", "v"));
console.log("no-group-1-matches=" + /^\1$/.test(String.fromCharCode(1)));
console.log("no-group-8=" + syn("\\8", "") + "/" + syn("\\8", "u"));
console.log("no-group-8-matches=" + /^\8$/.test("8"));
console.log("no-group-9-matches=" + /^\9$/.test("9"));
console.log("octal-77=" + codes("?") + " match=" + /^\77$/.test("?"));
console.log("octal-101=" + /^\101$/.test("A"));
console.log("octal-400=" + /^\400$/.test(String.fromCharCode(0x20) + "0") + "/" + syn("\\400", ""));
console.log("nul-escape=" + /^\0$/.test(String.fromCharCode(0)) + "/" + syn("\\0", "u"));
console.log("nul-then-digit=" + syn("\\01", "") + "/" + syn("\\01", "u"));

// --- a reference NUMBER above the group count is octal in Annex B ---
console.log("over-count=" + syn("(a)\\2", "") + "/" + syn("(a)\\2", "u"));
console.log("over-count-match=" + /^(a)\2$/.test("a" + String.fromCharCode(2)));
console.log("in-count=" + m(/^(a)(b)\2$/, "abb"));

// --- \k is a literal 'k' without u when there are no named groups at all ---
console.log("bare-k=" + syn("\\k", "") + "/" + syn("\\k", "u"));
console.log("bare-k-match=" + /^\k$/.test("k"));
console.log("bare-k-with-named=" + syn("\\k(?<x>a)", ""));
console.log("unknown-name=" + syn("\\k<y>(?<x>a)", "") + "/" + syn("\\k<y>(?<x>a)", "u"));

// --- backreferences are LENGTH-sensitive, so an empty capture always matches ---
console.log("empty-capture=" + m(/^(a*)b\1$/, "b"));
console.log("empty-capture-tail=" + m(/^(a*)b\1$/, "aabaa"));
console.log("empty-capture-mismatch=" + m(/^(a*)b\1$/, "aaba"));

// --- and they compare code UNITS: an astral capture is two units either way ---
console.log("astral=" + m(/^(.)\1$/u, "\u{1F600}\u{1F600}"));
console.log("astral-nou=" + m(/^(..)\1$/, "\u{1F600}\u{1F600}"));

// --- a quantified reference repeats the captured text, not the group ---
console.log("quantified-ref=" + m(/^(ab)\1{2}$/, "ababab"));
console.log("quantified-ref-short=" + m(/^(ab)\1{2}$/, "abab"));
