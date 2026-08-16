// Cross-runtime: a TextDecoder LABEL is normalised — trimmed, lowercased and
// mapped onto a canonical encoding name — so several spellings answer one
// `.encoding`, and a label with no mapping is a RangeError from the constructor.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

const labelOf = function (label: any): string {
  return t(function () { return new TextDecoder(label).encoding; });
};

// The UTF-8 family: three historical spellings, one canonical name.
for (const label of ["utf-8", "UTF-8", "utf8", "UTF8", "unicode-1-1-utf-8", "unicode11utf8", "unicode20utf8", "x-unicode20utf8"]) {
  console.log("utf8 " + label + "=" + labelOf(label));
}

// The single-byte family: latin1 and ascii are ALIASES of windows-1252, which is
// the substitution the Encoding Standard makes deliberately.
for (const label of ["windows-1252", "latin1", "iso-8859-1", "iso8859-1", "ascii", "us-ascii", "cp1252", "ansi_x3.4-1968"]) {
  console.log("single " + label + "=" + labelOf(label));
}

// The UTF-16 pair, and a few legacy multi-byte ones that both runtimes carry.
for (const label of ["utf-16le", "utf-16", "ucs-2", "unicode", "utf-16be", "big5", "gbk", "gb18030", "shift_jis", "sjis", "euc-jp", "euc-kr", "iso-2022-jp"]) {
  console.log("other " + label + "=" + labelOf(label));
}

// Normalisation: ASCII whitespace around the label is stripped and the case is
// folded, but nothing INSIDE the label is touched.
console.log("trim_spaces=" + labelOf("  utf-8  "));
console.log("trim_tabs=" + labelOf("\tutf-8\n"));
console.log("trim_all_ws=" + labelOf("\n\r\t\f utf-8 \f\t\r\n"));
console.log("mixed_case=" + labelOf("UtF-8"));
console.log("inner_space=" + labelOf("utf 8"));
console.log("inner_dash_missing=" + labelOf("utf16le"));
console.log("nbsp_not_trimmed=" + labelOf("\u00a0utf-8"));
console.log("leading_space=" + labelOf(" utf-8"));
console.log("trailing_space=" + labelOf("utf-8 "));

// No label at all, versus an empty one.
console.log("no_argument=" + new TextDecoder().encoding);
console.log("undefined_label=" + labelOf(undefined));
console.log("empty_label=" + labelOf(""));
console.log("whitespace_only=" + labelOf("   "));

// Unmapped labels are refused by the CONSTRUCTOR, not at decode time.
console.log("bogus=" + labelOf("utf-9"));
console.log("almost=" + labelOf("utf-8x"));
console.log("bom_label=" + labelOf("utf-8-bom"));
console.log("nonstandard=" + labelOf("utf-32") + "," + labelOf("utf-32le") + "," + labelOf("ucs-4"));

// The options bag: only the two flags are read, and they coerce as booleans.
console.log("flags_default=" + (function (): string { const d = new TextDecoder("utf-8"); return d.fatal + "," + d.ignoreBOM; })());
console.log("flags_set=" + (function (): string { const d = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }); return d.fatal + "," + d.ignoreBOM; })());
console.log("flags_empty_bag=" + t(function () { const d = new TextDecoder("utf-8", {} as any); return d.fatal + "," + d.ignoreBOM; }));
console.log("flags_extra_key=" + t(function () { const d = new TextDecoder("utf-8", { nope: 1 } as any); return d.fatal + "," + d.ignoreBOM; }));
console.log("flags_are_getters=" + (function (): string {
  const d: any = Object.getOwnPropertyDescriptor(TextDecoder.prototype, "fatal");
  const e: any = Object.getOwnPropertyDescriptor(TextDecoder.prototype, "encoding");
  return typeof d.get + "," + typeof e.get + "," + String(Object.getOwnPropertyDescriptor(new TextDecoder(), "encoding"));
})());

// TextEncoder has exactly one encoding and takes no label.
console.log("encoder_encoding=" + new TextEncoder().encoding);
console.log("encoder_ignores_label=" + t(function () { return new (TextEncoder as any)("utf-16le").encoding; }));
console.log("decode_sources=" + t(function () { return new TextDecoder().decode(new Uint8Array([65]).buffer); }) + "," + t(function () { return new TextDecoder().decode(new DataView(new Uint8Array([66]).buffer)); }) + "," + t(function () { return new TextDecoder().decode([65] as any); }));
console.log("tags=" + Object.prototype.toString.call(new TextDecoder()) + "," + Object.prototype.toString.call(new TextEncoder()));
console.log("no_new=" + t(function () { return (TextDecoder as any)("utf-8"); }));
