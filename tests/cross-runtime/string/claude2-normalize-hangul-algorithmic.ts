// Cross-runtime: Hangul is the one block whose normalization is ALGORITHMIC, not
// table-driven — S = 0xAC00 + (L*21 + V)*28 + T — so an engine can pass every
// Latin normalize test and still fail here. 194 and claude-normalize-forms only
// use Latin combining marks and compatibility singletons.
// Every syllable is built from code points so the file stays plain ASCII.

const SBASE = 0xac00;
const LBASE = 0x1100;
const VBASE = 0x1161;
const TBASE = 0x11a7;

function cp(s: string): string {
  const out: string[] = [];
  for (const ch of s) out.push((ch.codePointAt(0) as any).toString(16));
  return out.join(" ");
}

function syllable(l: number, v: number, t: number): string {
  return String.fromCharCode(SBASE + (l * 21 + v) * 28 + t);
}

function show(label: string, s: string): void {
  const nfd = s.normalize("NFD");
  const nfc = s.normalize("NFC");
  console.log(
    label +
      " in=" + cp(s) +
      " nfd=" + cp(nfd) + "(" + nfd.length + ")" +
      " nfc=" + cp(nfc) + "(" + nfc.length + ")" +
      " round=" + (nfd.normalize("NFC") === nfc),
  );
}

// --- an LV syllable decomposes to two jamo, an LVT to three ---
show("ga", syllable(0, 0, 0));
show("gag", syllable(0, 0, 1));
show("hih", syllable(18, 20, 27));
show("first", String.fromCharCode(SBASE));
show("last", String.fromCharCode(0xd7a3));

// --- composing the jamo back is the same algorithm run forwards ---
const L = String.fromCharCode(LBASE);
const V = String.fromCharCode(VBASE);
const T = String.fromCharCode(TBASE + 1);
show("L+V", L + V);
show("L+V+T", L + V + T);
show("LV+T", String.fromCharCode(SBASE) + T);
console.log("lv-plus-t-equals=" + ((String.fromCharCode(SBASE) + T).normalize("NFC") === syllable(0, 0, 1)));

// --- partial sequences do NOT compose: a lone L, a lone V, an L+T ---
show("L-alone", L);
show("V-alone", V);
show("T-alone", T);
show("L+T", L + T);
show("V+T", V + T);
show("T+V", T + V);
show("V+L", V + L);

// --- an L followed by a full syllable does not absorb it ---
show("L+syllable", L + syllable(0, 0, 0));
show("syllable+V", syllable(0, 0, 0) + V);
show("syllable+T-twice", syllable(0, 0, 1) + T);

// --- two syllables in a row stay two ---
show("two", syllable(0, 0, 0) + syllable(1, 1, 1));
show("word", syllable(11, 5, 0) + syllable(6, 0, 21));

// --- NFKC and NFKD agree with NFC and NFD on modern Hangul ---
const s1 = syllable(0, 0, 1);
console.log("nfkd-eq-nfd=" + (s1.normalize("NFKD") === s1.normalize("NFD")));
console.log("nfkc-eq-nfc=" + (s1.normalize("NFKC") === s1.normalize("NFC")));

// --- but COMPATIBILITY jamo (the U+31xx block) map to conjoining jamo under K ---
const COMPAT_KIYEOK = String.fromCharCode(0x3131);
console.log("compat-nfc=" + cp(COMPAT_KIYEOK.normalize("NFC")));
console.log("compat-nfkc=" + cp(COMPAT_KIYEOK.normalize("NFKC")));
console.log("compat-nfkd=" + cp(COMPAT_KIYEOK.normalize("NFKD")));
const HALFWIDTH_KIYEOK = String.fromCharCode(0xffa1);
console.log("halfwidth-nfkc=" + cp(HALFWIDTH_KIYEOK.normalize("NFKC")));
console.log("halfwidth-nfc=" + cp(HALFWIDTH_KIYEOK.normalize("NFC")));

// --- a compatibility jamo run does NOT become a syllable under NFKC ---
const run = String.fromCharCode(0x3131) + String.fromCharCode(0x314f);
console.log("compat-run-nfkc=" + cp(run.normalize("NFKC")));
console.log("compat-run-len=" + run.normalize("NFKC").length);

// --- idempotence and length invariants over a longer word ---
const word = syllable(0, 0, 0) + syllable(18, 20, 27) + syllable(11, 5, 0);
console.log("word-len=" + word.length + " nfd-len=" + word.normalize("NFD").length);
console.log("nfd-idempotent=" + (word.normalize("NFD").normalize("NFD") === word.normalize("NFD")));
console.log("nfc-idempotent=" + (word.normalize("NFC").normalize("NFC") === word.normalize("NFC")));
console.log("nfc-is-original=" + (word.normalize("NFD").normalize("NFC") === word));
console.log("equal-under-nfd=" + (word.normalize("NFD") === word.normalize("NFD")));

// --- a jamo sequence interleaved with Latin composes only in its own runs ---
const mixed = "a" + L + V + "b" + L + V + T;
console.log("mixed-nfc=" + cp(mixed.normalize("NFC")));
console.log("mixed-len=" + mixed.normalize("NFC").length);
