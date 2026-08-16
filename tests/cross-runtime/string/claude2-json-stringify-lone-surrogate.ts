// Cross-runtime: ES2019 made JSON.stringify WELL-FORMED — a lone surrogate is
// emitted as a \uXXXX escape instead of being copied through, so the output is
// always valid UTF-16 and always re-parseable. A valid PAIR is never escaped.
// claude-encodeuri-lone-surrogate covers the URI functions; nothing covers what
// JSON does, nor how it differs from the control-character escapes.

const LEAD = String.fromCharCode(0xd83d);
const TRAIL = String.fromCharCode(0xde00);

function codes(s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(",");
}

function show(label: string, s: string): void {
  const j = JSON.stringify(s);
  console.log(
    label +
      " len=" + s.length +
      " wf=" + s.isWellFormed() +
      " json=" + j +
      " jsonLen=" + j.length +
      " jsonWf=" + j.isWellFormed(),
  );
}

// --- a valid pair passes through as the character itself, unescaped ---
show("pair", LEAD + TRAIL);
show("pair-in-text", "a" + LEAD + TRAIL + "b");
show("two-pairs", LEAD + TRAIL + LEAD + TRAIL);
console.log("pair-json-codes=" + codes(JSON.stringify(LEAD + TRAIL)));

// --- a lone surrogate is escaped, and the escape is LOWERCASE hex ---
show("lone-lead", LEAD);
show("lone-trail", TRAIL);
show("lead-then-ascii", LEAD + "a");
show("ascii-then-trail", "a" + TRAIL);
show("reversed", TRAIL + LEAD);
show("d800", String.fromCharCode(0xd800));
show("dbff", String.fromCharCode(0xdbff));
show("dc00", String.fromCharCode(0xdc00));
show("dfff", String.fromCharCode(0xdfff));
console.log("case-of-hex=" + JSON.stringify(String.fromCharCode(0xdbff)));
console.log("adjacent-d7ff=" + JSON.stringify(String.fromCharCode(0xd7ff)));
console.log("adjacent-e000=" + JSON.stringify(String.fromCharCode(0xe000)));

// --- the round trip: parse of the escaped form gives back the same units ---
const broken = "a" + LEAD + "b" + TRAIL + "c";
const parsed = JSON.parse(JSON.stringify(broken));
console.log("roundtrip-eq=" + (parsed === broken));
console.log("roundtrip-codes=" + codes(parsed));
console.log("roundtrip-wf=" + parsed.isWellFormed());

// --- the escaping is the same inside keys, arrays and nested values ---
const keyed: any = {};
keyed[LEAD] = 1;
console.log("key=" + JSON.stringify(keyed));
console.log("array=" + JSON.stringify([LEAD, LEAD + TRAIL]));
console.log("nested=" + JSON.stringify({ a: { b: [TRAIL] } }));
console.log("key-roundtrip=" + Object.keys(JSON.parse(JSON.stringify(keyed)))[0].charCodeAt(0).toString(16));

// --- contrast with the CONTROL character escapes, which use the short forms ---
console.log("controls=" + JSON.stringify("\b\f\n\r\t"));
console.log("nul=" + JSON.stringify(String.fromCharCode(0)));
console.log("ctrl-1f=" + JSON.stringify(String.fromCharCode(0x1f)));
console.log("ctrl-7f=" + JSON.stringify(String.fromCharCode(0x7f)));
console.log("quote=" + JSON.stringify('"'));
console.log("backslash=" + JSON.stringify("\\"));
console.log("slash=" + JSON.stringify("/"));
console.log("ls-2028=" + JSON.stringify(String.fromCharCode(0x2028)));
console.log("ps-2029=" + JSON.stringify(String.fromCharCode(0x2029)));
console.log("nbsp=" + codes(JSON.stringify(String.fromCharCode(0xa0))));
console.log("bom=" + codes(JSON.stringify(String.fromCharCode(0xfeff))));

// --- toWellFormed first gives a DIFFERENT, lossy result ---
console.log("wf-first=" + JSON.stringify(broken.toWellFormed()));
console.log("wf-first-codes=" + codes(broken.toWellFormed()));
console.log("wf-eq-raw=" + (JSON.stringify(broken.toWellFormed()) === JSON.stringify(broken)));

// --- the same string through the other string-producing globals ---
function attempt(f: () => any): string {
  try {
    return JSON.stringify(f());
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}
console.log("encodeURIComponent-lone=" + attempt(() => encodeURIComponent(LEAD)));
console.log("encodeURIComponent-pair=" + attempt(() => encodeURIComponent(LEAD + TRAIL)));
console.log("encodeURI-lone=" + attempt(() => encodeURI(LEAD)));
console.log("escape-lone=" + attempt(() => (globalThis as any).escape(LEAD)));
console.log("template-lone=" + codes(`${LEAD}`));
console.log("concat-lone=" + codes("" + LEAD));

// --- JSON.parse accepts an escaped lone surrogate and produces one ---
const fromText: any = JSON.parse('"a\\ud800b"');
console.log("parse-lone-len=" + fromText.length + " wf=" + fromText.isWellFormed());
console.log("parse-lone-codes=" + codes(fromText));
console.log("parse-pair-len=" + JSON.parse('"\\ud83d\\ude00"').length);
console.log("parse-pair-cp=" + (JSON.parse('"\\ud83d\\ude00"').codePointAt(0) as any).toString(16));

// --- stringify with a replacer and indent keeps the same escaping ---
console.log("indent=" + JSON.stringify({ a: LEAD }, null, 1).split("\n").join("|"));
console.log("replacer=" + JSON.stringify({ a: LEAD }, (k, v) => v));
