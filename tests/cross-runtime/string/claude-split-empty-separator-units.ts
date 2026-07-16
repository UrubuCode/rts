// Cross-runtime: split("") splits by UTF-16 code UNIT, not by code point.
// Surrogate pairs are torn apart into lone halves — unlike [...str] / for..of.

const emoji = "a\u{1F600}b"; // 'a', surrogate hi, surrogate lo, 'b'

// --- length by unit vs by code point ---
console.log("split-len=" + emoji.split("").length);
console.log("spread-len=" + [...emoji].length);
console.log("str-len=" + emoji.length);

// --- each unit's code, in hex ---
const units = emoji.split("");
console.log("units=" + units.map((c) => c.charCodeAt(0).toString(16)).join(","));

// --- each split piece has length 1 (a single code unit) ---
console.log("piece-lens=" + units.map((c) => c.length).join(","));

// --- rejoining by unit round-trips; the halves are individually lone ---
console.log("rejoin-eq=" + (units.join("") === emoji));
console.log("half-hi-isLone=" + (units[1].charCodeAt(0) >= 0xd800 && units[1].charCodeAt(0) <= 0xdbff));
console.log("half-lo-isLone=" + (units[2].charCodeAt(0) >= 0xdc00 && units[2].charCodeAt(0) <= 0xdfff));

// --- codePointAt on a lone half returns the half itself, not the pair ---
console.log("half-hi-cp=" + units[1].codePointAt(0).toString(16));
console.log("full-cp=" + emoji.codePointAt(1).toString(16));

// --- contrast: spread iterates by code point, keeping the pair whole ---
const pts = [...emoji];
console.log("spread-lens=" + pts.map((c) => c.length).join(","));
console.log("spread-cps=" + pts.map((c) => c.codePointAt(0).toString(16)).join(","));

// --- multiple surrogate pairs ---
const two = "\u{1F600}\u{1F601}";
console.log("two-split-len=" + two.split("").length);
console.log("two-spread-len=" + [...two].length);

// --- BMP astral boundary: U+FFFF is one unit, U+10000 is two ---
console.log("ffff-split=" + String.fromCharCode(0xFFFF).split("").length);
console.log("10000-split=" + "\u{10000}".split("").length);

// --- combining marks are separate units either way (no grapheme clustering) ---
const combining = "e" + String.fromCharCode(0x0301); // e + combining acute
console.log("combining-split=" + combining.split("").length);
console.log("combining-spread=" + [...combining].length);

// --- empty string and ASCII baselines ---
console.log("empty-split-len=" + "".split("").length);
console.log("ascii-split=" + "abc".split("").join("|"));

// --- split("") with a limit still counts units ---
console.log("limit=" + emoji.split("", 3).map((c) => c.charCodeAt(0).toString(16)).join(","));
console.log("limit0-len=" + emoji.split("", 0).length);
