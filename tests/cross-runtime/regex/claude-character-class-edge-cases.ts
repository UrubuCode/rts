// Cross-runtime: the corners of a character class in Annex-B (no /u) syntax —
// [] never matches anything while [^] matches everything including a newline,
// \b means BACKSPACE inside a class but a word boundary outside it, a dash at an
// edge is a literal, and \d\w\s keep their meaning inside. Nothing in the corpus
// tests class syntax itself.

function t(re: RegExp, s: string): string {
  return String(re.test(s));
}

// --- the empty class matches nothing; the negated empty class matches all ---
console.log("empty=" + t(/[]/, "a"));
console.log("empty-empty=" + t(/[]/, ""));
console.log("empty-any=" + t(/a[]b/, "ab"));
console.log("negempty-letter=" + t(/[^]/, "a"));
console.log("negempty-newline=" + t(/[^]/, "\n"));
console.log("negempty-astral=" + "\u{1F600}".replace(/[^]/g, "-"));
console.log("negempty-len=" + "a\nb".match(/[^]/g)?.length);

// --- . excludes line terminators unless /s ---
console.log("dot-newline=" + t(/a.b/, "a\nb"));
console.log("dot-s=" + t(/a.b/s, "a\nb"));
console.log("dot-cr=" + t(/a.b/, "a\rb"));
console.log("dot-ls=" + t(/a.b/, "a\u2028b"));
console.log("dot-tab=" + t(/a.b/, "a\tb"));
console.log("dot-astral-units=" + ("\u{1F600}".match(/./g) as any).length);
console.log("dot-astral-u=" + ("\u{1F600}".match(/./gu) as any).length);

// --- \b: word boundary outside a class, backspace inside one ---
console.log("boundary=" + t(/\bab\b/, "ab"));
console.log("boundary-mid=" + t(/\bb/, "ab"));
console.log("backspace=" + t(/[\b]/, "\b"));
console.log("backspace-not-b=" + t(/[\b]/, "b"));
console.log("backspace-code=" + "x\bx".replace(/[\b]/, "-"));
console.log("nonboundary=" + t(/\Bb/, "ab"));

// --- a dash is literal at either edge or when escaped ---
console.log("dash-first=" + t(/[-a]/, "-"));
console.log("dash-last=" + t(/[a-]/, "-"));
console.log("dash-escaped=" + t(/[a\-z]/, "-"));
console.log("dash-escaped-range=" + t(/[a\-z]/, "m"));
console.log("dash-range=" + t(/[a-z]/, "m"));
console.log("dash-after-class=" + t(/[\d-a]/, "-"));

// --- a caret is literal unless it is first ---
console.log("caret-first=" + t(/[^a]/, "a"));
console.log("caret-inner=" + t(/[a^]/, "^"));
console.log("caret-only=" + t(/[^]/, "^"));

// --- a closing bracket must be escaped; an opening one need not be ---
console.log("open-bracket=" + t(/[[]/, "["));
console.log("close-bracket=" + t(/[\]]/, "]"));
console.log("both=" + t(/[[\]]/, "]"));

// --- shorthand classes keep working inside a class, and can be combined ---
console.log("digit-in-class=" + t(/[\d]/, "5"));
console.log("union=" + t(/[\dA-F]/, "E"));
console.log("negated-shorthand=" + t(/[\D]/, "x"));
console.log("both-shorthands=" + t(/[\d\D]/, "\n"));
console.log("s-class=" + t(/[\s]/, " "));
console.log("S-class=" + t(/[\S]/, " "));
console.log("w-underscore=" + t(/^\w+$/, "a_1"));
console.log("w-accent=" + t(/^\w$/, "é"));

// --- ignoreCase widens ranges in both directions ---
console.log("i-range-up=" + t(/[a-z]/i, "Q"));
console.log("i-range-down=" + t(/[A-Z]/i, "q"));
console.log("i-negated=" + t(/[^a-z]/i, "Q"));
console.log("i-kelvin=" + t(/k/i, "K"));
console.log("i-sharp-s=" + t(/ss/i, "ß"));

// --- ranges are by code unit, so a reversed range is a SyntaxError ---
try {
  console.log("reversed=" + new RegExp("[z-a]").test("m"));
} catch (e: any) {
  console.log("reversed!" + e.constructor.name);
}
console.log("unit-range=" + t(/[\u0000-\uFFFF]/, "\uD800"));
console.log("hex-range=" + t(/[\x41-\x43]/, "B"));
console.log("octal-ish=" + t(/[\101]/, "A"));

// --- an unknown escape is the literal character without /u, an error with it ---
console.log("unknown-escape=" + t(/[\q]/, "q"));
try {
  console.log("unknown-escape-u=" + new RegExp("[\\q]", "u").test("q"));
} catch (e: any) {
  console.log("unknown-escape-u!" + e.constructor.name);
}
console.log("source-roundtrip=" + /[-a\]]/.source);
