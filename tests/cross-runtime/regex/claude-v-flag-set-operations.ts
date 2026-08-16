// Cross-runtime: the ES2024 `v` flag (unicodeSets). It is NOT just a stricter
// `u` — it turns a character class into a SET EXPRESSION with difference (--),
// intersection (&&), nesting, and multi-character string literals \q{...}, and it
// makes several punctuation characters need escaping. codex_regex_unicode_
// property_escape only uses \p{...} under /u.

function t(re: RegExp, s: string): string {
  return String(re.test(s));
}

function attempt(fn: () => any): string {
  try {
    return String(fn());
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

// --- the flag itself ---
console.log("flags=" + /a/v.flags);
console.log("unicodeSets=" + /a/v.unicodeSets);
console.log("unicode=" + /a/v.unicode);
console.log("u-unicodeSets=" + /a/u.unicodeSets);
console.log("uv=" + attempt(() => new RegExp("a", "uv").flags));
console.log("source=" + /[\p{L}--[a-z]]/v.source);

// --- difference: letters that are not lowercase ASCII ---
const diff = /^[\p{L}--[a-z]]$/v;
console.log("diff-upper=" + t(diff, "A"));
console.log("diff-lower=" + t(diff, "a"));
console.log("diff-accented=" + t(diff, "é"));
console.log("diff-digit=" + t(diff, "1"));
console.log("diff-replace=" + "aAbB".replace(/[\p{L}--[a-z]]/gv, "-"));

// --- intersection: ASCII letters only ---
const inter = /^[\p{ASCII}&&\p{L}]$/v;
console.log("inter-ascii-letter=" + t(inter, "z"));
console.log("inter-ascii-digit=" + t(inter, "7"));
console.log("inter-nonascii-letter=" + t(inter, "é"));
console.log("inter-chain=" + t(/^[\p{L}&&\p{ASCII}&&[a-m]]$/v, "c"));
console.log("inter-chain-miss=" + t(/^[\p{L}&&\p{ASCII}&&[a-m]]$/v, "z"));

// --- nested classes are a union ---
console.log("nested=" + t(/^[[a-c][0-2]]$/v, "1"));
console.log("nested-miss=" + t(/^[[a-c][0-2]]$/v, "9"));
console.log("nested-deep=" + t(/^[[[a]]]$/v, "a"));
console.log("nested-negated=" + t(/^[^[a-c]]$/v, "d"));
console.log("diff-of-union=" + t(/^[[a-z]--[aeiou]]$/v, "e") + "," + t(/^[[a-z]--[aeiou]]$/v, "b"));
console.log("inter-of-union=" + t(/^[[a-z]&&[aeiou]]$/v, "e") + "," + t(/^[[a-z]&&[aeiou]]$/v, "b"));

// --- \q{...} matches a whole STRING, not a character ---
const q = /^[\q{abc|d|}]$/v;
console.log("q-multi=" + t(q, "abc"));
console.log("q-single=" + t(q, "d"));
console.log("q-partial=" + t(q, "a"));
console.log("q-empty=" + t(q, ""));
console.log("q-replace=" + "xabcy".replace(/[\q{abc}]/v, "-"));
console.log("q-longest-first=" + "ab".replace(/[\q{ab|a}]/v, "-"));
console.log("q-union=" + t(/^[\q{ab}[c]]$/v, "c"));
console.log("q-ci=" + t(/^[\q{ab}]$/vi, "AB"));

// --- properties OF STRINGS are only legal under v ---
console.log("rgi-emoji=" + t(/^\p{RGI_Emoji}$/v, "\u{1F1E7}\u{1F1F7}"));
console.log("basic-emoji=" + t(/^\p{Basic_Emoji}$/v, "\u{1F600}"));
console.log("rgi-under-u=" + attempt(() => new RegExp("\\p{RGI_Emoji}", "u").test("a")));

// --- v implies unicode semantics: . and quantifiers work on code points ---
console.log("dot-astral=" + ("\u{1F600}".match(/./gv) as any).length);
console.log("class-astral=" + t(/^[\u{1F600}]$/v, "\u{1F600}"));
console.log("astral-range=" + t(/^[\u{1F600}-\u{1F64F}]$/v, "\u{1F607}"));

// --- v is stricter about punctuation that u leaves alone ---
console.log("paren-v=" + attempt(() => new RegExp("[(]", "v").test("(")));
console.log("paren-u=" + attempt(() => new RegExp("[(]", "u").test("(")));
console.log("paren-escaped-v=" + attempt(() => new RegExp("[\\(]", "v").test("(")));
console.log("doubled-amp-u=" + attempt(() => new RegExp("[a&&b]", "u").test("a")));
console.log("doubled-amp-v=" + attempt(() => new RegExp("[a&&b]", "v").test("a")));
console.log("single-amp-v=" + attempt(() => new RegExp("[a&b]", "v").test("&")));
console.log("bracket-v=" + attempt(() => new RegExp("[\\]]", "v").test("]")));

// --- case-insensitive v folds the whole set, including a difference ---
console.log("vi-diff=" + t(/^[\p{L}--[a-z]]$/vi, "A"));
console.log("vi-range=" + t(/^[a-z]$/vi, "Q"));

// --- and it still round-trips through the constructor ---
const built = new RegExp("[\\p{L}--[a-z]]", "v");
console.log("built-flags=" + built.flags + "|" + built.source);
console.log("built-clone=" + new RegExp(built).flags);
console.log("built-strip=" + attempt(() => new RegExp(built, "u").flags));
