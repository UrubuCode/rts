// A lookbehind of unbounded width inside one BRANCH of an alternation — #2891,
// the half #2897 did not reach, and one answer it got wrong.
//
// #2897 lifted the width limit for a lookbehind that is the first thing in a
// pattern: it takes the lookbehind out and this runtime answers it, by asking
// whether its content ends exactly where the match would begin. An alternation
// breaks that in two ways, and the first is the one worth reading:
//
//   /(?<=\.\s*)[a-z]+|z/ has its lookbehind leading, so it was ACCEPTED — and
//   the body matched under the check was `[a-z]+|z`, which applies the
//   lookbehind to `z` as well. `"z".match` of it answers `["z"]` in node 22 and
//   bun 1.4 and answered `null` here. A wrong answer that nothing refused.
//
//   /z|(?<=\.\s*)[a-z]+/ has it leading inside the second branch, and was
//   refused outright — which is what blocked `kire`, whose identifier scanner
//   is a three-branch alternation with the variable lookbehind in the middle
//   one.
//
// Both are one fact: in JavaScript a lookbehind at the head of a branch governs
// that branch and nothing else. So a top-level alternation neither engine
// compiles is taken apart, each branch compiled on its own, and the results
// recombined by the rule the language actually has — the leftmost position any
// branch matches at, and on a tie the EARLIEST branch, because an alternation
// is ordered rather than greedy. `regex/compile/branches.rs` carries the design.
//
// Every value below was measured under node 22 and bun 1.4 on 2026-10-03, with
// one process each; the two agreed on all 27 probes.
import { test, expect } from "rts:test";

const text = "a. foo z b.x c.  bar";

