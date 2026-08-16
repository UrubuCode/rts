// Cross-runtime: in a TAGGED template an illegal escape sequence is not a
// syntax error — the cooked value of that segment becomes `undefined` while the
// raw text is preserved verbatim (ES2018). Only the offending segment is lost.

function show(strings: TemplateStringsArray, ...values: any[]): string {
  const cooked = strings.map((s) => (s === undefined ? "<undef>" : JSON.stringify(s))).join(" ");
  const raw = strings.raw.map((s) => JSON.stringify(s)).join(" ");
  return "cooked[" + cooked + "] raw[" + raw + "] vals[" + values.join(",") + "]";
}

// A legal escape cooks normally.
console.log("legal_newline=" + show`a\nb`);
console.log("legal_tab=" + show`a\tb`);
console.log("legal_cr=" + show`a\rb`);
console.log("legal_codepoint=" + show`a\u{42}b`);
console.log("legal_hex=" + show`a\x41b`);
console.log("legal_backslash=" + show`a\\b`);

// `\unicode` is not a valid \u escape: cooked undefined, raw preserved.
console.log("bad_u_word=" + show`\unicode`);
console.log("bad_u_short=" + show`\u12`);
console.log("bad_u_brace_empty=" + show`\u{}`);
console.log("bad_u_brace_bad=" + show`\u{zz}`);

// `\x` needs two hex digits.
console.log("bad_x=" + show`\xZZ`);
console.log("bad_x_short=" + show`\x1`);

// A legacy octal escape is illegal in a template.
console.log("bad_octal=" + show`\01`);
console.log("bad_octal_8=" + show`\08`);

// Only the segment carrying the bad escape is undefined.
console.log("one_bad_segment=" + show`ok\t${1}\unicode${2}fine\n`);
console.log("first_bad=" + show`\xZZ${1}good`);
console.log("last_bad=" + show`good${1}\u{`);

// The raw text is exactly what was written, backslash included.
function rawOnly(strings: TemplateStringsArray): string {
  return strings.raw.join("|");
}
console.log("raw_word=" + rawOnly`\unicode`);
console.log("raw_octal=" + rawOnly`\01`);
console.log("raw_len=" + rawOnly`\unicode`.length);

// `String.raw` works over the same shapes.
console.log("string_raw_bad=" + String.raw`\unicode\xZZ`);
console.log("string_raw_octal=" + String.raw`\01\02`);

// The cooked array still has the right length; only entries are undefined.
function shapeOf(strings: TemplateStringsArray, ...values: any[]): string {
  return strings.length + "/" + strings.raw.length + "/" + values.length +
    " undef=" + strings.map((s) => (s === undefined ? "1" : "0")).join("");
}
console.log("shape_all_good=" + shapeOf`a${1}b${2}c`);
console.log("shape_middle_bad=" + shapeOf`a${1}\unicode${2}c`);
console.log("shape_all_bad=" + shapeOf`\u{${1}\xZ${2}\01`);

// The template object is still frozen and still cached per call site.
const seen: any[] = [];
function grab(strings: TemplateStringsArray): any { return strings; }
for (let i = 0; i < 2; i++) seen.push(grab`\unicode`);
console.log("still_cached=" + (seen[0] === seen[1]));
console.log("still_frozen=" + Object.isFrozen(seen[0]));
console.log("cooked_is_undefined=" + (seen[0][0] === undefined));
console.log("raw_survives=" + JSON.stringify(seen[0].raw[0]));

// An untagged template with the SAME text would be a syntax error, so a legal
// untagged one is used to show the contrast in cooked output.
console.log("untagged_legal=" + JSON.stringify(`a\nb`));
console.log("untagged_hex=" + JSON.stringify(`a\x41b`));

// A tag that reads only `raw` never sees the undefined at all.
function joinRaw(strings: TemplateStringsArray, ...values: any[]): string {
  let out = strings.raw[0];
  for (let i = 0; i < values.length; i++) out += String(values[i]) + strings.raw[i + 1];
  return out;
}
console.log("join_raw=" + joinRaw`pre\unicode${42}post\01`);

// Mixing a valid and an invalid escape inside ONE segment.
console.log("mixed_segment=" + show`\n\unicode\t`);
console.log("mixed_valid_first=" + show`A\u{zz}`);
