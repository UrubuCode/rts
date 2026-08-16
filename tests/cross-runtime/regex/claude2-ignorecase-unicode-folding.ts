// Cross-runtime: `i` means two different things. Without `u` it is the ES5
// Canonicalize (toUpperCase, with a guard that keeps non-ASCII from folding onto
// ASCII); with `u` it is full simple CASE FOLDING, so U+212A KELVIN SIGN matches
// `k` and U+017F LATIN SMALL LETTER LONG S matches `s`. Nothing in the corpus
// compares the two canonicalizations.
// \p{...} is deliberately never combined with `i` here: the two runtimes
// disagree about case-closing a property escape, so it has no answer to pin.

function pair(src: string, subject: string): string {
  const nou = new RegExp(src, "i").test(subject);
  const u = new RegExp(src, "iu").test(subject);
  return "i=" + nou + " iu=" + u;
}

const KELVIN = "K"; // KELVIN SIGN, folds to 'k'
const ANGSTROM = "Å"; // ANGSTROM SIGN, folds to U+00E5
const LONG_S = "ſ"; // LATIN SMALL LETTER LONG S, folds to 's'
const DOTLESS_I = "ı";
const DOTTED_I = "İ";
const MICRO = "µ"; // MICRO SIGN, folds to GREEK SMALL MU
const MU = "μ";

// --- the two headline pairs: only the u canonicalization unifies them ---
console.log("kelvin-vs-k=" + pair("^k$", KELVIN));
console.log("kelvin-vs-K=" + pair("^K$", KELVIN));
console.log("k-vs-kelvin=" + pair("^" + KELVIN + "$", "k"));
console.log("longs-vs-s=" + pair("^s$", LONG_S));
console.log("longs-vs-S=" + pair("^S$", LONG_S));
console.log("s-vs-longs=" + pair("^" + LONG_S + "$", "s"));

// --- angstrom and micro fold onto non-ASCII, so the ASCII guard never applies ---
console.log("angstrom-vs-aring=" + pair("^å$", ANGSTROM));
console.log("angstrom-vs-Aring=" + pair("^Å$", ANGSTROM));
console.log("micro-vs-mu=" + pair("^" + MU + "$", MICRO));
console.log("micro-vs-Mu=" + pair("^Μ$", MICRO));

// --- Greek sigma: three forms, one folding class ---
console.log("final-vs-medial=" + pair("^σ$", "ς"));
console.log("final-vs-capital=" + pair("^Σ$", "ς"));
console.log("medial-vs-capital=" + pair("^Σ$", "σ"));

// --- Turkish I: the DEFAULT folding does not unify these, in either mode ---
console.log("dotless-vs-i=" + pair("^i$", DOTLESS_I));
console.log("dotless-vs-I=" + pair("^I$", DOTLESS_I));
console.log("dotted-vs-i=" + pair("^i$", DOTTED_I));
console.log("dotted-vs-I=" + pair("^I$", DOTTED_I));

// --- folding is SIMPLE: it never changes length, so ß never matches ss ---
console.log("sharp-vs-ss=" + pair("^ss$", "ß"));
console.log("sharp-vs-SS=" + pair("^SS$", "ß"));
console.log("sharp-vs-capital=" + pair("^ẞ$", "ß"));
console.log("ligature-fi=" + pair("^fi$", "ﬁ"));
console.log("ligature-vs-self=" + pair("^ﬁ$", "ﬁ"));

// --- inside a character class, and inside a range ---
console.log("class-kelvin=" + pair("^[k]$", KELVIN));
console.log("class-longs=" + pair("^[s]$", LONG_S));
console.log("class-two=" + pair("^[xk]$", KELVIN));
// A RANGE under /iu is deliberately absent: `[a-z]` against U+212A is answered
// differently by the two reference runtimes, so it has no answer to pin here.
console.log("range-upper=" + pair("^[A-Z]$", "a"));
console.log("range-ascii=" + pair("^[a-z]$", "K"));
console.log("negclass-kelvin=" + pair("^[^k]$", KELVIN));

// --- \w and \W under the two modes: the fold pulls K and long-s into \w ---
console.log("w-kelvin=" + pair("^\\w$", KELVIN));
console.log("w-longs=" + pair("^\\w$", LONG_S));
console.log("W-kelvin=" + pair("^\\W$", KELVIN));
console.log("b-kelvin=" + pair("\\bk", "k" + KELVIN));

// --- backreferences canonicalize too ---
console.log("backref-fold=" + pair("^(k)" + KELVIN + "\\1$", "k" + KELVIN + KELVIN));
console.log("backref-ascii=" + pair("^(a)\\1$", "aA"));

// --- astral case pairs: without `u` the pattern is two lone surrogates, so the
//     anchors cannot both hold against a single code point folded whole ---
console.log("deseret-u=" + new RegExp("^\u{10428}$", "iu").test("\u{10400}"));
console.log("deseret-nou=" + new RegExp("^\u{10428}$", "i").test("\u{10400}"));
console.log("deseret-plain=" + new RegExp("^\u{10428}$", "u").test("\u{10400}"));

// --- ASCII behaves identically in both modes ---
console.log("ascii=" + pair("^abc$", "ABC"));
console.log("ascii-digit=" + pair("^\\d$", "7"));
console.log("dot=" + pair("^.$", KELVIN));
