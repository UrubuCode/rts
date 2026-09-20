// Latin-1 text of thirty code units or fewer is held inside the string rather
// than in a heap buffer of its own. Nothing a program can do observes that —
// which is the point — so what is written here is everything that WOULD break
// if the two forms disagreed: the boundary in both directions, equality across
// it, keys, and text that arrives by every route the runtime builds one.
//
// The boundary is the interesting part. A string of exactly thirty characters
// is held inline and one of thirty-one is not, so every pair below straddles a
// representation change that must be invisible.
//
// Checked against node, and it passes on a binary from before the change.
import { describe, test, expect } from "rts:test";

const AT = "x".repeat(30);
const PAST = "x".repeat(31);

describe("the boundary is invisible", () => {
  test("length, indexing and iteration either side of it", () => {
    expect(AT.length).toBe(30);
    expect(PAST.length).toBe(31);
    expect(AT[0]).toBe("x");
    expect(AT[29]).toBe("x");
    expect(AT[30]).toBe(undefined);
    expect(PAST[30]).toBe("x");
    expect([...AT].length).toBe(30);
    expect(AT.charCodeAt(29)).toBe(120);
  });

  test("a string that crosses the boundary by growing", () => {
    let built = "";
    for (let i = 0; i < 40; i++) built += "y";
    expect(built.length).toBe(40);
    expect(built.slice(0, 30)).toBe("y".repeat(30));
    expect(built === "y".repeat(40)).toBe(true);
  });

  test("and one that crosses it by shrinking", () => {
    const long = "z".repeat(64);
    expect(long.slice(0, 30).length).toBe(30);
    expect(long.slice(0, 31).length).toBe(31);
    expect(long.slice(0, 30) === "z".repeat(30)).toBe(true);
    expect(long.slice(34) === "z".repeat(30)).toBe(true);
    expect("".length).toBe(0);
    expect("".slice(0, 5)).toBe("");
  });

  test("equality does not depend on how the text was built", () => {
    const grown = "x".repeat(29) + "x";
    const cut = ("x".repeat(40)).slice(0, 30);
    const joined = ["x".repeat(15), "x".repeat(15)].join("");
    expect(grown === AT).toBe(true);
    expect(cut === AT).toBe(true);
    expect(joined === AT).toBe(true);
    expect(grown === PAST).toBe(false);
    expect([grown, cut, joined].every((one) => one.length === 30)).toBe(true);
  });
});

describe("a key is a key whichever side of the boundary it is on", () => {
  test("reading back what was written under a computed key", () => {
    const held: Record<string, number> = {};
    held[AT] = 1;
    held[PAST] = 2;
    expect(held[AT]).toBe(1);
    expect(held[PAST]).toBe(2);
    expect(held["x".repeat(30)]).toBe(1, "a second spelling of the same key");
    expect(Object.keys(held).length).toBe(2);
    expect(AT in held).toBe(true);
  });

  test("a Map and a Set hash the text, not the layout", () => {
    const map = new Map<string, number>();
    map.set(AT, 1);
    map.set(PAST, 2);
    expect(map.get("x".repeat(30))).toBe(1);
    expect(map.get("x".repeat(31))).toBe(2);
    expect(map.size).toBe(2);
    const set = new Set([AT, "x".repeat(30), PAST]);
    expect(set.size).toBe(2);
  });

  test("a round trip through JSON keeps both", () => {
    const back = JSON.parse(JSON.stringify({ [AT]: AT, [PAST]: PAST }));
    expect(back[AT]).toBe(AT);
    expect(back[PAST]).toBe(PAST);
    expect(Object.keys(back)[0].length).toBe(30);
  });
});

describe("text that arrives by every route the runtime builds one", () => {
  test("numbers, concatenation, templates and case", () => {
    expect(String(42)).toBe("42");
    expect((255).toString(16)).toBe("ff");
    expect(`${1}-${2}`).toBe("1-2");
    expect("a" + 1 + true + null).toBe("a1truenull");
    expect("AbC".toUpperCase()).toBe("ABC");
    expect("AbC".toLowerCase()).toBe("abc");
    expect("  pad  ".trim()).toBe("pad");
    expect("a-b-c".split("-").join("+")).toBe("a+b+c");
    expect("abc".repeat(3)).toBe("abcabcabc");
    expect("abcdef".indexOf("cd")).toBe(2);
    expect("abc".replace("b", "B")).toBe("aBc");
    expect("café".length).toBe(4, "Latin-1 above ASCII is still narrow");
    expect("café".toUpperCase()).toBe("CAFÉ");
  });

  test("wide text is not narrowed and keeps its units", () => {
    const wide = "中文";
    expect(wide.length).toBe(2);
    expect(wide.charCodeAt(0)).toBe(0x4e2d);
    expect(("a" + wide).length).toBe(3);
    expect((wide + "a")[2]).toBe("a");
    expect(("\ud800" + "x").charCodeAt(0)).toBe(0xd800, "a lone surrogate survives");
    expect("😀".length).toBe(2);
    expect([..."😀"].length).toBe(1);
  });

  test("a narrow and a wide spelling of the same text are one string", () => {
    // `"a"` is narrow; the same character out of a wide string is built the
    // wide way and narrowed. They must be equal and hash alike.
    const fromWide = ("中a".slice(1));
    expect(fromWide).toBe("a");
    expect(fromWide === "a").toBe(true);
    const held: Record<string, number> = { a: 1 };
    expect(held[fromWide]).toBe(1);
    expect(new Set(["a", fromWide]).size).toBe(1);
  });
});

describe("sorting and comparing short text", () => {
  test("order is by code unit, across the boundary", () => {
    const sorted = [PAST, AT, "b", "a", ""].sort();
    expect(sorted[0]).toBe("");
    expect(sorted[1]).toBe("a");
    expect(sorted[2]).toBe("b");
    expect(sorted[3].length).toBe(30);
    expect(sorted[4].length).toBe(31);
    expect("a" < "b").toBe(true);
    expect(AT < PAST).toBe(true, "a prefix sorts before what extends it");
  });
});
