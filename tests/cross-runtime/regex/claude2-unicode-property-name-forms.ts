// Cross-runtime: \p{...} has THREE spellings that must all resolve to the same
// set — a lone binary/general-category name, `General_Category=X`, and the
// aliases (`gc=`, `sc=`, `scx=`, `Lu`, `Grek`) — and a strict list of what is
// REFUSED. codex_regex_unicode_property_escape uses only a couple of long forms;
// nothing pins the alias table or the rejections.
// `i` is deliberately never combined with \p: the two reference runtimes
// disagree about case-closing a property escape, so it has no answer to pin.

function t(src: string, subject: string): string {
  try {
    return String(new RegExp(src, "u").test(subject));
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

function syn(src: string): string {
  try {
    new RegExp(src, "u");
    return "ok";
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

const ALPHA = "α"; // GREEK SMALL LETTER ALPHA
const CYR = "д"; // CYRILLIC SMALL LETTER DE
const HIRA = "あ"; // HIRAGANA LETTER A
const KATA_DOT = "・"; // KATAKANA MIDDLE DOT: Common script, Kana in Script_Extensions

// --- General_Category: bare, long, short, and the `gc=` prefix ---
console.log("Lu-bare=" + t("^\\p{Lu}$", "A"));
console.log("Lu-long=" + t("^\\p{Uppercase_Letter}$", "A"));
console.log("Lu-gc=" + t("^\\p{gc=Lu}$", "A"));
console.log("Lu-gc-long=" + t("^\\p{General_Category=Uppercase_Letter}$", "A"));
console.log("Lu-on-lower=" + t("^\\p{Lu}$", "a"));
console.log("Ll=" + t("^\\p{Ll}$", "a") + "/" + t("^\\p{Lowercase_Letter}$", "a"));
console.log("Nd=" + t("^\\p{Nd}$", "7") + "/" + t("^\\p{Decimal_Number}$", "7"));
console.log("L-supercat=" + t("^\\p{L}$", ALPHA) + "/" + t("^\\p{Letter}$", ALPHA));
console.log("P-punct=" + t("^\\p{P}$", ",") + "/" + t("^\\p{Punctuation}$", ","));
console.log("Zs-space=" + t("^\\p{Zs}$", " ") + "/" + t("^\\p{Space_Separator}$", " "));

// --- Script= vs Script_Extensions=, long and short ---
console.log("sc-greek=" + t("^\\p{Script=Greek}$", ALPHA));
console.log("sc-short=" + t("^\\p{sc=Greek}$", ALPHA));
console.log("sc-code=" + t("^\\p{sc=Grek}$", ALPHA));
console.log("scx-greek=" + t("^\\p{Script_Extensions=Greek}$", ALPHA));
console.log("scx-short=" + t("^\\p{scx=Grek}$", ALPHA));
console.log("sc-cyrillic=" + t("^\\p{Script=Cyrillic}$", CYR));
console.log("sc-greek-on-cyr=" + t("^\\p{Script=Greek}$", CYR));
console.log("sc-hiragana=" + t("^\\p{Script=Hiragana}$", HIRA));

// --- the pair that makes Script_Extensions a different question ---
console.log("dot-sc-common=" + t("^\\p{Script=Common}$", KATA_DOT));
console.log("dot-sc-kana=" + t("^\\p{Script=Katakana}$", KATA_DOT));
console.log("dot-scx-kana=" + t("^\\p{Script_Extensions=Katakana}$", KATA_DOT));
console.log("dot-scx-hira=" + t("^\\p{Script_Extensions=Hiragana}$", KATA_DOT));

// --- binary properties take the bare form only ---
console.log("Alphabetic=" + t("^\\p{Alphabetic}$", ALPHA));
console.log("Alpha-alias=" + t("^\\p{Alpha}$", ALPHA));
console.log("ASCII=" + t("^\\p{ASCII}$", "a") + "/" + t("^\\p{ASCII}$", ALPHA));
console.log("Any=" + t("^\\p{Any}$", "\u{10000}"));
console.log("Assigned=" + t("^\\p{Assigned}$", "a"));
console.log("White_Space=" + t("^\\p{White_Space}$", "\t"));
console.log("Emoji-prop=" + t("^\\p{Emoji_Presentation}$", "\u{1F600}"));
console.log("ID_Start=" + t("^\\p{ID_Start}$", "a") + "/" + t("^\\p{ID_Start}$", "7"));

// --- \P is the complement of exactly the same set ---
console.log("P-neg=" + t("^\\P{Lu}$", "a") + "/" + t("^\\P{Lu}$", "A"));
console.log("P-neg-script=" + t("^\\P{Script=Greek}$", CYR));
console.log("neg-in-class=" + t("^[^\\p{L}]$", "7"));
console.log("union-in-class=" + t("^[\\p{Nd}\\p{Lu}]$", "A") + "/" + t("^[\\p{Nd}\\p{Lu}]$", "7"));

// --- what is REFUSED ---
console.log("no-u-flag=" + (function () { try { return String(new RegExp("\\p{L}").test("p")); } catch (e: any) { return "!" + e.constructor.name; } })());
console.log("unknown-name=" + syn("\\p{NotARealProperty}"));
console.log("unknown-value=" + syn("\\p{Script=Klingon}"));
console.log("binary-with-value=" + syn("\\p{Alphabetic=Yes}"));
console.log("gc-as-binary-loose=" + syn("\\p{general_category=Lu}"));
console.log("lowercase-lu=" + syn("\\p{lu}"));
console.log("script-lowercase=" + syn("\\p{Script=greek}"));
console.log("empty-braces=" + syn("\\p{}"));
console.log("no-braces=" + syn("\\pL"));
console.log("space-in-name=" + syn("\\p{Uppercase Letter}"));
