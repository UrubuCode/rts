// Cross-runtime future coverage: regex captures, exec and lastIndex.
const text = "Item-42 and item-99";
const basic = /item-(\d+)/i;
const globalRx = /item-(\d+)/gi;

console.log("test=" + basic.test(text));

const match = text.match(basic);
console.log("match0=" + (match ? match[0] : "none"));
console.log("match1=" + (match ? match[1] : "none"));

console.log("replace=" + text.replace(/item/gi, "unit"));
console.log("replaceAll=" + "a-b-c".replaceAll("-", "/"));
console.log("split=" + "x1y2z3".split(/\d/).join(","));

const ex1 = globalRx.exec(text);
console.log("g1=" + (ex1 ? ex1[1] : "none"));
console.log("last1=" + globalRx.lastIndex);
const ex2 = globalRx.exec(text);
console.log("g2=" + (ex2 ? ex2[1] : "none"));
console.log("last2=" + globalRx.lastIndex);

const multi = "aa\nbb";
console.log("flag_m=" + /^bb/m.test(multi));
console.log("flag_s=" + /a.b/s.test("a\nb"));
