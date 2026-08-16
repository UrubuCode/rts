// Cross-runtime: `.` excludes exactly FOUR characters — LF, CR, U+2028, U+2029 —
// and `^`/`$` under `m` anchor at exactly those same four, which means CRLF is
// TWO line breaks to a regex and U+0085 (NEL) and U+000B are NOT line breaks at
// all. 170_regexp_flags only checks that `s` and `m` exist. Every character is
// built by fromCharCode so the file itself stays one ASCII line per statement.

const LF = String.fromCharCode(0x0a);
const CR = String.fromCharCode(0x0d);
const LS = String.fromCharCode(0x2028);
const PS = String.fromCharCode(0x2029);
const NEL = String.fromCharCode(0x85);
const VT = String.fromCharCode(0x0b);
const FF = String.fromCharCode(0x0c);
const NBSP = String.fromCharCode(0xa0);

const table: any[][] = [
  ["lf", LF], ["cr", CR], ["ls", LS], ["ps", PS],
  ["nel", NEL], ["vt", VT], ["ff", FF], ["nbsp", NBSP],
  ["tab", "\t"], ["space", " "],
];

// --- `.` against each candidate, with and without the `s` flag ---
for (let i = 0; i < table.length; i++) {
  const name = table[i][0];
  const ch = table[i][1];
  console.log(
    "dot-" + name +
      " plain=" + /^.$/.test(ch) +
      " s=" + /^.$/s.test(ch) +
      " u=" + /^.$/u.test(ch) +
      " su=" + /^.$/su.test(ch),
  );
}

// --- `^` and `$` under `m` split at exactly the same four ---
for (let i = 0; i < table.length; i++) {
  const name = table[i][0];
  const ch = table[i][1];
  console.log(
    "anchor-" + name +
      " parts=" + ("a" + ch + "b").split(/^/m).length +
      " matches=" + JSON.stringify(("a" + ch + "b").match(/^\w$/gm)),
  );
}

// --- without `m`, ^ and $ are the whole-string ends only ---
console.log("no-m-caret=" + ("a" + LF + "b").split(/^/).length);
console.log("no-m-dollar=" + JSON.stringify(("a" + LF + "b").match(/^\w$/g)));
console.log("no-m-whole=" + /^a$/.test("a" + LF));
console.log("no-m-whole-strict=" + /^a$/.test("a"));

// --- CRLF is two terminators, so it produces an EMPTY line between them ---
const crlf = "a" + CR + LF + "b";
console.log("crlf-lines=" + JSON.stringify(crlf.split(/^/m).map((s) => s.length)));
console.log("crlf-empty-line=" + JSON.stringify(crlf.match(/^.*$/gm)));
console.log("crlf-dollar-count=" + (crlf.match(/$/gm) as any).length);
console.log("lf-dollar-count=" + (("a" + LF + "b").match(/$/gm) as any).length);

// --- $ sits BEFORE the terminator and ^ AFTER it: neither consumes ---
const probe: any = /^(b)$/m.exec("a" + LF + "b");
console.log("m-index=" + probe.index + " len=" + probe[0].length);
const probe2: any = /^(a)$/m.exec("a" + LF + "b");
console.log("m-first-index=" + probe2.index);
console.log("m-empty-at-end=" + JSON.stringify(("a" + LF).match(/^$/gm)));

// --- \s DOES include all four, plus NEL is excluded there too ---
for (let i = 0; i < table.length; i++) {
  console.log("s-class-" + table[i][0] + "=" + /^\s$/.test(table[i][1]));
}

// --- [^] is the portable "any character", unlike `.` ---
console.log("negempty-lf=" + /^[^]$/.test(LF));
console.log("negempty-ls=" + /^[^]$/.test(LS));
console.log("dotall-count=" + ("a" + LF + "b").replace(/./gs, "-"));
console.log("dot-count=" + ("a" + LF + "b").replace(/./g, "-").length);

// --- a class containing an explicit terminator matches it in every mode ---
console.log("explicit-lf=" + /^[\n]$/.test(LF));
console.log("explicit-range=" + new RegExp("^[" + LS + "]$").test(LS));
console.log("word-boundary=" + JSON.stringify(("a" + LF + "b").match(/\b\w\b/g)));

// --- the `m` flag does not change `.`, and `s` does not change the anchors ---
console.log("m-dot=" + /^.$/m.test(LF));
console.log("s-anchor=" + ("a" + LF + "b").split(/^/ms).length);
console.log("ms-both=" + JSON.stringify(("a" + LF + "b").match(/^.$/gms)));
