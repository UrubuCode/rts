// Cross-runtime: the String iterator walks CODE POINTS, not UTF-16 code units.
// Focus: surrogate pairs via for-of / spread / Array.from vs .length + index.

// A 4-byte astral char (U+1F600) built from its surrogate halves, so this
// source file stays pure ASCII and cannot be mangled by encoding.
const grin = String.fromCharCode(0xd83d, 0xde00); // U+1F600
const musical = String.fromCharCode(0xd834, 0xdd1e); // U+1D11E

// 1) .length counts UNITS, the iterator counts POINTS
const s1 = "a" + grin + "b";
console.log("length=" + s1.length);
console.log("fromLen=" + Array.from(s1).length);
console.log("spreadLen=" + [...s1].length);

// 2) for-of yields whole code points
const chars: string[] = [];
for (const c of s1) chars.push(String(c.length));
console.log("forOfUnitLens=" + chars.join(","));

// 3) index access splits the pair; the iterator does not
console.log("indexUnitLen=" + s1[1].length);
console.log("iterCodePoint=" + Array.from(s1)[1].codePointAt(0));

// 4) codePointAt vs charCodeAt on the astral char
console.log("codePointAt=" + s1.codePointAt(1));
console.log("charCodeAt=" + s1.charCodeAt(1));

// 5) a pure-BMP string: units === points
const bmp = "hello";
console.log("bmpLen=" + bmp.length + "|bmpPoints=" + Array.from(bmp).length);

// 6) multiple astral chars in a row
const many = grin + musical + grin;
console.log("manyLen=" + many.length + "|manyPoints=" + Array.from(many).length);

// 7) round-trip: joining the iterated points rebuilds the string
console.log("roundTrip=" + (Array.from(s1).join("") === s1));

// 8) a LONE (unpaired) high surrogate is yielded as a single unit
const lone = "x" + String.fromCharCode(0xd83d) + "y";
console.log("loneLen=" + lone.length + "|lonePoints=" + Array.from(lone).length);
console.log("loneMid=" + Array.from(lone)[1].charCodeAt(0));

// 9) a REVERSED pair (low then high) is not a pair: 2 separate points
const reversed = String.fromCharCode(0xde00, 0xd83d);
console.log("reversedPoints=" + Array.from(reversed).length);

// 10) Array.from with a mapFn sees code points
console.log("mapFn=" + Array.from(s1, (c: string) => c.codePointAt(0)).join(","));

// 11) the string iterator is a fresh one each time (re-iterable)
const first = [...s1].length;
const second = [...s1].length;
console.log("reIterable=" + (first === second) + "|" + first);

// 12) manual String Symbol.iterator drive
const it = s1[Symbol.iterator]();
const r1 = it.next();
const r2 = it.next();
const r3 = it.next();
const r4 = it.next();
console.log("manual1=" + r1.value + "|done=" + r1.done);
console.log("manual2Points=" + r2.value.length + "|done=" + r2.done);
console.log("manual3=" + r3.value + "|done=" + r3.done);
console.log("manual4=" + String(r4.value) + "|done=" + r4.done);

// 13) empty string iterates zero times
let emptyN = 0;
for (const _c of "") emptyN++;
console.log("emptyIter=" + emptyN);

// 14) split("") uses UNITS (contrast with the iterator)
console.log("splitLen=" + s1.split("").length + "|iterLen=" + [...s1].length);
