// Cross-runtime: toUpperCase / toLowerCase apply Unicode SpecialCasing, so they
// can CHANGE THE LENGTH of a string (ß -> SS, ﬃ -> FFI, İ -> i + U+0307) and are
// not round-trippable. 225_string_tolowercase_touppercase.ts only tests ASCII.
// The locale-sensitive variants are deliberately not used.

function codes(s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16).toUpperCase());
  return out.join(" ");
}

function up(s: string): string {
  const u = s.toUpperCase();
  return u + "/" + u.length + "/" + codes(u);
}

function down(s: string): string {
  const d = s.toLowerCase();
  return d + "/" + d.length + "/" + codes(d);
}

// --- one code unit in, two out ---
console.log("sharp-s=" + up("ß"));
console.log("sharp-s-in-word=" + up("straße"));
console.log("sharp-s-roundtrip=" + ("ß".toUpperCase().toLowerCase() === "ß"));
console.log("capital-sharp-s-down=" + down("ẞ"));
console.log("capital-sharp-s-up=" + up("ẞ"));

// --- ligatures expand ---
console.log("fi=" + up("ﬁ"));
console.log("fl=" + up("ﬂ"));
console.log("ffi=" + up("ﬃ"));
console.log("ffl=" + up("ﬄ"));
console.log("st=" + up("ﬅ"));
console.log("ligature-down=" + down("ﬁ"));

// --- Greek sigma: both lowercase forms uppercase to the same letter ---
console.log("final-sigma-up=" + up("ς"));
console.log("medial-sigma-up=" + up("σ"));
console.log("sigma-down=" + down("Σ"));
console.log("sigma-roundtrip=" + ("ς".toUpperCase().toLowerCase() === "ς"));
console.log("word-sigma=" + up("ὀδυσσεύς"));
console.log("word-sigma-down=" + down("ΟΔΥΣΣΕΥΣ"));

// --- dotted and dotless I: the DEFAULT (locale-independent) mapping ---
console.log("dotless-up=" + up("ı"));
console.log("dotted-down=" + down("İ"));
console.log("dotted-up=" + up("İ"));
console.log("ascii-i-up=" + up("i"));
console.log("ascii-I-down=" + down("I"));

// --- other multi-character expansions ---
console.log("ypogegrammeni=" + up("ᾀ"));
console.log("hebrew-yod=" + up("ﬗ"));
console.log("armenian-ech=" + up("ﬔ"));
console.log("n-preceded=" + up("ŉ"));

// --- astral characters map too (Deseret) ---
console.log("deseret-up=" + "\u{10428}".toUpperCase().codePointAt(0)?.toString(16));
console.log("deseret-down=" + "\u{10400}".toLowerCase().codePointAt(0)?.toString(16));
console.log("deseret-len=" + "\u{10428}".toUpperCase().length);

// --- characters with no case are untouched ---
console.log("digits=" + up("123"));
console.log("cjk=" + up("中文"));
console.log("emoji-len=" + "\u{1F600}".toUpperCase().length);
console.log("lone-surrogate=" + codes("\uD800".toUpperCase()));
console.log("empty=" + "[" + "".toUpperCase() + "]");

// --- case folding is not the same as either mapping ---
console.log("kelvin-down=" + down("K"));
console.log("angstrom-down=" + down("Å"));
console.log("micro-up=" + up("µ"));
console.log("turkish-i-up=" + up("ı") + "|" + up("i"));

// --- case conversion is not a bijection: uppercase then lowercase loses info ---
console.log("lossy=" + ("ﬁ".toUpperCase().toLowerCase() === "ﬁ"));
console.log("lossy2=" + ("İ".toLowerCase().toUpperCase().length));
