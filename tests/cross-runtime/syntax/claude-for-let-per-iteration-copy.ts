// Cross-runtime: `for (let ...)` makes a FRESH binding per iteration.
// The update expression runs against the copy made for the next iteration, so a
// closure built in one iteration keeps that iteration's value; `var` does not.

const letFns: Array<() => number> = [];
for (let i = 0; i < 3; i++) letFns.push(() => i);
console.log("let_closures=" + letFns.map((f) => f()).join(","));

const varFns: Array<() => number> = [];
for (var v = 0; v < 3; v++) varFns.push(() => v);
console.log("var_closures=" + varFns.map((f) => f()).join(","));
console.log("var_after=" + v);

// The closure sees the value BEFORE the update expression ran.
const early: Array<() => number> = [];
for (let i = 10; i < 13; i++) early.push(() => i);
console.log("early=" + early.map((f) => f()).join(","));

// Mutating the binding inside the body IS visible to the closure of that same
// iteration, and does drive the loop forward.
const mutated: number[] = [];
for (let i = 0; i < 3; i++) {
  const f = () => i;
  i += 10;
  mutated.push(f());
}
console.log("body_mutation=" + mutated.join(","));

// Two declarators each get their own per-iteration copy.
const pairs: Array<() => string> = [];
for (let i = 0, j = 10; i < 3; i++, j--) pairs.push(() => i + ":" + j);
console.log("two_declarators=" + pairs.map((f) => f()).join("|"));

// The condition sees the copy the previous update wrote.
const seenByCond: number[] = [];
for (let i = 0; seenByCond.push(i) && i < 2; i++) { /* body empty */ }
console.log("cond_sees=" + seenByCond.join(","));

// `for-of` also binds per iteration.
const ofFns: Array<() => string> = [];
for (const c of ["a", "b", "c"]) ofFns.push(() => c);
console.log("for_of=" + ofFns.map((f) => f()).join(","));

// `for-in` with let binds per iteration too.
const inFns: Array<() => string> = [];
for (const k in { p: 1, q: 2 }) inFns.push(() => k);
console.log("for_in=" + inFns.map((f) => f()).join(","));

// Nested loops: each level has its own per-iteration binding.
const nested: Array<() => string> = [];
for (let a = 0; a < 2; a++) for (let b = 0; b < 2; b++) nested.push(() => a + "" + b);
console.log("nested=" + nested.map((f) => f()).join(","));

// A `let` binding declared in the body is separate from the loop's own binding.
const bodyLet: Array<() => number> = [];
for (let i = 0; i < 3; i++) {
  let doubled = i * 2;
  bodyLet.push(() => doubled);
  doubled = i * 2 + 1;
}
console.log("body_let=" + bodyLet.map((f) => f()).join(","));

// The loop binding is not visible after the loop.
console.log("typeof_after_let=" + typeof (globalThis as any).i);

// `var` in a body block is one binding shared by every iteration.
const varBody: Array<() => number> = [];
for (let i = 0; i < 3; i++) {
  var shared = i;
  varBody.push(() => shared);
}
console.log("var_body=" + varBody.map((f) => f()).join(","));

// Breaking out leaves the last copy intact for closures already made.
const beforeBreak: Array<() => number> = [];
for (let i = 0; i < 5; i++) {
  beforeBreak.push(() => i);
  if (i === 2) break;
}
console.log("before_break=" + beforeBreak.map((f) => f()).join(","));

// `continue` still runs the update against a fresh copy.
const withContinue: Array<() => number> = [];
for (let i = 0; i < 4; i++) {
  if (i === 1) continue;
  withContinue.push(() => i);
}
console.log("with_continue=" + withContinue.map((f) => f()).join(","));

// A closure made in the update expression itself sees the post-increment copy.
const fromUpdate: Array<() => number> = [];
for (let i = 0; i < 3; fromUpdate.push(() => i), i++) { /* body empty */ }
console.log("from_update=" + fromUpdate.map((f) => f()).join(","));

// Each iteration makes a distinct function object, not a shared one.
console.log("distinct_fns=" + (letFns[0] !== letFns[1] && letFns[1] !== letFns[2]));

// A `const` loop variable in `for-of` is a fresh binding each round, so a
// closure keeps its own element.
const constOf: Array<() => string> = [];
for (const item of ["p", "q", "r"]) constOf.push(() => item);
console.log("const_for_of=" + constOf.map((f) => f()).join(","));

// Destructuring in the head binds per iteration as well.
const destructured: Array<() => string> = [];
for (const [k, val] of [["a", 1], ["b", 2]] as Array<[string, number]>) {
  destructured.push(() => k + val);
}
console.log("destructured=" + destructured.map((f) => f()).join(","));

// A `while` loop has no per-iteration binding: one `let` outside it is shared.
const whileFns: Array<() => number> = [];
let w = 0;
while (w < 3) {
  whileFns.push(() => w);
  w++;
}
console.log("while_shared=" + whileFns.map((f) => f()).join(","));

// Declaring inside the while body restores per-iteration capture.
const whileBody: Array<() => number> = [];
let z = 0;
while (z < 3) {
  const snapshot = z;
  whileBody.push(() => snapshot);
  z++;
}
console.log("while_body_let=" + whileBody.map((f) => f()).join(","));

// Reassigning the loop variable inside a nested function affects the loop.
const log: number[] = [];
for (let i = 0; i < 6; i++) {
  const bump = () => { i += 1; };
  log.push(i);
  bump();
}
console.log("bumped=" + log.join(","));
