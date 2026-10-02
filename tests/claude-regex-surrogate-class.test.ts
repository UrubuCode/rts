// A surrogate range inside a character class — #2837.
//
// `/[\u0000-\u001f\u0022\u005c\ud800-\udfff]/` is how `safe-stable-stringify` asks
// "does this string need escaping", and it is the shape every JSON escaper uses.
// It used to be refused when the PATTERN was compiled, which took the whole program
// with it.
//
// It is now rewritten to the astral range, which is NOT exact — agreed 2026-10-02
// with the divergences measured rather than discovered, and
// `regex/translate/surrogates.rs` carries the reasoning. This fixture asserts BOTH
// halves: what agrees, and what diverges. The second half is the point. A fixture
// that pinned only the agreements would read as though the rewrite were exact, and
// the next person would find the rest the hard way.
//
// Every value here was measured under node 22.23.2.
import { test, expect } from "rts:test";

const escaping = /[\u0000-\u001f\u0022\u005c\ud800-\udfff]/;
const astral = "a\u{1F600}b"; // one astral character: two UTF-16 code units

// ---------------------------------------------------------------- it compiles --

test("the pattern compiles at all", () => {
  expect(escaping instanceof RegExp).toBe(true);
  expect(escaping.source.indexOf("ud800") >= 0).toBe(true);
});

// ------------------------------------------------------------- what AGREES ----

test("plain text needs no escaping", () => {
  expect(escaping.test("hello")).toBe(false);
  expect(escaping.test("")).toBe(false);
});

test("a control character, a quote and a backslash are found", () => {
  expect(escaping.test("a\u0001b")).toBe(true);
  expect(escaping.test('a"b')).toBe(true);
  expect(escaping.test("a\\b")).toBe(true);
});

// THE case the rewrite exists for: the predicate, which is what the class is read
// by almost every time it appears.
test("astral text is found", () => {
  expect(escaping.test(astral)).toBe(true);
  expect(/[\ud800-\udfff]/.test(astral)).toBe(true);
});

test("and the position of the match agrees", () => {
  const found = /[\ud800-\udfff]/.exec(astral);
  expect(found === null).toBe(false);
  expect((found as any).index).toBe(1);
});

// ------------------------------------------------------------ what DIVERGES ---

// node answers 1 — the match is ONE code unit, half of the pair. Here the match is
// the whole character, so it is 2.
test("DIVERGES: the match's length is the character, not the code unit", () => {
  const found: any = /[\ud800-\udfff]/.exec(astral);
  expect(found[0].length).toBe(2);
});

// node answers 2 — one match per half.
test("DIVERGES: a global match counts the character once", () => {
  const found = astral.match(/[\ud800-\udfff]/g);
  expect((found as any).length).toBe(1);
});

// node answers "a##b".
test("DIVERGES: a replace substitutes once", () => {
  expect(astral.replace(/[\ud800-\udfff]/g, "#")).toBe("a#b");
});

// node answers true: a lone surrogate IS a code unit in the range. Here the class
// is a range of astral characters, and a lone surrogate is not one.
test("DIVERGES: a lone surrogate is not found", () => {
  const lone = "a" + String.fromCharCode(0xd800) + "b";
  expect(/[\ud800-\udfff]/.test(lone)).toBe(false);
});

// ------------------------------------------------------- the `u` flag is apart --

// Not caution — the class means the OPPOSITE under `u`, and node says so: astral
// text is `false` there and a lone surrogate is `true`. Rewriting would invert an
// answer rather than widen one, so a `u` pattern is left exactly as it was.
//
// Which leaves a divergence of its own, stated rather than implied: node ACCEPTS
// that pattern under `u` and answers `false` for astral text, where this refuses the
// pattern outright. A refusal is the visible kind of wrong and an inverted predicate
// is the silent kind, so this is the direction to be wrong in until the exact form —
// matching over code units — exists.
test("DIVERGES: a `u` pattern is refused, where node accepts it", () => {
  let threw = false;
  try {
    new RegExp("[\\ud800-\\udfff]", "u").test(astral);
  } catch {
    threw = true;
  }
  expect(threw).toBe(true);
});

// --------------------------------------------------- the reason this was done --

// The program that could not run. `safe-stable-stringify`'s fast path: if the class
// does not match, the string is quoted as it is; if it does, `JSON.stringify` is
// asked. Both branches are correct here, and a false positive would be too.
test("the JSON fast path answers both ways", () => {
  const quick = (text: string) => (!escaping.test(text) ? `"${text}"` : JSON.stringify(text));
  expect(quick("hello")).toBe('"hello"');
  expect(quick('a"b')).toBe(JSON.stringify('a"b'));
  expect(quick(astral)).toBe(JSON.stringify(astral));
});
