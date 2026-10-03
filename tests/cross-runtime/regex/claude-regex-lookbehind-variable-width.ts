// Cross-runtime: a lookbehind whose content holds a QUANTIFIER — #2891.
//
// JavaScript is one of the few languages whose lookbehind may be of unbounded
// width. Every line here was measured under node 22 and bun 1.4 on 2026-10-03
// and the two agreed on all of them.
const text = "a. foo b.x c.  bar";
console.log(JSON.stringify(text.match(/(?<=\.\s*)[a-z]+/g)));
console.log(JSON.stringify("cab".match(/(?<=a+)b/)));
console.log(JSON.stringify("b".match(/(?<=a+)b/)));
console.log(JSON.stringify("xxy".match(/(?<=x{1,3})y/)));
console.log(JSON.stringify("y".match(/(?<=x{1,3})y/)));
console.log(JSON.stringify("a. foo".match(/(?<!\.\s*)[a-z]+/g)));

// Zero repetitions, at the very start of the subject.
console.log(JSON.stringify("abc".match(/(?<=x*)a/)));
console.log(JSON.stringify("  a a".match(/(?<=\s*)a/g)));
console.log(JSON.stringify("aab".match(/(?<=^a*)b/)));
console.log(JSON.stringify("aaa".match(/(?<=a+)$/)));

// A resumed search looks BEFORE where it resumes, which is the whole point of
// a lookbehind and the place a prefix built from the resumption point onwards
// would answer differently.
const walked = /(?<=\.\s*)[a-z]+/g;
const steps = [];
let step;
while ((step = walked.exec(text)) !== null) {
  steps.push([step[0], step.index, walked.lastIndex]);
}
console.log(JSON.stringify(steps));
const jumped = /(?<=\.\s*)[a-z]+/g;
jumped.lastIndex = 5;
const landed = jumped.exec(text);
console.log(JSON.stringify([landed[0], landed.index, jumped.lastIndex]));
console.log(
  JSON.stringify([...text.matchAll(/(?<=\.\s*)[a-z]+/g)].map((m) => [m[0], m.index]))
);

// A group AFTER the lookbehind is group one.
console.log(JSON.stringify("a.  foo".match(/(?<=\.\s*)([a-z]+)/)));
const named = "a.  foo".match(/(?<=\.\s*)(?<word>[a-z]+)/);
console.log(JSON.stringify(named.groups.word));
