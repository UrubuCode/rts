// Cross-runtime: isWellFormed / toWellFormed over lone surrogates.
// toWellFormed replaces each LONE surrogate with U+FFFD (one unit -> one unit,
// so .length is preserved) and leaves valid pairs untouched.

function codes(s: string): string {
  return s.split("").map((c) => c.charCodeAt(0).toString(16)).join(",");
}

function probe(label: string, s: string): void {
  const fixed = s.toWellFormed();
  console.log(
    label +
      " wf=" + s.isWellFormed() +
      " len=" + s.length +
      " fixedLen=" + fixed.length +
      " fixed=" + codes(fixed),
  );
}

// --- well-formed baselines ---
probe("ascii", "abc");
probe("empty", "");
probe("bmp", "caf" + String.fromCharCode(0xE9));
probe("valid-pair", String.fromCharCode(0xD83D) + String.fromCharCode(0xDE00));
probe("pair-in-text", "a" + String.fromCharCode(0xD83D) + String.fromCharCode(0xDE00) + "b");
probe("two-pairs", String.fromCharCode(0xD83D) + String.fromCharCode(0xDE00) + String.fromCharCode(0xD83D) + String.fromCharCode(0xDE01));

// --- a lone HIGH surrogate ---
probe("lone-hi", String.fromCharCode(0xD83D));
probe("lone-hi-mid", "a" + String.fromCharCode(0xD83D) + "b");
probe("lone-hi-end", "ab" + String.fromCharCode(0xD83D));
probe("lone-hi-start", String.fromCharCode(0xD83D) + "ab");

// --- a lone LOW surrogate ---
probe("lone-lo", String.fromCharCode(0xDE00));
probe("lone-lo-mid", "a" + String.fromCharCode(0xDE00) + "b");
probe("lone-lo-start", String.fromCharCode(0xDE00) + "ab");

// --- REVERSED pair: low then high is two lone surrogates, not a pair ---
probe("reversed-pair", String.fromCharCode(0xDE00) + String.fromCharCode(0xD83D));

// --- high followed by a non-low: the high is lone ---
probe("hi-then-ascii", String.fromCharCode(0xD83D) + "a");
probe("hi-then-hi", String.fromCharCode(0xD83D) + String.fromCharCode(0xD83D));

// --- low followed by low ---
probe("lo-then-lo", String.fromCharCode(0xDE00) + String.fromCharCode(0xDE00));

// --- three highs then one low: only the last high pairs up ---
probe("hi-hi-hi-lo", String.fromCharCode(0xD83D) + String.fromCharCode(0xD83D) + String.fromCharCode(0xD83D) + String.fromCharCode(0xDE00));

// --- a valid pair immediately after a lone high ---
probe("lone-then-pair", String.fromCharCode(0xD83D) + String.fromCharCode(0xD83D) + String.fromCharCode(0xDE00));

// --- a valid pair immediately before a lone low ---
probe("pair-then-lone", String.fromCharCode(0xD83D) + String.fromCharCode(0xDE00) + String.fromCharCode(0xDE00));

// --- surrogate range boundaries ---
probe("hi-min-D800", String.fromCharCode(0xD800));
probe("hi-max-DBFF", String.fromCharCode(0xDBFF));
probe("lo-min-DC00", String.fromCharCode(0xDC00));
probe("lo-max-DFFF", String.fromCharCode(0xDFFF));
probe("min-pair", String.fromCharCode(0xD800) + String.fromCharCode(0xDC00));
probe("max-pair", String.fromCharCode(0xDBFF) + String.fromCharCode(0xDFFF));

// --- chars adjacent to the surrogate block are ordinary ---
probe("D7FF", String.fromCharCode(0xD7FF));
probe("E000", String.fromCharCode(0xE000));

// --- U+FFFD itself is well-formed and passes through ---
probe("replacement-char", String.fromCharCode(0xFFFD));

// --- toWellFormed is idempotent, and its result is always well-formed ---
const broken = "a" + String.fromCharCode(0xD83D) + "b" + String.fromCharCode(0xDE00) + "c";
console.log("idempotent=" + (broken.toWellFormed().toWellFormed() === broken.toWellFormed()));
console.log("fixed-is-wf=" + broken.toWellFormed().isWellFormed());

// --- a well-formed string is returned unchanged ---
const good = "a" + String.fromCharCode(0xD83D) + String.fromCharCode(0xDE00) + "b";
console.log("unchanged=" + (good.toWellFormed() === good));

// --- length is always preserved (unit-for-unit substitution) ---
console.log("len-preserved=" + (broken.toWellFormed().length === broken.length));

// --- the receiver is not mutated ---
broken.toWellFormed();
console.log("no-mutate=" + codes(broken));