// `kire`'s identifier scanner, verbatim from core/src/utils/regex.ts:41 —
// strings, a property name after a dot, and a bare identifier.
const scanner =
  /(?:['"`].*?['"`])|(?<=\.\s*)[a-zA-Z_$][a-zA-Z0-9_$]*|(?<![a-zA-Z0-9_$])([a-zA-Z_$][a-zA-Z0-9_$]*)(?![a-zA-Z0-9_$])/;

// ------------------------------ a lookbehind governs its own branch only --

test("the leading branch's lookbehind does not reach the other branches", () => {
  // This is the wrong answer: `["foo","x","bar"]`, with the `z` dropped.
  expect(JSON.stringify(text.match(/(?<=\.\s*)[a-z]+|z/g))).toBe(
    '["foo","z","x","bar"]'
  );
  expect(JSON.stringify("z".match(/(?<=\.\s*)[a-z]+|z/g))).toBe('["z"]');
  expect(JSON.stringify("z".match(/(?<=\.\s*)[a-z]+|z/))).toBe('["z"]');
});

test("a lookbehind in a LATER branch", () => {
  expect(JSON.stringify(text.match(/z|(?<=\.\s*)[a-z]+/g))).toBe(
    '["foo","z","x","bar"]'
  );
  expect(JSON.stringify("a. foo".match(/z|(?<=\.\s*)[a-z]+/))).toBe('["foo"]');
});

test("a NEGATIVE lookbehind in a branch", () => {
  expect(JSON.stringify("a. foo".match(/q|(?<!\.\s*)[a-z]+/g))).toBe(
    '["a","oo"]'
  );
});

// ------------------------------------------------- the pattern of the issue --

test("kire's identifier scanner", () => {
  const all = new RegExp(scanner.source, "g");
  expect(JSON.stringify('u . nome + "s" + a.b'.match(all))).toBe(
    '["u","nome","\\"s\\"","a","b"]'
  );
});

test("the scanner's capture group keeps its number", () => {
  // Group 1 belongs to the THIRD branch, and is `undefined` for a match the
  // second branch made — which is how the library tells a bare identifier
  // from a property name, and the whole reason the numbering matters.
  const all = new RegExp(scanner.source, "g");
  const got: string[] = [];
  let m = all.exec("u . nome + a.b");
  while (m !== null) {
    got.push(m[0] + ":" + (m[1] === undefined ? "-" : m[1]));
    m = all.exec("u . nome + a.b");
  }
  expect(got.join(" ")).toBe("u:u nome:- a:a b:-");
});

// ------------------------------------------- the two halves of the rule --

test("a tie of position goes to the earlier branch, not the longer match", () => {
  // Both branches can match at 0. An alternation is ordered rather than
  // greedy, so the written order decides and the answers differ.
  expect(JSON.stringify("ab".match(/(?<=x*)ab|a/))).toBe('["ab"]');
  expect(JSON.stringify("ab".match(/a|(?<=x*)ab/))).toBe('["a"]');
});

test("the leftmost branch position wins over the earlier branch", () => {
  expect("a. foo z b.x c.  bar".search(/z|(?<=\.\s*)[a-z]+/)).toBe(3);
});

// ------------------------------------------------------ driving the search --

test("exec in a loop advances lastIndex the way every other match does", () => {
  const re = /(?<=\.\s*)[a-z]+|z/g;
  const got: string[] = [];
  let m = re.exec(text);
  while (m !== null) {
    got.push(m[0] + "@" + m.index + ":" + re.lastIndex);
    m = re.exec(text);
  }
  expect(got.join(" ")).toBe("foo@3:6 z@7:8 x@11:12 bar@17:20");
});

test("a lastIndex the program wrote is honoured", () => {
  const re = /(?<=\.\s*)[a-z]+|z/g;
  re.lastIndex = 10;
  const m = re.exec(text);
  expect(m === null ? "null" : m[0] + "@" + m.index + ":" + re.lastIndex).toBe(
    "x@11:12"
  );
});

test("matchAll, replace with and without g, and split", () => {
  const seen = [...text.matchAll(/(?<=\.\s*)[a-z]+|z/g)].map(
    (m) => m[0] + "@" + m.index
  );
  expect(seen.join(" ")).toBe("foo@3 z@7 x@11 bar@17");
  expect(text.replace(/(?<=\.\s*)[a-z]+|z/g, "<$&>")).toBe(
    "a. <foo> <z> b.<x> c.  <bar>"
  );
  expect(text.replace(/(?<=\.\s*)[a-z]+|z/, "<$&>")).toBe(
    "a. <foo> z b.x c.  bar"
  );
  expect(JSON.stringify("a1b|c2d".split(/(?<=x*)\d|\|/))).toBe(
    '["a","b","c","d"]'
  );
});

test("sticky anchors at lastIndex and does not search forward", () => {
  const hit = /(?<=\.\s*)[a-z]+|z/y;
  hit.lastIndex = 3;
  const first = hit.exec(text);
  expect((first === null ? "null" : first[0]) + ":" + hit.lastIndex).toBe(
    "foo:6"
  );
  const miss = /(?<=\.\s*)[a-z]+|z/y;
  miss.lastIndex = 0;
  expect((miss.exec(text) === null ? "null" : "hit") + ":" + miss.lastIndex).toBe(
    "null:0"
  );
});

// ------------------------------------------ the edges a split would break --

test("zero repetitions at the start of the subject is a legal match", () => {
  expect(JSON.stringify("abc".match(/q|(?<=x*)a/))).toBe('["a"]');
});

test("combined with the two anchors", () => {
  expect(JSON.stringify("aab".match(/q|(?<=^a*)b/))).toBe('["b"]');
  expect(JSON.stringify("aaa".match(/q|(?<=a+)$/))).toBe('[""]');
});

test("a bar inside a group or a class is not a branch boundary", () => {
  // Splitting at one of these would compile a different regular expression.
  expect(JSON.stringify("ab".match(/(?<=x*)(?:a|b)+/))).toBe('["ab"]');
  expect(JSON.stringify("a|b".match(/(?<=x*)[a|b]+/))).toBe('["a|b"]');
});

test("groups are numbered across the branches, absent rather than empty", () => {
  const first = "ab".match(/(?<=x*)(a)(b)|(c)/);
  expect(first === null ? "null" : first.length).toBe(4);
  expect(first === null ? "null" : first[1] + first[2]).toBe("ab");
  expect(first === null ? "x" : first[3] === undefined ? "undefined" : "set").toBe(
    "undefined"
  );
  const second = "c".match(/(?<=x*)(a)(b)|(c)/);
  expect(second === null ? "x" : second[1] === undefined ? "undefined" : "set").toBe(
    "undefined"
  );
  expect(second === null ? "null" : second[3]).toBe("c");
});

test("a named group in a later branch keeps its position", () => {
  const m = "c".match(/(?<=x*)(?<head>a)|(?<tail>c)/);
  expect(m === null ? "null" : m.groups.tail).toBe("c");
  expect(
    m === null ? "x" : m.groups.head === undefined ? "undefined" : "set"
  ).toBe("undefined");
});

// --------------------------------------------- what stays out of reach --

test("a backreference in a LATER branch refuses the split", () => {
  // Splitting renumbers every group after the first branch, so `\1` here would
  // come to name `(b)` instead of `(a)`: node and bun answer `"b"` for
  // `"bb".match(/(a)|(?<=x*)(b)\1/)` — the backreference to a group that took
  // part in nothing matches the empty string — and a split branch of `(b)\1`
  // would answer `"bb"`. #2839 settled that a pattern with no spelling in Rust
  // is refused rather than approximated, so this one says which form it was.
  let said = "no throw";
  try {
    new RegExp("(a)|(?<=x*)(b)\\1");
  } catch (e) {
    said = (e as Error).message;
  }
  expect(said.indexOf("numbered backreference after the first branch") >= 0).toBe(
    true
  );
});

test("a backreference in the FIRST branch keeps working", () => {
  // Its numbering is untouched by the split, and this pattern already answered
  // correctly before the split existed — so refusing it to buy a simpler rule
  // would be a capability lost rather than a hazard avoided.
  expect(JSON.stringify("bb".match(/(?<=x*)(b)\1|(a)/))).toBe(
    '["bb","b",null]'
  );
  expect("b".match(/(?<=x*)(b)\1|(a)/)).toBe(null);
});

test("a lookbehind that is not leading in its branch is still refused", () => {
  let said = "no throw";
  try {
    new RegExp("z|x(?<=\\.\\s*)y");
  } catch (e) {
    said = (e as Error).message;
  }
  expect(said.indexOf("first thing in the pattern") >= 0).toBe(true);
});
