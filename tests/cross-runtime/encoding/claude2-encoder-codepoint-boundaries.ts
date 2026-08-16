// Cross-runtime: where UTF-8 changes width. Each boundary code point is encoded,
// counted and decoded back, so the 1/2/3/4-byte frontiers and the surrogate hole
// are pinned together with what encodeInto reports at each of them.

const enc = new TextEncoder();
const dec = new TextDecoder();

const hex = function (u: Uint8Array): string {
  const out: string[] = [];
  for (let i = 0; i < u.length; i++) out.push(u[i].toString(16).padStart(2, "0"));
  return out.join(" ");
};

// The width boundaries: the last code point of one width and the first of the next.
const boundaries = [0x00, 0x7f, 0x80, 0x7ff, 0x800, 0xd7ff, 0xe000, 0xffff, 0x10000, 0x10ffff];
for (const cp of boundaries) {
  const s = String.fromCodePoint(cp);
  const bytes = enc.encode(s);
  console.log("cp_" + cp.toString(16) + "=units:" + s.length + " bytes:" + bytes.length + " [" + hex(bytes) + "] roundtrip:" + (dec.decode(bytes) === s));
}

// A string mixing all four widths.
const mixed = "A\u00e9\u20ac\u{1f600}";
console.log("mixed_units=" + mixed.length + " bytes=" + enc.encode(mixed).length + " [" + hex(enc.encode(mixed)) + "]");
console.log("mixed_roundtrip=" + (dec.decode(enc.encode(mixed)) === mixed));
console.log("mixed_codepoints=" + Array.from(mixed).length);

// The surrogate hole: a lone half becomes U+FFFD, a pair becomes one code point.
console.log("lone_high=" + hex(enc.encode("\ud83d")));
console.log("lone_low=" + hex(enc.encode("\ude00")));
console.log("reversed_pair=" + hex(enc.encode("\ude00\ud83d")));
console.log("pair=" + hex(enc.encode("\u{1f600}")));
console.log("high_then_ascii=" + hex(enc.encode("\ud83dA")));
console.log("replacement_itself=" + hex(enc.encode("\ufffd")));
console.log("lone_roundtrip=" + (function (): string {
  const back = dec.decode(enc.encode("\ud800"));
  return back.length + "/" + back.charCodeAt(0).toString(16);
})());

// encode() argument coercion, and the shape of what it answers.
console.log("encode_empty=" + enc.encode("").length + " no_arg=" + (enc as any).encode().length);
console.log("encode_undefined=" + hex(enc.encode(undefined)) + " null=" + hex(enc.encode(null as any)) + " number=" + hex(enc.encode(123 as any)));
console.log("encode_kind=" + enc.encode("A").constructor.name + " offset=" + enc.encode("A").byteOffset + " buffer=" + enc.encode("A").buffer.byteLength);
console.log("encode_fresh_each_time=" + (enc.encode("A") !== enc.encode("A")));

// encodeInto stops at the last WHOLE code point that fits.
const into = function (s: string, size: number): string {
  const dest = new Uint8Array(size);
  const r = enc.encodeInto(s, dest);
  return r.read + "/" + r.written + "/[" + hex(dest) + "]";
};
console.log("into_exact_1=" + into("A", 1));
console.log("into_exact_2=" + into("\u00e9", 2));
console.log("into_short_2=" + into("\u00e9", 1));
console.log("into_exact_3=" + into("\u20ac", 3));
console.log("into_short_3=" + into("\u20ac", 2));
console.log("into_exact_4=" + into("\u{1f600}", 4));
console.log("into_ascii_then_astral=" + into("A\u{1f600}", 3));
console.log("into_ascii_then_astral_fits=" + into("A\u{1f600}", 5));
console.log("into_zero_dest=" + into("A", 0));
console.log("into_empty_string=" + into("", 2));
console.log("into_lone_surrogate=" + into("\ud800x", 4));
console.log("into_lone_surrogate_short=" + into("\ud800x", 2));
console.log("into_result_shape=" + (function (): string {
  const r: any = enc.encodeInto("A", new Uint8Array(4));
  return Object.keys(r).sort().join(",") + "/" + (Object.getPrototypeOf(r) === Object.prototype) + "/" + typeof r.read;
})());
console.log("into_leaves_tail=" + (function (): string {
  const dest = new Uint8Array(4).fill(0xaa);
  enc.encodeInto("A", dest);
  return hex(dest);
})());
console.log("into_view_window=" + (function (): string {
  const backing = new Uint8Array(8).fill(0xff);
  const dest = backing.subarray(2, 6);
  const r = enc.encodeInto("abc", dest);
  return r.read + "/" + r.written + "/[" + hex(backing) + "]";
})());
console.log("into_read_is_units=" + (function (): string {
  const r = enc.encodeInto("\u{1f600}", new Uint8Array(4));
  return r.read + "/" + r.written;
})());
console.log("byte_length_matches=" + boundaries.map(function (cp) {
  const s = String.fromCodePoint(cp);
  return String(enc.encode(s).length === enc.encodeInto(s, new Uint8Array(8)).written);
}).join(","));
