// Cross-runtime: the Annex B pair escape/unescape. They predate UTF-8 and work on
// UTF-16 CODE UNITS: escape emits %XX below U+0100 and %uXXXX above it, and
// unescape leaves any malformed escape exactly where it found it.

console.log("types=" + typeof escape + "," + typeof unescape);
console.log("lengths=" + escape.length + "," + unescape.length);
console.log("names=" + escape.name + "," + unescape.name);

// The unreserved set: letters, digits and @ * _ + - . / survive untouched.
const asciiRange = function (): string {
  let s = "";
  for (let i = 0x20; i < 0x7f; i++) s += String.fromCharCode(i);
  return s;
};
const untouched = function (fn: (s: string) => string, src: string): string {
  let kept = "";
  for (const ch of src) {
    if (fn(ch) === ch) kept += ch;
  }
  return kept;
};
console.log("escape_unreserved=" + untouched(escape, asciiRange()));
console.log("escape_ascii=" + escape("abc ABC 019"));
console.log("escape_punct=" + escape("@*_+-./"));
console.log("escape_other_punct=" + escape("!~'()"));
console.log("escape_quotes=" + escape("\"'`"));
console.log("escape_reserved=" + escape(";/?:@&=+$,#"));
console.log("escape_controls=" + escape("\u0000\u000a\u001f"));
console.log("escape_del=" + escape("\u007f") + " high_ascii=" + escape("\u0080\u00ff"));
console.log("escape_boundary=" + escape("\u00ff\u0100"));
console.log("escape_bmp=" + escape("\u20ac\ud7ff\ue000\uffff"));
console.log("escape_astral=" + escape("\u{1f600}"));
console.log("escape_lone_surrogate=" + escape("\ud800") + "," + escape("\udfff"));
console.log("escape_empty=" + JSON.stringify(escape("")));
console.log("escape_coerces=" + escape(123 as any) + "," + escape(null as any) + "," + escape(true as any));
console.log("escape_case=" + escape("\u00ab") + " uses_upper_hex=" + (escape("\u00ab") === escape("\u00ab").toUpperCase()));

// unescape reverses both forms, and ignores anything that does not parse.
const codes = function (s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(",");
};
console.log("unescape_hex=" + codes(unescape("%41%42")));
console.log("unescape_u=" + codes(unescape("%u0041%u20AC")));
console.log("unescape_lowercase_u=" + codes(unescape("%u00e9")) + " uppercase_hex=" + codes(unescape("%E9")));
console.log("unescape_mixed=" + codes(unescape("A%42%u0043D")));
console.log("unescape_incomplete=" + codes(unescape("%4")) + "," + codes(unescape("%")) + "," + codes(unescape("%u")) + "," + codes(unescape("%u00")));
console.log("unescape_bad_hex=" + codes(unescape("%zz")) + "," + codes(unescape("%uzzzz")) + "," + codes(unescape("%4z")));
console.log("unescape_percent_only=" + codes(unescape("100%")) + " trailing=" + codes(unescape("a%")));
console.log("unescape_double=" + codes(unescape("%2541")));
console.log("unescape_plain=" + JSON.stringify(unescape("hello+world")));
console.log("unescape_empty=" + JSON.stringify(unescape("")));
console.log("unescape_coerces=" + JSON.stringify(unescape(123 as any)) + "," + JSON.stringify(unescape(null as any)));
console.log("unescape_never_throws=" + (function (): string {
  try {
    return JSON.stringify(unescape("%u{1f600}%%%")) + "/no-throw";
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
})());

// Round trips, and how the pair differs from encodeURIComponent.
console.log("roundtrip_latin1=" + (unescape(escape("\u00e9 \u00ff")) === "\u00e9 \u00ff"));
console.log("roundtrip_astral=" + (unescape(escape("\u{1f600}")) === "\u{1f600}"));
console.log("roundtrip_all_bmp=" + (function (): string {
  let src = "";
  for (let i = 0; i < 0x300; i++) src += String.fromCharCode(i);
  return String(unescape(escape(src)) === src);
})());
console.log("vs_encodeURIComponent=" + escape("\u00e9") + " vs " + encodeURIComponent("\u00e9"));
console.log("vs_encodeURI_astral=" + escape("\u{1f600}") + " vs " + encodeURIComponent("\u{1f600}"));
console.log("vs_space=" + escape(" ") + " vs " + encodeURIComponent(" "));
console.log("vs_plus=" + escape("+") + " vs " + encodeURIComponent("+"));
console.log("vs_slash=" + escape("/") + " vs " + encodeURIComponent("/"));
console.log("decodeURIComponent_of_escape=" + (function (): string {
  try {
    return decodeURIComponent(escape("\u00e9"));
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
})());
console.log("unescape_of_encodeURIComponent=" + codes(unescape(encodeURIComponent("\u00e9"))));
console.log("escape_is_not_enumerable=" + (function (): string {
  const d: any = Object.getOwnPropertyDescriptor(globalThis, "escape");
  return d ? "w:" + d.writable + " e:" + d.enumerable + " c:" + d.configurable : "<absent>";
})());
