// Cross-runtime: the UTF-8 BOM. A decoder removes ONE leading EF BB BF unless
// ignoreBOM is set; a second BOM, or a BOM anywhere but the start, decodes as an
// ordinary U+FEFF; and the encoder never adds one.

const units = function (s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(",");
};
const hex = function (u: Uint8Array): string {
  const out: string[] = [];
  for (let i = 0; i < u.length; i++) out.push(u[i].toString(16).padStart(2, "0"));
  return out.join(" ");
};
const bytes = function (...v: number[]): Uint8Array {
  return new Uint8Array(v);
};

const plain = new TextDecoder();
const keeping = new TextDecoder("utf-8", { ignoreBOM: true });

console.log("bom_stripped=" + units(plain.decode(bytes(0xef, 0xbb, 0xbf, 0x41))));
console.log("bom_kept=" + units(keeping.decode(bytes(0xef, 0xbb, 0xbf, 0x41))));
console.log("length_difference=" + plain.decode(bytes(0xef, 0xbb, 0xbf, 0x41)).length + "," + keeping.decode(bytes(0xef, 0xbb, 0xbf, 0x41)).length);
console.log("bom_only=" + JSON.stringify(plain.decode(bytes(0xef, 0xbb, 0xbf))) + "," + units(keeping.decode(bytes(0xef, 0xbb, 0xbf))));
console.log("two_boms=" + units(plain.decode(bytes(0xef, 0xbb, 0xbf, 0xef, 0xbb, 0xbf, 0x41))));
console.log("three_boms=" + units(plain.decode(bytes(0xef, 0xbb, 0xbf, 0xef, 0xbb, 0xbf, 0xef, 0xbb, 0xbf))));
console.log("bom_in_middle=" + units(plain.decode(bytes(0x41, 0xef, 0xbb, 0xbf, 0x42))));
console.log("bom_at_end=" + units(plain.decode(bytes(0x41, 0xef, 0xbb, 0xbf))));
console.log("no_bom=" + units(plain.decode(bytes(0x41, 0x42))));
console.log("empty=" + JSON.stringify(plain.decode(bytes())) + "," + JSON.stringify(keeping.decode(bytes())));

// A truncated BOM is not a BOM: it is one or two malformed sequences.
console.log("truncated_two=" + units(plain.decode(bytes(0xef, 0xbb))));
console.log("truncated_one=" + units(plain.decode(bytes(0xef))));
console.log("near_miss=" + units(plain.decode(bytes(0xef, 0xbb, 0xbe, 0x41))));
console.log("truncated_fatal=" + (function (): string {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes(0xef, 0xbb));
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
})());
console.log("bom_under_fatal=" + units(new TextDecoder("utf-8", { fatal: true }).decode(bytes(0xef, 0xbb, 0xbf, 0x41))));
console.log("bom_under_fatal_kept=" + units(new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes(0xef, 0xbb, 0xbf, 0x41))));

// The stripping is per DECODE call, not per decoder, when not streaming.
console.log("second_call=" + (function (): string {
  const d = new TextDecoder();
  const first = units(d.decode(bytes(0xef, 0xbb, 0xbf, 0x41)));
  const second = units(d.decode(bytes(0xef, 0xbb, 0xbf, 0x42)));
  return first + "|" + second;
})());

// Other encodings do not know about the UTF-8 BOM's bytes.
console.log("bom_bytes_as_1252=" + units(new TextDecoder("windows-1252").decode(bytes(0xef, 0xbb, 0xbf))));
console.log("bom_bytes_as_1252_then_a=" + units(new TextDecoder("windows-1252").decode(bytes(0xef, 0xbb, 0xbf, 0x41))));
console.log("utf16_bom_bytes_in_utf8=" + units(plain.decode(bytes(0xff, 0xfe, 0x41, 0x00))));

// The encoder emits U+FEFF as an ordinary code point and never prepends one.
const enc = new TextEncoder();
console.log("encode_plain=" + hex(enc.encode("A")));
console.log("encode_feff=" + hex(enc.encode("\uFEFF")));
console.log("encode_feff_then_a=" + hex(enc.encode("\uFEFFA")));
console.log("roundtrip_loses_bom=" + JSON.stringify(plain.decode(enc.encode("\uFEFFA"))));
console.log("roundtrip_keeps_bom=" + units(keeping.decode(enc.encode("\uFEFFA"))));
console.log("roundtrip_middle=" + units(plain.decode(enc.encode("A\uFEFFB"))));
console.log("encodeInto_feff=" + (function (): string {
  const dest = new Uint8Array(6);
  const r = enc.encodeInto("\uFEFFA", dest);
  return r.read + "/" + r.written + "/" + hex(dest);
})());
console.log("bom_trimmed_as_whitespace=" + JSON.stringify("\uFEFF  x".trim()) + "," + ("\uFEFF".charCodeAt(0)).toString(16));
console.log("decoded_char_codes=" + units(keeping.decode(bytes(0xef, 0xbb, 0xbf, 0x41))) + " length=" + keeping.decode(bytes(0xef, 0xbb, 0xbf, 0x41)).length);
