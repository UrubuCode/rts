// Cross-runtime: <, >, and the default Array.prototype.sort compare strings by
// UTF-16 CODE UNIT, never by code point. So U+FF3A (a BMP char above the lead
// surrogate range) sorts AFTER every astral character, and the default sort of a
// mixed array is not code-point order. claude-string-relational-compare covers
// only ASCII; nothing pins the surrogate inversion.

const FF3A = "Ｚ";       // FULLWIDTH LATIN CAPITAL LETTER Z, one unit
const GRIN = "\u{1F600}";     // U+1F600, two units: D83D DE00
const LEAD = "\uD83D";        // lone lead surrogate
const TRAIL = "\uDE00";       // lone trail surrogate

function units(s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join("+");
}

// --- the raw units involved ---
console.log("ff3a-units=" + units(FF3A));
console.log("grin-units=" + units(GRIN));
console.log("ff3a-cp=" + (FF3A.codePointAt(0) as number).toString(16));
console.log("grin-cp=" + (GRIN.codePointAt(0) as number).toString(16));

// --- code point says FF3A < 1F600, code unit says the opposite ---
console.log("cp-order=" + ((FF3A.codePointAt(0) as number) < (GRIN.codePointAt(0) as number)));
console.log("unit-order=" + (FF3A < GRIN));
console.log("unit-order-gt=" + (FF3A > GRIN));

// --- a lone surrogate compares by its own unit ---
console.log("lead-vs-ff3a=" + (LEAD < FF3A));
console.log("trail-vs-ff3a=" + (TRAIL < FF3A));
console.log("lead-vs-trail=" + (LEAD < TRAIL));
console.log("grin-vs-lead=" + (GRIN > LEAD));
console.log("grin-eq-pair=" + (GRIN === LEAD + TRAIL));

// --- default sort keeps that inversion ---
const mixed = [GRIN, FF3A, "z", "A", "é", "�"];
console.log("sorted-units=" + mixed.slice().sort().map(units).join(" "));
console.log("sorted-len=" + mixed.slice().sort().map((x) => x.length).join(","));

// --- prefix rules: shorter wins only when it IS a prefix ---
console.log("prefix=" + ("ab" < "abc"));
console.log("empty=" + ("" < "a"));
console.log("empty-eq=" + ("" < ""));
console.log("case=" + ("Z" < "a"));
console.log("digit=" + ("9" < "A"));

// --- comparison is on the raw units, so normalization is not applied ---
const composed = "é";       // é
const decomposed = "é";    // e + combining acute
console.log("nfc-nfd-eq=" + (composed === decomposed));
console.log("nfc-nfd-lt=" + (composed < decomposed));
console.log("nfc-nfd-norm-eq=" + (composed.normalize("NFC") === decomposed.normalize("NFC")));

// --- indexOf/lastIndexOf work on units too, so half a pair is findable ---
const text = "a" + GRIN + "b";
console.log("indexof-lead=" + text.indexOf(LEAD));
console.log("indexof-trail=" + text.indexOf(TRAIL));
console.log("indexof-grin=" + text.indexOf(GRIN));
console.log("slice-half=" + units(text.slice(1, 2)));
console.log("charAt-half=" + units(text.charAt(2)));

// --- includes/startsWith/endsWith also split pairs ---
console.log("startsWith-half=" + (GRIN + "x").startsWith(LEAD));
console.log("endsWith-half=" + ("x" + GRIN).endsWith(TRAIL));
console.log("includes-half=" + text.includes(TRAIL));

// --- spreading iterates code POINTS, so it disagrees with .length ---
console.log("len-vs-spread=" + text.length + ":" + [...text].length);
console.log("spread-units=" + [...text].map(units).join(" "));

// --- and sorting code points is not the same as sorting the string ---
const chars = [...(FF3A + GRIN)];
console.log("spread-sorted=" + chars.slice().sort().map(units).join(" "));
console.log("cp-sorted=" + chars.slice().sort((a, b) =>
  (a.codePointAt(0) as number) - (b.codePointAt(0) as number)).map(units).join(" "));
