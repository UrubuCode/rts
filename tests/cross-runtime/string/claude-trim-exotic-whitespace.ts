// Cross-runtime: trimStart/trimEnd/trim over the FULL WhiteSpace + LineTerminator
// set — including NBSP, BOM, U+2028/U+2029, VT, FF, and the Unicode Zs block.
// Notably: U+FEFF (BOM) and U+00A0 (NBSP) ARE trimmed; U+200B (ZWSP) is NOT.
// Every char is built via fromCharCode so the file itself stays plain ASCII.

function codes(s: string): string {
  return s.split("").map((c) => c.charCodeAt(0).toString(16)).join(",");
}

// --- each whitespace char, one at a time, wrapped around 'x' ---
const ws: any[][] = [
  ["tab", 0x09],
  ["lf", 0x0a],
  ["vt", 0x0b],
  ["ff", 0x0c],
  ["cr", 0x0d],
  ["space", 0x20],
  ["nbsp", 0x00a0],
  ["ogham", 0x1680],
  ["enquad", 0x2000],
  ["emspace", 0x2003],
  ["thin", 0x2009],
  ["hair", 0x200a],
  ["ls", 0x2028],
  ["ps", 0x2029],
  ["narrownb", 0x202f],
  ["mathsp", 0x205f],
  ["ideo", 0x3000],
  ["bom", 0xfeff],
];
for (const pair of ws) {
  const name = pair[0];
  const ch = String.fromCharCode(pair[1]);
  console.log("trim-" + name + "=" + codes((ch + "x" + ch).trim()));
}

// --- chars that look like whitespace but are NOT trimmed ---
const notWs: any[][] = [
  ["zwsp", 0x200b],
  ["zwnj", 0x200c],
  ["zwj", 0x200d],
  ["nul", 0x00],
  ["nextline", 0x85],
];
for (const pair of notWs) {
  const name = pair[0];
  const ch = String.fromCharCode(pair[1]);
  console.log("notrim-" + name + "=" + codes((ch + "x" + ch).trim()));
}

// --- trimStart vs trimEnd asymmetry with exotic chars ---
const exotic = String.fromCharCode(0x00a0) + String.fromCharCode(0xfeff) + String.fromCharCode(0x2028) + "x" + String.fromCharCode(0x2029) + String.fromCharCode(0x3000);
console.log("exotic-raw=" + codes(exotic));
console.log("exotic-trimStart=" + codes(exotic.trimStart()));
console.log("exotic-trimEnd=" + codes(exotic.trimEnd()));
console.log("exotic-trim=" + codes(exotic.trim()));

// --- a long mixed run collapses entirely ---
const mixed = String.fromCharCode(0x09) + String.fromCharCode(0x0a) + String.fromCharCode(0x20) + String.fromCharCode(0x00a0) + String.fromCharCode(0xfeff) + String.fromCharCode(0x2003) + String.fromCharCode(0x2028) + String.fromCharCode(0x3000);
console.log("all-ws-trim-len=" + mixed.trim().length);
console.log("all-ws-trimStart-len=" + mixed.trimStart().length);
console.log("all-ws-trimEnd-len=" + mixed.trimEnd().length);

// --- interior whitespace is never touched ---
console.log("interior=" + codes(" a" + String.fromCharCode(0x00a0) + "b "));
console.log("interior-trim=" + codes((" a" + String.fromCharCode(0x00a0) + "b ").trim()));

// --- ZWSP blocks trimming of the whitespace behind it ---
console.log("zwsp-blocks=" + codes((" " + String.fromCharCode(0x200b) + " x").trimStart()));

// --- empty and whitespace-only strings ---
console.log("empty-trim-len=" + "".trim().length);
console.log("nbsp-only-trim-len=" + (String.fromCharCode(0x00a0) + String.fromCharCode(0x00a0)).trim().length);
console.log("bom-only-trim-len=" + String.fromCharCode(0xfeff).trim().length);

// --- trim does not mutate the receiver ---
const src = " x ";
src.trim();
console.log("no-mutate-len=" + src.length);
