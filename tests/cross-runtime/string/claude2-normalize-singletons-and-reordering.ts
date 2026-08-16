// Cross-runtime: the two parts of normalization that are not "decompose then
// recompose". A SINGLETON (U+212B ANGSTROM SIGN, U+2126 OHM SIGN) decomposes to
// a different character and NFC never puts it back; and a run of combining marks
// is REORDERED by canonical combining class, which is what makes two visually
// identical strings compare equal. 194/claude-normalize-forms only use a single
// accent. All non-ASCII is built from code points.

function cp(s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(" ");
}

function forms(label: string, s: string): void {
  console.log(
    label +
      " in=" + cp(s) +
      " nfc=" + cp(s.normalize("NFC")) +
      " nfd=" + cp(s.normalize("NFD")),
  );
}

const ANGSTROM = String.fromCharCode(0x212b);
const A_RING = String.fromCharCode(0xc5);
const RING = String.fromCharCode(0x30a);
const OHM = String.fromCharCode(0x2126);
const OMEGA = String.fromCharCode(0x3a9);
const KELVIN = String.fromCharCode(0x212a);

// --- singletons: NFC leaves you at the canonical target, never at the source ---
forms("angstrom", ANGSTROM);
console.log("angstrom-nfc-is-Aring=" + (ANGSTROM.normalize("NFC") === A_RING));
console.log("Aring-nfc-is-Aring=" + (A_RING.normalize("NFC") === A_RING));
console.log("angstrom-not-restored=" + (A_RING.normalize("NFC") === ANGSTROM));
forms("ohm", OHM);
console.log("ohm-nfc-is-omega=" + (OHM.normalize("NFC") === OMEGA));
forms("kelvin", KELVIN);
console.log("kelvin-nfc-is-K=" + (KELVIN.normalize("NFC") === "K"));
console.log("kelvin-unchanged=" + (KELVIN.normalize("NFC") === KELVIN));

// --- so normalization UNIFIES what case mapping does not ---
console.log("angstrom-eq-Aring-raw=" + (ANGSTROM === A_RING));
console.log("angstrom-eq-Aring-nfc=" + (ANGSTROM.normalize("NFC") === A_RING.normalize("NFC")));
console.log("angstrom-lower=" + cp(ANGSTROM.toLowerCase()));

// --- canonical ORDERING: two marks of different class are sorted by class ---
const DOT_BELOW = String.fromCharCode(0x323); // ccc 220
const DOT_ABOVE = String.fromCharCode(0x307); // ccc 230
forms("q-below-above", "q" + DOT_BELOW + DOT_ABOVE);
forms("q-above-below", "q" + DOT_ABOVE + DOT_BELOW);
console.log("reorder-equal=" + (
  ("q" + DOT_BELOW + DOT_ABOVE).normalize("NFD") === ("q" + DOT_ABOVE + DOT_BELOW).normalize("NFD")
));
console.log("raw-not-equal=" + (("q" + DOT_BELOW + DOT_ABOVE) === ("q" + DOT_ABOVE + DOT_BELOW)));
console.log("nfc-equal=" + (
  ("q" + DOT_BELOW + DOT_ABOVE).normalize("NFC") === ("q" + DOT_ABOVE + DOT_BELOW).normalize("NFC")
));

// --- marks of the SAME class are never reordered, so order still matters ---
const ACUTE = String.fromCharCode(0x301); // ccc 230
forms("same-class-1", "e" + ACUTE + DOT_ABOVE);
forms("same-class-2", "e" + DOT_ABOVE + ACUTE);
console.log("same-class-differ=" + (
  ("e" + ACUTE + DOT_ABOVE).normalize("NFD") !== ("e" + DOT_ABOVE + ACUTE).normalize("NFD")
));

// --- composition only reaches the FIRST mark past a blocker ---
forms("s-dot-dot", "s" + DOT_BELOW + DOT_ABOVE);
console.log("s-dot-dot-nfc-len=" + ("s" + DOT_BELOW + DOT_ABOVE).normalize("NFC").length);
forms("s-above-first", "s" + DOT_ABOVE + DOT_BELOW);
console.log("blocked-len=" + ("s" + DOT_ABOVE + DOT_BELOW).normalize("NFC").length);

// --- COMPOSITION EXCLUSIONS: a pair that decomposes but never recomposes ---
const DIAERESIS_ACUTE = String.fromCharCode(0x344); // COMBINING GREEK DIALYTIKA TONOS
forms("excluded-344", "a" + DIAERESIS_ACUTE);
console.log("344-nfc-len=" + ("a" + DIAERESIS_ACUTE).normalize("NFC").length);
const ANGSTROM_SEQ = "A" + RING;
forms("A-plus-ring", ANGSTROM_SEQ);
console.log("A-ring-composes=" + (ANGSTROM_SEQ.normalize("NFC") === A_RING));

// --- Hebrew/Devanagari style: a mark with ccc 0 blocks nothing but composes not ---
const DEVA_QA = String.fromCharCode(0x915) + String.fromCharCode(0x93c);
forms("deva-nukta", DEVA_QA);
console.log("deva-precomposed=" + cp(String.fromCharCode(0x958).normalize("NFD")));
console.log("deva-nfc-back=" + cp(String.fromCharCode(0x958).normalize("NFC")));

// --- idempotence holds for every form on every sample above ---
const samples: string[] = [ANGSTROM, OHM, KELVIN, "q" + DOT_ABOVE + DOT_BELOW, "a" + DIAERESIS_ACUTE, String.fromCharCode(0x958)];
const names: string[] = ["NFC", "NFD", "NFKC", "NFKD"];
for (let i = 0; i < samples.length; i++) {
  const row: string[] = [];
  for (let f = 0; f < names.length; f++) {
    const once = samples[i].normalize(names[f]);
    row.push(names[f] + "=" + (once.normalize(names[f]) === once));
  }
  console.log("idem" + i + " " + row.join(" "));
}

// --- length can shrink, grow, or stay: pin all three on the same call ---
console.log("grow=" + A_RING.length + "->" + A_RING.normalize("NFD").length);
console.log("shrink=" + ANGSTROM_SEQ.length + "->" + ANGSTROM_SEQ.normalize("NFC").length);
console.log("same=" + ANGSTROM.length + "->" + ANGSTROM.normalize("NFC").length);
