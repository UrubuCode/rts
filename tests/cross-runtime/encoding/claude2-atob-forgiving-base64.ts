// Cross-runtime: atob runs the "forgiving base64" algorithm — ASCII whitespace is
// stripped anywhere, padding is checked after that, and a remainder of one
// character is always an error. btoa pads deterministically by input length.

const codes = function (s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(",");
};

const dec = function (s: any): string {
  try {
    return codes(atob(s));
  } catch (e: any) {
    return "throw:" + e.constructor.name + "/" + e.name;
  }
};

// Padding: two, one or none — all three spell the same bytes as long as the
// remaining length is legal.
console.log("pad2=" + dec("QQ==") + " pad1=" + dec("QUI=") + " pad0=" + dec("QUJD"));
console.log("unpadded_1=" + dec("QQ") + " unpadded_2=" + dec("QUI"));
console.log("wrong_pad_count=" + dec("QQ=") + " " + dec("QUI==") + " " + dec("QUJD="));
console.log("overpadded=" + dec("QQ===") + " " + dec("QQ===="));
console.log("remainder_one=" + dec("Q") + " " + dec("QUJDQ"));
console.log("all_padding=" + dec("====") + " " + dec("="));
console.log("empty=" + JSON.stringify(atob("")) + " length=" + atob("").length);
console.log("pad_in_middle=" + dec("QQ==QQ==") + " " + dec("=QQQ") + " " + dec("QQ=Q"));

// Whitespace: the four ASCII whitespace characters are removed before anything
// else is decided, so they may appear anywhere, padding included.
console.log("ws_around=" + dec(" QQ== "));
console.log("ws_inside=" + dec("Q Q = ="));
console.log("ws_kinds=" + dec("\tQ\nQ\r=\f="));
console.log("ws_only=" + dec("   ") + " length=" + atob("  \t\n").length);
console.log("ws_makes_it_legal=" + dec("Q Q") + " ws_cannot_fix_remainder=" + dec("Q "));
console.log("nbsp_is_not_ws=" + dec("\u00a0QQ==") + " vertical_tab=" + dec("\u000bQQ=="));

// The alphabet is the standard one: + and / decode, - and _ do not.
console.log("plus_slash=" + dec("+/8="));
console.log("url_safe=" + dec("-_8="));
console.log("bad_char=" + dec("Q*==") + " " + dec("Q.A=") + " " + dec("QQ@="));
console.log("non_ascii=" + dec("QQ\u00e9=") + " " + dec("\u{1f600}QQ="));
console.log("full_range=" + dec("/w==") + " " + dec("AA==") + " " + dec("AAAA"));

// The result is a BINARY string: one code unit per byte, never text.
console.log("high_bytes=" + dec("gIH+/w=="));
console.log("nul_byte=" + dec("AEEA") + " length=" + atob("AEEA").length);
console.log("length_by_group=" + atob("QUJD").length + "," + atob("QUI=").length + "," + atob("QQ==").length);

// Argument coercion: ToString first, so a number or an object with toString is
// decoded as its string form.
console.log("coerce_object=" + dec({ toString: function () { return "QQ=="; } }));
console.log("coerce_null=" + dec(null));
console.log("coerce_number=" + dec(1234));
console.log("coerce_array=" + dec(["QQ=="]));
console.log("no_argument=" + (function (): string {
  try {
    return codes((atob as any)());
  } catch (e: any) {
    return "throw:" + e.constructor.name + "/" + e.name;
  }
})());

// btoa is the inverse over the Latin-1 range, and its padding follows length % 3.
const enc = function (s: any): string {
  try {
    return btoa(s);
  } catch (e: any) {
    return "throw:" + e.constructor.name + "/" + e.name;
  }
};
console.log("btoa_lengths=" + enc("A") + "," + enc("AB") + "," + enc("ABC") + "," + enc("ABCD"));
console.log("btoa_empty=" + JSON.stringify(btoa("")));
console.log("btoa_high=" + enc("\u00ff\u00fe") + " nul=" + enc("\u0000\u0000"));
console.log("btoa_above_latin1=" + enc("\u0100") + " " + enc("\u20ac") + " " + enc("A\u0100"));
console.log("btoa_lone_surrogate=" + enc("\ud800"));
console.log("btoa_coerces=" + enc(123) + "," + enc(null) + "," + enc(true) + "," + enc({ toString: function () { return "A"; } }));
console.log("roundtrip=" + (function (): string {
  let src = "";
  for (let i = 0; i < 256; i++) src += String.fromCharCode(i);
  const back = atob(btoa(src));
  return (back === src) + "/" + back.length + "/" + btoa(src).length;
})());
console.log("error_is_domexception=" + (function (): string {
  try {
    atob("Q");
    return "no-throw";
  } catch (e: any) {
    return (e instanceof DOMException) + "/" + (e instanceof Error) + "/" + e.name + "/" + e.code;
  }
})());
