// Cross-runtime: TextDecoder over the UTF-16 encodings. Bytes are read in pairs,
// an odd trailing byte becomes U+FFFD (or throws when fatal), a lone surrogate is
// replaced while a well-formed pair survives, and streaming holds a half pair.

const units = function (s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(",");
};

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

const le = new TextDecoder("utf-16le");
const be = new TextDecoder("utf-16be");

console.log("le_encoding=" + le.encoding + " be_encoding=" + be.encoding);
console.log("le_ascii=" + le.decode(new Uint8Array([0x41, 0x00, 0x42, 0x00])));
console.log("be_ascii=" + be.decode(new Uint8Array([0x00, 0x41, 0x00, 0x42])));
console.log("le_of_be_bytes=" + units(le.decode(new Uint8Array([0x00, 0x41]))));
console.log("le_units=" + units(le.decode(new Uint8Array([0xac, 0x20]))));
console.log("be_units=" + units(be.decode(new Uint8Array([0x20, 0xac]))));
console.log("le_empty=" + JSON.stringify(le.decode(new Uint8Array(0))) + " no_arg=" + JSON.stringify(le.decode()));

// An odd number of bytes leaves a dangling half unit.
console.log("odd_tail=" + units(le.decode(new Uint8Array([0x41, 0x00, 0x42]))));
console.log("odd_only=" + units(le.decode(new Uint8Array([0x41]))));
console.log("odd_fatal=" + t(function () { return new TextDecoder("utf-16le", { fatal: true }).decode(new Uint8Array([0x41, 0x00, 0x42])); }));
console.log("even_fatal=" + t(function () { return new TextDecoder("utf-16le", { fatal: true }).decode(new Uint8Array([0x41, 0x00])); }));

// Surrogates: a well-formed pair is kept as two code units, an unpaired one is
// replaced — even under fatal, which UTF-16 treats as a decode ERROR.
console.log("pair=" + units(le.decode(new Uint8Array([0x3d, 0xd8, 0x00, 0xde]))) + " codepoint=" + (le.decode(new Uint8Array([0x3d, 0xd8, 0x00, 0xde])).codePointAt(0) as number).toString(16));
console.log("lone_high=" + units(le.decode(new Uint8Array([0x00, 0xd8, 0x41, 0x00]))));
console.log("lone_low=" + units(le.decode(new Uint8Array([0x00, 0xdc, 0x41, 0x00]))));
console.log("reversed_pair=" + units(le.decode(new Uint8Array([0x00, 0xde, 0x3d, 0xd8]))));
console.log("high_at_end=" + units(le.decode(new Uint8Array([0x41, 0x00, 0x00, 0xd8]))));
console.log("lone_fatal=" + t(function () { return new TextDecoder("utf-16le", { fatal: true }).decode(new Uint8Array([0x00, 0xd8, 0x41, 0x00])); }));
console.log("pair_fatal=" + t(function () { return units(new TextDecoder("utf-16le", { fatal: true }).decode(new Uint8Array([0x3d, 0xd8, 0x00, 0xde]))); }));

// The BOM is consumed at the start unless ignoreBOM asks for it.
console.log("bom_stripped=" + units(new TextDecoder("utf-16le").decode(new Uint8Array([0xff, 0xfe, 0x41, 0x00]))));
console.log("bom_kept=" + units(new TextDecoder("utf-16le", { ignoreBOM: true }).decode(new Uint8Array([0xff, 0xfe, 0x41, 0x00]))));
console.log("be_bom_stripped=" + units(new TextDecoder("utf-16be").decode(new Uint8Array([0xfe, 0xff, 0x00, 0x41]))));
console.log("wrong_bom_for_le=" + units(new TextDecoder("utf-16le").decode(new Uint8Array([0xfe, 0xff, 0x41, 0x00]))));
console.log("bom_mid=" + units(new TextDecoder("utf-16le").decode(new Uint8Array([0x41, 0x00, 0xff, 0xfe, 0x42, 0x00]))));
console.log("bom_only=" + units(new TextDecoder("utf-16le").decode(new Uint8Array([0xff, 0xfe]))));

// Streaming: a half unit is carried across calls, and the final call flushes it.
console.log("stream_split_unit=" + (function (): string {
  const d = new TextDecoder("utf-16le");
  const a = units(d.decode(new Uint8Array([0x41]), { stream: true }));
  const b = units(d.decode(new Uint8Array([0x00]), { stream: true }));
  const c = units(d.decode());
  return JSON.stringify(a) + "|" + b + "|" + JSON.stringify(c);
})());
console.log("stream_split_pair=" + (function (): string {
  const d = new TextDecoder("utf-16le");
  const a = units(d.decode(new Uint8Array([0x3d, 0xd8]), { stream: true }));
  const b = units(d.decode(new Uint8Array([0x00, 0xde]), { stream: true }));
  return JSON.stringify(a) + "|" + b;
})());
console.log("stream_flush_odd=" + (function (): string {
  const d = new TextDecoder("utf-16le");
  const a = units(d.decode(new Uint8Array([0x41, 0x00, 0x42]), { stream: true }));
  const b = units(d.decode());
  return a + "|" + b;
})());
console.log("stream_flush_dangling_high=" + (function (): string {
  const d = new TextDecoder("utf-16le");
  const a = units(d.decode(new Uint8Array([0x3d, 0xd8]), { stream: true }));
  const b = units(d.decode());
  return JSON.stringify(a) + "|" + b;
})());
console.log("decoder_state_resets=" + (function (): string {
  const d = new TextDecoder("utf-16le");
  d.decode(new Uint8Array([0x41]), { stream: true });
  d.decode();
  return units(d.decode(new Uint8Array([0x42, 0x00])));
})());
console.log("props=" + (function (): string {
  const d = new TextDecoder("utf-16be", { fatal: true, ignoreBOM: true });
  return d.encoding + "," + d.fatal + "," + d.ignoreBOM;
})());
