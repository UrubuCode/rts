// Cross-runtime: case-INSENSITIVE matching and case MAPPING are two different
// Unicode operations, and this pins the pairs where they disagree in both
// directions. /iu unifies K with k (simple case folding) while toLowerCase does
// too — but /iu refuses to unify ß with SS, which toUpperCase does, because
// folding is length-preserving and mapping is not. claude-case-mapping-special
// pins the mapping alone; nothing compares the two.
// \p{...} is never combined with `i` here: the two runtimes disagree there.

function row(label: string, a: string, b: string): void {
  console.log(
    label +
      " fold=" + new RegExp("^" + a + "$", "iu").test(b) +
      " foldNoU=" + new RegExp("^" + a + "$", "i").test(b) +
      " lower=" + (a.toLowerCase() === b.toLowerCase()) +
      " upper=" + (a.toUpperCase() === b.toUpperCase()) +
      " lenA=" + a.length + " lenB=" + b.length,
  );
}

const KELVIN = String.fromCharCode(0x212a);
const ANGSTROM = String.fromCharCode(0x212b);
const LONG_S = String.fromCharCode(0x17f);
const MICRO = String.fromCharCode(0xb5);
const MU = String.fromCharCode(0x3bc);
const OHM = String.fromCharCode(0x2126);
const OMEGA = String.fromCharCode(0x3a9);
const SHARP_S = String.fromCharCode(0xdf);
const CAP_SHARP_S = String.fromCharCode(0x1e9e);
const FI = String.fromCharCode(0xfb01);
const DOTLESS_I = String.fromCharCode(0x131);
const DOTTED_I = String.fromCharCode(0x130);
const FINAL_SIGMA = String.fromCharCode(0x3c2);
const SIGMA = String.fromCharCode(0x3c3);
const CAP_SIGMA = String.fromCharCode(0x3a3);
const CHEROKEE_UP = String.fromCharCode(0x13a0);
const CHEROKEE_LOW = String.fromCharCode(0xab70);

// --- both agree: the singleton-style pairs ---
row("kelvin-k", KELVIN, "k");
row("angstrom-aring", ANGSTROM, String.fromCharCode(0xe5));
row("longs-s", LONG_S, "s");
row("micro-mu", MICRO, MU);
row("ohm-omega", OHM, OMEGA);
row("sigma-final", SIGMA, FINAL_SIGMA);
row("sigma-capital", CAP_SIGMA, FINAL_SIGMA);
row("cherokee", CHEROKEE_UP, CHEROKEE_LOW);
row("ascii", "A", "a");

// --- folding refuses what mapping accepts: the length-changing expansions ---
row("sharp-ss", SHARP_S, "ss");
row("sharp-SS", SHARP_S, "SS");
row("sharp-capital", SHARP_S, CAP_SHARP_S);
row("fi-ligature", FI, "fi");
row("fi-ligature-upper", FI, "FI");

// --- mapping refuses what folding accepts is the rarer direction ---
row("dotless-i", DOTLESS_I, "i");
row("dotted-I", DOTTED_I, "I");
row("dotted-i", DOTTED_I, "i");

// --- the round-trip identities each operation breaks ---
const samples: any[][] = [
  ["kelvin", KELVIN], ["angstrom", ANGSTROM], ["longs", LONG_S], ["micro", MICRO],
  ["sharp", SHARP_S], ["fi", FI], ["final-sigma", FINAL_SIGMA], ["dotted-I", DOTTED_I],
  ["dotless-i", DOTLESS_I], ["ascii-a", "a"],
];
for (let i = 0; i < samples.length; i++) {
  const s: string = samples[i][1];
  console.log(
    "trip-" + samples[i][0] +
      " up.down=" + (s.toUpperCase().toLowerCase() === s) +
      " down.up=" + (s.toLowerCase().toUpperCase() === s) +
      " upLen=" + s.toUpperCase().length +
      " downLen=" + s.toLowerCase().length +
      " selfFold=" + new RegExp("^" + s + "$", "iu").test(s),
  );
}

// --- a case-insensitive INDEX search has no built-in, so lowercasing is the
//     usual substitute — and it finds what /iu does not, and misses what it does ---
function insensitiveIndexOf(hay: string, needle: string): number {
  return hay.toLowerCase().indexOf(needle.toLowerCase());
}
console.log("map-search-kelvin=" + insensitiveIndexOf("a" + KELVIN + "b", "k"));
console.log("fold-search-kelvin=" + ("a" + KELVIN + "b").search(/k/iu));
console.log("map-search-sharp=" + insensitiveIndexOf("aSSb", SHARP_S));
console.log("fold-search-sharp=" + ("aSSb").search(new RegExp(SHARP_S, "iu")));
console.log("map-search-longs=" + insensitiveIndexOf("a" + LONG_S + "b", "s"));
console.log("fold-search-longs=" + ("a" + LONG_S + "b").search(/s/iu));

// --- normalization is a THIRD operation again: it unifies the singletons only ---
console.log("nfc-kelvin=" + (KELVIN.normalize("NFC") === "K"));
console.log("nfc-longs=" + (LONG_S.normalize("NFC") === "s"));
console.log("nfkc-longs=" + (LONG_S.normalize("NFKC") === "s"));
console.log("nfkc-fi=" + (FI.normalize("NFKC") === "fi"));
console.log("nfkc-sharp=" + (SHARP_S.normalize("NFKC") === SHARP_S));
console.log("nfkc-micro=" + (MICRO.normalize("NFKC") === MU));

// --- and the three combined is what a real "same word?" check needs ---
function same(a: string, b: string): boolean {
  return a.normalize("NFKC").toLowerCase() === b.normalize("NFKC").toLowerCase();
}
console.log("same-kelvin=" + same(KELVIN, "k"));
console.log("same-fi=" + same(FI, "fi"));
console.log("same-sharp=" + same(SHARP_S, "ss"));
console.log("same-sharp-upper=" + same(SHARP_S.toUpperCase(), "ss"));
console.log("same-micro=" + same(MICRO, MU));
console.log("same-dotted=" + same(DOTTED_I, "i"));
