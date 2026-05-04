// Termometro de paridade JS/TS — exercita ~60 APIs adicionadas no
// epic #226. Quando o expected falhar, divergencia entre RTS e
// Bun/Node foi introduzida (regressao) ou consertada (atualizar).
//
// Linhas marcadas FIXME indicam gaps conhecidos que ainda diferem
// de bun/node — mantidas no expected = saida atual do RTS para
// servir de baseline de regressao.

import { describe, test, expect } from "rts:test";

let out: string = "";
function p(label: string, v: string): void { out += label + "=" + v + "\n"; }

// ============ Array ============
const arr = [1, 2, 3, 4, 5, 2, 3];
p("indexOf", arr.indexOf(3).toString());
p("indexOfFrom", arr.indexOf(3, 3).toString());
p("lastIndexOf", arr.lastIndexOf(2).toString());

const splArr = [1, 2, 3, 4, 5];
splArr.splice(1, 2);
p("spliceRemove", splArr.join(","));

const nums = [1, 5, 2, 8, 3];
p("findLast", nums.findLast((x: number) => x < 5).toString());
p("findLastIndex", nums.findLastIndex((x: number) => x < 5).toString());

const strs = ["banana", "apple", "cherry"];
strs.sort();
p("sortStrings", strs.join(","));

p("arrayFromLen", Array.from({ length: 3 }).length.toString());

// ============ Object ============
const o = { a: 1, b: 2, c: 3 };
p("objEntries", Object.entries(o).length.toString());

const merged = Object.assign({}, { x: 1 }, { y: 2 });
p("objAssign", (merged.x + merged.y).toString());

const fe = Object.fromEntries([["k1", 10], ["k2", 20]]);
p("fromEntries", (fe.k1 + fe.k2).toString());

// ============ Math ============
p("sign_neg", Math.sign(-5).toString());
p("sign_pos", Math.sign(7).toString());
p("hypot", Math.hypot(3, 4).toString());
p("imul", Math.imul(3, 4).toString());
p("clz32", Math.clz32(1).toString());
p("SQRT2", Math.SQRT2.toFixed(4));
p("LN2", Math.LN2.toFixed(4));

// ============ Symbol ============
const s2 = Symbol.for("shared");
p("symKeyFor", Symbol.keyFor(s2));

// ============ URL ============
const u = new URL("https://example.com/path?x=1&y=2#frag");
p("urlHost", u.host);
p("urlPath", u.pathname);
p("urlSearch", u.search);
p("urlHash", u.hash);

const sp = new URLSearchParams("a=1&b=2");
p("spGet", sp.get("a"));
sp.set("a", "9");
p("spSet", sp.get("a"));

// ============ Date ============
const d = new Date(0);
p("dateISO", d.toISOString());
const d2 = new Date(0);
d2.setFullYear(2025);
p("dateSetYear", d2.getFullYear().toString());

// ============ parseInt radix ============
p("parseHex", parseInt("ff", 16).toString());
p("parseAuto", parseInt("0x10").toString());
p("parseBin", parseInt("1010", 2).toString());

// ============ encodeURIComponent ============
p("enc", encodeURIComponent("a b&c"));
p("dec", decodeURIComponent("a%20b%26c"));

// ============ Destructuring (#210) ============
const [da, db, ...drest] = [1, 2, 3, 4, 5];
p("destrArr", da + "," + db + "," + drest.join("/"));

const [[na, nb]] = [[7, 8]];
p("destrNested", na + "," + nb);

const expected =
  "indexOf=2\nindexOfFrom=6\nlastIndexOf=5\n" +
  "spliceRemove=1,4,5\n" +
  "findLast=3\nfindLastIndex=4\n" +
  "sortStrings=apple,banana,cherry\n" +
  "arrayFromLen=3\n" +
  "objEntries=3\nobjAssign=3\nfromEntries=30\n" +
  "sign_neg=-1\nsign_pos=1\nhypot=5\nimul=12\nclz32=31\n" +
  "SQRT2=1.4142\nLN2=0.6931\n" +
  "symKeyFor=shared\n" +
  "urlHost=example.com\nurlPath=/path\nurlSearch=?x=1&y=2\nurlHash=#frag\n" +
  "spGet=1\nspSet=9\n" +
  "dateISO=1970-01-01T00:00:00.000Z\ndateSetYear=2025\n" +
  "parseHex=255\nparseAuto=16\nparseBin=10\n" +
  "enc=a%20b%26c\ndec=a b&c\n" +
  "destrArr=1,2,3/4/5\ndestrNested=7,8\n";

describe("js parity epic #226 — baseline", () => {
  test("APIs estaveis batem com bun/node", () => expect(out).toBe(expected));
});
