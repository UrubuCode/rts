// Cross-runtime: JavaScript has no grapheme-aware string operation, so every
// "one character" that is really a CLUSTER — a ZWJ emoji family, a flag built
// from two regional indicators, a keycap, a skin-tone modifier, a combining
// sequence — is counted three different ways by length, by the iterator, and by
// a /u regex, and every one of those numbers is specified. Nothing in the corpus
// pins the cluster shapes; the existing fixtures use single astral characters.
// Every sequence is built from code points so the file stays plain ASCII.

const ZWJ = String.fromCodePoint(0x200d);
const VS16 = String.fromCodePoint(0xfe0f);
const KEYCAP = String.fromCodePoint(0x20e3);

const MAN = String.fromCodePoint(0x1f468);
const WOMAN = String.fromCodePoint(0x1f469);
const BOY = String.fromCodePoint(0x1f466);
const WAVE = String.fromCodePoint(0x1f44b);
const SKIN4 = String.fromCodePoint(0x1f3fd);
const RI_B = String.fromCodePoint(0x1f1e7);
const RI_R = String.fromCodePoint(0x1f1f7);
const HEART = String.fromCodePoint(0x2764);
const ACUTE = String.fromCodePoint(0x301);
const CEDILLA = String.fromCodePoint(0x327);

const samples: any[][] = [
  ["family", MAN + ZWJ + WOMAN + ZWJ + BOY],
  ["wave-skin", WAVE + SKIN4],
  ["flag", RI_B + RI_R],
  ["heart-vs16", HEART + VS16],
  ["keycap", "1" + VS16 + KEYCAP],
  ["e-acute-seq", "e" + ACUTE],
  ["c-two-marks", "c" + CEDILLA + ACUTE],
  ["plain-astral", MAN],
  ["ascii", "ab"],
];

// --- the three counts, side by side ---
for (let i = 0; i < samples.length; i++) {
  const name = samples[i][0];
  const s = samples[i][1];
  console.log(
    name +
      " units=" + s.length +
      " points=" + [...s].length +
      " dotU=" + (s.match(/./gu) as any).length +
      " dot=" + (s.match(/./gs) as any).length +
      " split=" + s.split("").length,
  );
}

// --- the code points of each cluster, in order ---
for (let i = 0; i < samples.length; i++) {
  const pts = [...(samples[i][1] as string)].map((c) => (c.codePointAt(0) as any).toString(16));
  console.log("cp-" + samples[i][0] + "=" + pts.join(" "));
}

// --- a cluster is NOT a regex "character": \w, ., and a class see the parts ---
const FAMILY = MAN + ZWJ + WOMAN + ZWJ + BOY;
console.log("family-anchored=" + /^.$/u.test(FAMILY));
console.log("family-three=" + /^...$/u.test(FAMILY));
console.log("family-five=" + /^.....$/u.test(FAMILY));
console.log("family-class=" + new RegExp("^[" + MAN + "]", "u").test(FAMILY));
console.log("zwj-is-not-space=" + /\s/.test(ZWJ));
console.log("zwj-is-not-word=" + /\w/.test(ZWJ));

// --- slicing at a code-point boundary still cuts the cluster in half ---
console.log("family-slice2=" + [...FAMILY.slice(0, 2)].length);
console.log("family-slice2-cp=" + ([...FAMILY.slice(0, 2)][0].codePointAt(0) as any).toString(16));
console.log("family-at0=" + ((FAMILY.at(0) as any).charCodeAt(0)).toString(16));
console.log("family-first-point=" + (([...FAMILY][0].codePointAt(0)) as any).toString(16));
console.log("family-substring=" + [...FAMILY.substring(0, 5)].map((c) => (c.codePointAt(0) as any).toString(16)).join(" "));

// --- the classic bug: reversing by unit, by point, and what survives ---
const flagRev = [...(RI_B + RI_R)].reverse().join("");
console.log("flag-reversed-cp=" + [...flagRev].map((c) => (c.codePointAt(0) as any).toString(16)).join(" "));
console.log("flag-reversed-wf=" + flagRev.isWellFormed());
const unitRev = (RI_B + RI_R).split("").reverse().join("");
console.log("unit-reversed-wf=" + unitRev.isWellFormed());
console.log("unit-reversed-len=" + unitRev.length);
const accentRev = [..."e" + ACUTE].reverse().join("");
console.log("accent-reversed-cp=" + [...accentRev].map((c) => (c.codePointAt(0) as any).toString(16)).join(" "));

// --- normalization composes the combining sequence but never the emoji ones ---
console.log("accent-nfc-points=" + [...("e" + ACUTE).normalize("NFC")].length);
console.log("family-nfc-points=" + [...FAMILY.normalize("NFC")].length);
console.log("keycap-nfkc-points=" + [...("1" + VS16 + KEYCAP).normalize("NFKC")].length);
console.log("heart-nfc-len=" + (HEART + VS16).normalize("NFC").length);

// --- case mapping leaves every one of them alone ---
console.log("family-upper-len=" + FAMILY.toUpperCase().length);
console.log("accent-upper-points=" + [...("e" + ACUTE).toUpperCase()].map((c) => (c.codePointAt(0) as any).toString(16)).join(" "));

// --- indexOf finds a PART of a cluster, which is why naive search is wrong ---
console.log("indexOf-man=" + FAMILY.indexOf(MAN));
console.log("indexOf-boy=" + FAMILY.indexOf(BOY));
console.log("includes-woman=" + FAMILY.includes(WOMAN));
console.log("wave-in-skin=" + (WAVE + SKIN4).includes(WAVE));
console.log("replace-part=" + [...(FAMILY.replace(WOMAN, ""))].length);

// --- padEnd counts units, so a cluster pad is truncated anywhere ---
const padded = "x".padEnd(6, FAMILY);
console.log("pad-len=" + padded.length + " points=" + [...padded].length + " wf=" + padded.isWellFormed());
