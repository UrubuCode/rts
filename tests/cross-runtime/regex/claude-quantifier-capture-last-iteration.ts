// Cross-runtime: when a capturing group sits inside a quantifier, the result
// holds only the LAST iteration's text, and every group inside the loop is RESET
// to undefined at the start of each repetition — so /(?:(a)|(b))+/ over "ab"
// leaves group 1 undefined even though it matched on the first pass. Nothing in
// the corpus pins quantifier/capture interaction.

function ex(re: RegExp, s: string): string {
  const m: any = re.exec(s);
  if (m === null) return "null";
  const parts: string[] = [];
  for (let i = 0; i < m.length; i++) parts.push(String(m[i]));
  return "@" + m.index + " [" + parts.join("|") + "]";
}

// --- only the last repetition is kept ---
console.log("last=" + ex(/(?:(\w)\d)+/, "a1b2c3"));
console.log("last-pair=" + ex(/(\w\d)+/, "a1b2"));
console.log("last-two=" + ex(/(?:(\w)(\d))+/, "a1b2"));

// --- a group in a NOT-TAKEN alternative is reset, not left over ---
console.log("alt-ab=" + ex(/(?:(a)|(b))+/, "ab"));
console.log("alt-ba=" + ex(/(?:(a)|(b))+/, "ba"));
console.log("alt-aa=" + ex(/(?:(a)|(b))+/, "aa"));
console.log("alt-once=" + ex(/(?:(a)|(b))/, "a"));

// --- an optional group after a successful earlier attempt is reset too ---
console.log("opt-reset=" + ex(/(?:(z)x)?(a)/, "a"));
console.log("opt-taken=" + ex(/(?:(z)x)?(a)/, "zxa"));
console.log("opt-quest=" + ex(/(a)?b/, "b"));
console.log("opt-quest-hit=" + ex(/(a)?b/, "ab"));

// --- greedy vs lazy on the same subject ---
console.log("greedy=" + ex(/<(.+)>/, "<a><b>"));
console.log("lazy=" + ex(/<(.+?)>/, "<a><b>"));
console.log("greedy-star=" + ex(/a(.*)a/, "abaca"));
console.log("lazy-star=" + ex(/a(.*?)a/, "abaca"));
console.log("greedy-backoff=" + ex(/(\d+)(\d)/, "12345"));
console.log("lazy-forward=" + ex(/(\d+?)(\d)/, "12345"));

// --- a group that can match empty inside * ---
console.log("empty-star=" + ex(/(a?)*/, "b"));
console.log("empty-star-hit=" + ex(/(a?)*/, "aa"));
console.log("empty-plus=" + ex(/(a?)+/, "b"));
console.log("empty-group-star=" + ex(/(?:()){2}/, "x"));
console.log("empty-inner=" + ex(/(a|)+/, "aa"));

// --- bounded quantifiers ---
console.log("exact=" + ex(/(?:(\w)){3}/, "abcd"));
console.log("range=" + ex(/(?:(\w)){2,3}/, "abcd"));
console.log("range-lazy=" + ex(/(?:(\w)){2,3}?/, "abcd"));
console.log("atmost=" + ex(/(?:(\w)){0,2}/, "ab"));
console.log("brace-literal=" + ex(/a{,2}/, "a{,2}"));
console.log("brace-open=" + ex(/a{2/, "a{2"));

// --- nested quantifiers keep the innermost last value ---
console.log("nested=" + ex(/(?:(?:(\w)(\w))+;)+/, "ab;cd;"));
console.log("nested2=" + ex(/((\w)+)+/, "abc"));

// --- backtracking is observable through what the captures end up holding ---
console.log("backtrack=" + ex(/^(a+)(a)$/, "aaa"));
console.log("backtrack-fail=" + ex(/^(a+)(b)$/, "aaa"));
console.log("backtrack-alt=" + ex(/^(?:(ab)|(a))(b?)$/, "ab"));

// --- possessive-like behaviour via an atomic-ish lookahead ---
console.log("atomic=" + ex(/^(?=(a+))\1b$/, "aaab"));
console.log("atomic-fail=" + ex(/^(?=(a+))\1ab$/, "aaab"));

// --- global replace exposes the per-match capture reset ---
console.log("global-repl=" + "ab ba".replace(/(?:(a)|(b))+/g, (...args: any[]) =>
  "[" + String(args[1]) + "," + String(args[2]) + "]"));
console.log("matchall-caps=" + [..."a1b2".matchAll(/(?:(\w)(\d))/g)]
  .map((m: any) => m[1] + m[2]).join(" "));
