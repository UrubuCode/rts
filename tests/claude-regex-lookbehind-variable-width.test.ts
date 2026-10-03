// A lookbehind whose content holds a QUANTIFIER — #2891.
//
// `new RegExp("(?<=\\.\\s*)[a-z]+")` threw a `SyntaxError`. JavaScript is one of
// the few languages whose lookbehind may be of unbounded width, and
// `fancy-regex` — the backtracking half of this engine's matcher — compiles a
// lookbehind as a jump back of a FIXED number of characters. So the whole
// pattern was refused, which blocked `kire` at its first `render`: one
// occurrence, in the identifier scanner of its template compiler.
//
// `regex/lookbehind.rs` carries the design. What it does NOT support is in the
// last section of this file, and that half is the point: #2839 settled that a
// pattern with no spelling in Rust is REFUSED rather than approximated, so a
// form out of reach must fail at construction and say which form it was.
//
// Every value here was measured under node 22 and bun 1.4 on 2026-10-03, and
// the two agreed on every one.
import { test, expect } from "rts:test";

const text = "a. foo b.x c.  bar";

// ------------------------------------------------- the five forms that failed --

test("a quantifier inside a positive lookbehind", () => {
  expect(JSON.stringify(text.match(/(?<=\.\s*)[a-z]+/g))).toBe(
    '["foo","x","bar"]'
  );
});

test("an unbounded quantifier, matching and not matching", () => {
  expect(JSON.stringify("cab".match(/(?<=a+)b/))).toBe('["b"]');
  expect("b".match(/(?<=a+)b/)).toBe(null);
});

test("a bounded quantifier, matching and not matching", () => {
  expect(JSON.stringify("xxy".match(/(?<=x{1,3})y/))).toBe('["y"]');
  expect("y".match(/(?<=x{1,3})y/)).toBe(null);
});

test("a quantifier inside a NEGATIVE lookbehind", () => {
  // The second answer is what decides how a rejected candidate is skipped: by
  // ONE character, not by the length of the body's match. At index 3 the
  // lookbehind holds and the negation fails; at index 4 it does not, so `"oo"`
  // is a match.
  expect(JSON.stringify("a. foo".match(/(?<!\.\s*)[a-z]+/g))).toBe('["a","oo"]');
});

// ------------------------------------------- the three forms that already ran --

test("a fixed-width lookbehind still answers", () => {
  expect(JSON.stringify("a.foo b.x c.bar".match(/(?<=\.)[a-z]+/g))).toBe(
    '["foo","x","bar"]'
  );
});

test("an alternation of differently sized branches still answers", () => {
  expect(JSON.stringify("abd cd xd".match(/(?<=ab|c)d/g))).toBe('["d","d"]');
});

test("a lookahead and a named group are untouched", () => {
  expect(JSON.stringify("a1".match(/a(?=\d)/))).toBe('["a"]');
  expect("year 2026".match(/(?<year>\d{4})/)!.groups!.year).toBe("2026");
});

// ---------------------------------------------------- zero repetitions, and ^ --

test("a lookbehind that can match nothing holds at position zero", () => {
  expect(JSON.stringify("abc".match(/(?<=x*)a/))).toBe('["a"]');
  expect(JSON.stringify("  a a".match(/(?<=\s*)a/g))).toBe('["a","a"]');
});

test("combined with ^ and with $", () => {
  expect(JSON.stringify("aab".match(/(?<=^a*)b/))).toBe('["b"]');
  expect(JSON.stringify("aaa".match(/(?<=a+)$/))).toBe('[""]');
});

// ------------------------------------- lastIndex, a loop, and the text before --

test("exec in a loop walks every match", () => {
  const walked = /(?<=\.\s*)[a-z]+/g;
  const steps: unknown[] = [];
  let step: RegExpExecArray | null;
  while ((step = walked.exec(text)) !== null) {
    steps.push([step[0], step.index, walked.lastIndex]);
  }
  expect(JSON.stringify(steps)).toBe('[["foo",3,6],["x",9,10],["bar",15,18]]');
});

test("a resumed search still reads the text BEFORE where it resumes", () => {
  const jumped = /(?<=\.\s*)[a-z]+/g;
  jumped.lastIndex = 5;
  const landed = jumped.exec(text)!;
  expect(JSON.stringify([landed[0], landed.index, jumped.lastIndex])).toBe(
    '["x",9,10]'
  );
});

test("matchAll answers the same three", () => {
  const seen = [...text.matchAll(/(?<=\.\s*)[a-z]+/g)].map((m) => [
    m[0],
    m.index,
  ]);
  expect(JSON.stringify(seen)).toBe('[["foo",3],["x",9],["bar",15]]');
});

// --------------------------------------------------------- groups in the BODY --

test("a group after the lookbehind is group one", () => {
  expect(JSON.stringify("a.  foo".match(/(?<=\.\s*)([a-z]+)/))).toBe(
    '["foo","foo"]'
  );
  expect("a.  foo".match(/(?<=\.\s*)(?<word>[a-z]+)/)!.groups!.word).toBe("foo");
});

// ------------------------------------------- what is REFUSED, and by what name --

// A refusal must name the form. `invalid regular expression: /<the pattern>/`
// was the whole message, and the pattern is the part the reader already has.
function refusalFor(pattern: string): string {
  try {
    new RegExp(pattern);
    return "accepted";
  } catch (error: any) {
    return String(error.message);
  }
}

test("a lookbehind that is not the first thing in the pattern", () => {
  expect(refusalFor("x(?<=a+)b").indexOf("first thing in the pattern") >= 0).toBe(
    true
  );
  expect(
    refusalFor("(?:(?<=a+)b)").indexOf("first thing in the pattern") >= 0
  ).toBe(true);
});

test("a capture inside the lookbehind", () => {
  // Refused rather than answered: a lookbehind matches RIGHT TO LEFT in
  // JavaScript, so a group inside one captures a different substring than the
  // same group read forwards — #2502. Asking the runtime check for a BOOLEAN
  // makes the direction unobservable, which is what makes it exact.
  expect(
    refusalFor("(?<=(a+))b").indexOf("capture group inside a lookbehind") >= 0
  ).toBe(true);
});

test("a lookahead inside the lookbehind", () => {
  expect(
    refusalFor("(?<=a+(?=b))c").indexOf("lookahead inside a lookbehind") >= 0
  ).toBe(true);
});
