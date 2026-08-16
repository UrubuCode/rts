// Cross-runtime: the K forms throw away formatting distinctions the canonical
// forms keep — superscripts, fractions, circled letters, fullwidth/halfwidth,
// no-break space — and the ARGUMENT is validated against exactly four strings
// with a RangeError for anything else (including a lowercase "nfc"), while
// undefined means NFC and null is coerced to the string "null" and rejected.
// 194/claude-normalize-forms use only a couple of K samples and never the errors.

function cp(s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(" ");
}

function four(label: string, s: string): void {
  console.log(
    label +
      " raw=" + cp(s) +
      " NFC=" + cp(s.normalize("NFC")) +
      " NFD=" + cp(s.normalize("NFD")) +
      " NFKC=" + cp(s.normalize("NFKC")) +
      " NFKD=" + cp(s.normalize("NFKD")),
  );
}

// --- superscripts and subscripts lose their position under K ---
four("super-2", String.fromCharCode(0xb2));
four("super-n", String.fromCharCode(0x207f));
four("sub-1", String.fromCharCode(0x2081));

// --- fractions expand to three characters ---
four("half", String.fromCharCode(0xbd));
four("quarter", String.fromCharCode(0xbc));
console.log("half-nfkc-len=" + String.fromCharCode(0xbd).normalize("NFKC").length);

// --- circled and parenthesised forms ---
four("circled-1", String.fromCharCode(0x2460));
four("circled-A", String.fromCharCode(0x24b6));
four("paren-a", String.fromCharCode(0x249c));

// --- fullwidth and halfwidth ---
four("fullwidth-A", String.fromCharCode(0xff21));
four("fullwidth-digit", String.fromCharCode(0xff10));
four("halfwidth-ka", String.fromCharCode(0xff76));

// --- halfwidth katakana + halfwidth voiced mark composes into one char ---
const HW_KA = String.fromCharCode(0xff76);
const HW_VOICED = String.fromCharCode(0xff9e);
four("hw-ga", HW_KA + HW_VOICED);
console.log("hw-ga-nfkc-len=" + (HW_KA + HW_VOICED).normalize("NFKC").length);
console.log("hw-ga-nfkd-len=" + (HW_KA + HW_VOICED).normalize("NFKD").length);
console.log("hw-ga-is-GA=" + ((HW_KA + HW_VOICED).normalize("NFKC") === String.fromCharCode(0x30ac)));

// --- spaces: NBSP and the Zs block become an ordinary space under K ---
four("nbsp", String.fromCharCode(0xa0));
four("en-space", String.fromCharCode(0x2002));
four("narrow-nbsp", String.fromCharCode(0x202f));
console.log("nbsp-nfkc-is-space=" + (String.fromCharCode(0xa0).normalize("NFKC") === " "));
console.log("nbsp-nfc-unchanged=" + (String.fromCharCode(0xa0).normalize("NFC") === String.fromCharCode(0xa0)));

// --- ligatures and the long s ---
four("fi-lig", String.fromCharCode(0xfb01));
four("long-s", String.fromCharCode(0x17f));

// --- a few that K leaves ALONE, so the difference is real and not blanket ---
four("emoji-digit", "1");
four("eacute", String.fromCharCode(0xe9));
four("cjk", String.fromCharCode(0x4e2d));
four("zwj", String.fromCharCode(0x200d));
four("bom", String.fromCharCode(0xfeff));

// --- K is lossy: NFC of NFKC does not get you back ---
const SUPER2 = String.fromCharCode(0xb2);
console.log("k-lossy=" + (SUPER2.normalize("NFKC").normalize("NFC") === SUPER2));
console.log("k-idempotent=" + (SUPER2.normalize("NFKC").normalize("NFKC") === SUPER2.normalize("NFKC")));
console.log("nfkd-then-nfkc=" + cp(SUPER2.normalize("NFKD").normalize("NFKC")));

// --- the argument table ---
function norm(arg: any): string {
  try {
    return "ok:" + cp(String.fromCharCode(0xa0).normalize(arg));
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}
console.log("arg-NFC=" + norm("NFC"));
console.log("arg-NFD=" + norm("NFD"));
console.log("arg-NFKC=" + norm("NFKC"));
console.log("arg-NFKD=" + norm("NFKD"));
console.log("arg-none=" + (function () { try { return "ok:" + cp(String.fromCharCode(0xa0).normalize()); } catch (e: any) { return "!" + e.constructor.name; } })());
console.log("arg-undefined=" + norm(undefined));
console.log("arg-lower=" + norm("nfc"));
console.log("arg-mixed=" + norm("Nfc"));
console.log("arg-pad=" + norm(" NFC"));
console.log("arg-empty=" + norm(""));
console.log("arg-null=" + norm(null));
console.log("arg-number=" + norm(1));
console.log("arg-object=" + norm({ toString: () => "NFD" }));
console.log("arg-symbol=" + norm(Symbol("NFC")));

// --- the default really is NFC, not "leave alone" ---
const SEQ = "A" + String.fromCharCode(0x30a);
console.log("default-is-nfc=" + (SEQ.normalize() === SEQ.normalize("NFC")));
console.log("default-len=" + SEQ.length + "->" + SEQ.normalize().length);

// --- normalize is generic over its receiver ---
console.log("on-number=" + String.prototype.normalize.call(12 as any, "NFC"));
console.log("on-boxed=" + String.prototype.normalize.call(new String("a") as any, "NFC"));
console.log("on-null=" + (function () { try { return String.prototype.normalize.call(null as any, "NFC"); } catch (e: any) { return "!" + e.constructor.name; } })());
