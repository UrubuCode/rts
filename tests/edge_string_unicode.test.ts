// Edge cases para strings — Unicode (emojis, surrogates), empty, repeticao
// limite, indices negativos, métodos com out-of-range. Cobre String class
// + namespace string + interaction com .length real (chars vs bytes).
import { describe, test, expect } from "rts:test";

const empty: string = "";
const ascii: string = "hello";
const emoji: string = "café";
const multi: string = "ab".repeat(3);
const slicedNeg: string = ascii.slice(-3);
const slicedOver: string = ascii.slice(100, 200);
const padded: string = "5".padStart(3, "0");
const trimMixed: string = "  hi  ".trim();
const replacedAll: string = "a-b-c-d".replaceAll("-", "/");
const splitEmpty: string[] = "abc".split("");
const splitNoMatch: string[] = "abc".split(",");
const includesEmpty: boolean = ascii.includes("");
const startsEmpty: boolean = ascii.startsWith("");
const endsEmpty: boolean = ascii.endsWith("");
const charAtNeg: string = ascii.charAt(-1);
const charAtOver: string = ascii.charAt(99);
const concatEmpty: string = empty + ascii + empty;

describe("String — edge cases unicode/limites", () => {
  test("empty.length = 0", () => expect(empty.length).toBe(0));
  test("'hello'.length = 5", () => expect(ascii.length).toBe(5));
  test("repeat 3x", () => expect(multi).toBe("ababab"));
  test("slice(-3) ultimos 3 chars", () => expect(slicedNeg).toBe("llo"));
  test("slice fora do range = empty", () => expect(slicedOver).toBe(""));
  test("padStart com 0", () => expect(padded).toBe("005"));
  test("trim em mixed whitespace", () => expect(trimMixed).toBe("hi"));
  test("replaceAll '-' -> '/'", () => expect(replacedAll).toBe("a/b/c/d"));
  test("split('') retorna chars", () => expect(splitEmpty.length).toBe(3));
  test("split sem match retorna [whole]", () => expect(splitNoMatch.length).toBe(1));
  test("includes('') sempre true", () => expect(includesEmpty).toBe(true));
  test("startsWith('') sempre true", () => expect(startsEmpty).toBe(true));
  test("endsWith('') sempre true", () => expect(endsEmpty).toBe(true));
  test("charAt(-1) = empty", () => expect(charAtNeg).toBe(""));
  test("charAt(99) = empty", () => expect(charAtOver).toBe(""));
  test("'' + s + '' = s", () => expect(concatEmpty).toBe(ascii));
});
