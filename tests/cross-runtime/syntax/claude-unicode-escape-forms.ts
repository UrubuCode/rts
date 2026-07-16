// Cross-runtime: the two Unicode escape FORMS in a string literal —
// legacy \uXXXX (exactly 4 hex digits, one code UNIT) and ES6 \u{...}
// (1-6 hex digits, a code POINT) — including the two forms written
// ADJACENT to each other inside a single literal.

function codes(s: string): string {
  return s.split("").map((c) => c.charCodeAt(0).toString(16)).join(",");
}

// --- \uXXXX alone ---
console.log("hex4=" + "\u0041");
console.log("hex4-lower=" + "\u0061");
console.log("hex4-upper-hex=" + "\u00E9");
console.log("hex4-lower-hex=" + "\u00e9");
console.log("hex4-case-eq=" + ("\u00E9" === "\u00e9"));
console.log("hex4-max=" + "\uFFFF".charCodeAt(0).toString(16));
console.log("hex4-nul-len=" + "\u0000".length);

// --- \u{...} alone: 1 to 6 hex digits, leading zeros allowed ---
console.log("brace-1=" + "\u{41}");
console.log("brace-2=" + "\u{041}");
console.log("brace-pad5=" + "\u{00041}");
console.log("brace-pad6=" + "\u{000041}");
console.log("brace-astral-len=" + "\u{1F600}".length);
console.log("brace-astral-cp=" + "\u{1F600}".codePointAt(0).toString(16));
console.log("brace-max-len=" + "\u{10FFFF}".length);
console.log("brace-bmp-len=" + "\u{FFFF}".length);

// --- the two forms ADJACENT in one literal ---
const mix1 = "\uD83D\u{1F600}";
console.log("mix-hi-then-brace-len=" + mix1.length);
console.log("mix-hi-then-brace=" + codes(mix1));

const mix2 = "\u{1F600}\uDE00";
console.log("mix-brace-then-lo-len=" + mix2.length);
console.log("mix-brace-then-lo=" + codes(mix2));

const mix3 = "\u0041B\u0043";
console.log("mix-hex4-text=" + mix3);

const mix4 = "\u{41}B\u{43}";
console.log("mix-brace-text=" + mix4);

const mix5 = "\u0041\u{42}C";
console.log("mix-alternating=" + mix5);

// --- a surrogate PAIR as two \uXXXX equals the single \u{...} form ---
console.log("pair-eq-brace=" + ("\uD83D\uDE00" === "\u{1F600}"));
console.log("pair-len=" + "\uD83D\uDE00".length);

// --- escapes adjacent to plain text / hex-looking digits ---
console.log("text-adjacent=" + "a\u0042c\u{44}e");
console.log("hex4-then-digit=" + "\u0041" + "1");
console.log("brace-then-digit=" + "\u{41}" + "1");
console.log("brace-then-brace-char=" + "\u{41}{}");

// --- the same escapes inside a template literal ---
console.log("tpl-hex4=" + `\u0041`);
console.log("tpl-brace=" + `\u{42}`);
console.log("tpl-mix-len=" + `\uD83D\u{1F600}`.length);
console.log("tpl-interp=" + `${"\u0041"}\u{42}`);

// --- escapes as object keys ---
const obj: any = { "\u0041": 1, "\u{42}": 2 };
console.log("key-hex4=" + obj.A);
console.log("key-brace=" + obj.B);

// --- lone surrogates in each form stay lone and are equal ---
console.log("lone-hi-len=" + "\uD83D".length);
console.log("lone-brace-hi-len=" + "\u{D83D}".length);
console.log("lone-forms-eq=" + ("\uD83D" === "\u{D83D}"));

// --- a run of alternating forms ---
const run = "A\u{1F600}B\uD83D\uDE01C";
console.log("run-len=" + run.length);
console.log("run-points=" + [...run].length);
