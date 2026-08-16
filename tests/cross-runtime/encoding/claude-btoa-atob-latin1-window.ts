// Cross-runtime: btoa/atob work on a BINARY STRING, not on text. btoa refuses
// any code unit above U+00FF, atob answers one char per byte, and both raise a
// DOMException named InvalidCharacterError rather than a plain TypeError.

console.log("btoa_empty=" + JSON.stringify(btoa("")));
console.log("btoa_1=" + btoa("a"));
console.log("btoa_2=" + btoa("ab"));
console.log("btoa_3=" + btoa("abc"));
console.log("btoa_4=" + btoa("abcd"));
console.log("btoa_nul=" + btoa("\u0000"));
console.log("btoa_high=" + btoa("\u00ff\u00fe"));
console.log("btoa_boundary=" + btoa("\u00ff"));
console.log("btoa_all_bits=" + btoa("\u0000\u0001\u00fe\u00ff"));
console.log("btoa_coerces=" + btoa(123 as any) + "," + btoa(null as any) + "," + btoa(true as any));

// One code unit above the Latin-1 window is enough to refuse the whole call.
const overRange: string[] = ["Ā", "€", "aĀ", "\u{1F600}", "\uD800"];
for (const s of overRange) {
  try {
    btoa(s);
    console.log("btoa_over[" + s.length + ":" + (s.codePointAt(0) as number).toString(16) + "]=no-throw");
  } catch (e: any) {
    console.log("btoa_over[" + s.length + ":" + (s.codePointAt(0) as number).toString(16) + "]=" + e.constructor.name + "/" + e.name);
  }
}

console.log("atob_padded=" + atob("YWJj") + "|" + atob("YWI=") + "|" + atob("YQ=="));
console.log("atob_unpadded=" + atob("YWJj") + "|" + atob("YWI") + "|" + atob("YQ"));
console.log("atob_empty=" + JSON.stringify(atob("")));
console.log("atob_high_byte=" + atob("/w==").charCodeAt(0) + " len=" + atob("/w==").length);
console.log("atob_nul=" + atob("AA==").charCodeAt(0));
console.log("atob_all=" + (function (): string {
  const s = atob("AAH+/w==");
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(",");
})());

// Whitespace anywhere is stripped before decoding.
console.log("atob_spaces=" + atob("  Y W\tJ\nj\r  "));
console.log("atob_ws_in_padding=" + atob("YQ = ="));
console.log("atob_ws_only=" + JSON.stringify(atob("   ")));

// The 62nd and 63rd characters are + and /, and the alphabet is strict.
console.log("atob_plus_slash=" + (function (): string {
  const s = atob("+/8=");
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(",");
})());
const badInput: string[] = ["a", "abcde", "YWJ=j", "====", "YQ===", "-_", "YQ=", "@@@@", "YWJj="];
for (const s of badInput) {
  try {
    console.log("atob[" + s + "]=len:" + atob(s).length);
  } catch (e: any) {
    console.log("atob[" + s + "]=" + e.constructor.name + "/" + e.name);
  }
}
try {
  atob("é");
  console.log("atob_latin1=no-throw");
} catch (e: any) {
  console.log("atob_latin1=" + e.name);
}

// Round trip: every byte 0..255 survives btoa(atob(x)) and atob(btoa(x)).
let binary = "";
for (let i = 0; i < 256; i++) binary += String.fromCharCode(i);
const encoded = btoa(binary);
console.log("roundtrip_len=" + encoded.length);
console.log("roundtrip_equal=" + (atob(encoded) === binary));
console.log("roundtrip_head=" + encoded.slice(0, 8) + " tail=" + encoded.slice(-8));

// Bytes go through the Latin-1 window, so text must be encoded first.
const enc = new TextEncoder();
const bytes = enc.encode("héllo €");
let asBinary = "";
for (let i = 0; i < bytes.length; i++) asBinary += String.fromCharCode(bytes[i]);
const b64 = btoa(asBinary);
console.log("utf8_b64=" + b64);
const back = atob(b64);
const rebuilt = new Uint8Array(back.length);
for (let i = 0; i < back.length; i++) rebuilt[i] = back.charCodeAt(i);
console.log("utf8_roundtrip=" + new TextDecoder().decode(rebuilt));

// The functions live on globalThis and are ordinary writable properties.
const d = Object.getOwnPropertyDescriptor(globalThis, "btoa") as any;
console.log("btoa_descriptor=w:" + d.writable + " e:" + d.enumerable + " c:" + d.configurable);
console.log("lengths=" + btoa.length + "," + atob.length);
console.log("names=" + btoa.name + "," + atob.name);
try {
  (btoa as any)();
  console.log("btoa_no_arg=no-throw");
} catch (e: any) {
  console.log("btoa_no_arg=" + e.constructor.name);
}
