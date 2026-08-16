// Cross-runtime: `do...while` runs its body BEFORE the first test, its
// condition sees every mutation the body made, `continue` jumps to the
// condition rather than out, and a `let` in the body is a fresh binding on
// every pass.

// 1) A condition that is false from the start still gets one pass.
const once: string[] = [];
do {
  once.push("ran");
} while (false);
console.log("false_condition_runs_once=" + once.join(","));

// 2) The condition reads what the body just wrote.
let n = 0;
const steps: number[] = [];
do {
  n += 1;
  steps.push(n);
} while (n < 4);
console.log("condition_sees_body=" + steps.join(",") + "|final=" + n);

// 3) The condition is evaluated AFTER the body, once per pass — one fewer time
//    than a `while` loop over the same counter would suggest.
const order: string[] = [];
let i = 0;
do {
  order.push("body" + i);
  i += 1;
} while (order.push("test" + i) && i < 3);
console.log("eval_order=" + order.join(","));

// 4) `continue` goes to the condition, not out of the loop.
let c = 0;
const visited: string[] = [];
do {
  c += 1;
  if (c % 2 === 1) continue;
  visited.push("even" + c);
} while (c < 6);
console.log("continue_to_condition=" + visited.join(",") + "|c=" + c);

// 5) `break` skips the condition entirely.
let b = 0;
const tested: string[] = [];
do {
  b += 1;
  if (b === 2) break;
} while (tested.push("t" + b) > 0);
console.log("break_skips_test=" + tested.join(",") + "|b=" + b);

// 6) A `let` declared in the body is a new binding each pass, so closures made
//    there are independent.
const fns: Array<() => number> = [];
let k = 0;
do {
  let captured = k;
  fns.push(() => captured);
  k += 1;
} while (k < 3);
console.log("let_per_pass=" + fns.map((f) => f()).join(","));

// 7) A `var` in the body is one binding for the whole function, so every
//    closure agrees on the last value.
function varInBody(): string {
  const vfns: Array<() => number> = [];
  let j = 0;
  do {
    var shared = j;
    vfns.push(() => shared);
    j += 1;
  } while (j < 3);
  return vfns.map((f) => f()).join(",") + "|shared=" + shared;
}
console.log("var_in_body=" + varInBody());

// 8) A condition that MUTATES lives in claude2-loop-condition-assignment.ts.
// It is kept apart because an engine that loses the assignment across the back
// edge loops forever, and a hang here would hide everything below it.

// 9) Nested do-while: the inner condition runs to completion each outer pass.
const grid: string[] = [];
let oi = 0;
do {
  let ij = 0;
  do {
    grid.push(oi + "" + ij);
    ij += 1;
  } while (ij < 2);
  oi += 1;
} while (oi < 2);
console.log("nested=" + grid.join(","));

// 10) A labelled continue targeting the OUTER do-while.
const labelled: string[] = [];
let li = 0;
outer: do {
  li += 1;
  let lj = 0;
  do {
    lj += 1;
    if (lj === 2) continue outer;
    labelled.push(li + ":" + lj);
  } while (lj < 4);
  labelled.push("inner-finished" + li);
} while (li < 3);
console.log("labelled_continue=" + labelled.join(","));

// 11) A labelled break out of both loops.
const broken: string[] = [];
let bi = 0;
top: do {
  bi += 1;
  let bj = 0;
  do {
    bj += 1;
    broken.push(bi + "" + bj);
    if (bi === 2 && bj === 2) break top;
  } while (bj < 3);
} while (bi < 5);
console.log("labelled_break=" + broken.join(",") + "|bi=" + bi);

// 12) `return` from inside the body leaves immediately; the condition never
//     runs again.
function returnsFromBody(): string {
  const log: string[] = [];
  let r = 0;
  do {
    r += 1;
    if (r === 2) return log.join(",") + "|left-at=" + r;
    log.push("pass" + r);
  } while (log.push("cond" + r) > 0);
  return "unreachable";
}
console.log("return_from_body=" + returnsFromBody());

// 13) An exception thrown in the body skips the condition; a finally still runs.
function throwsInBody(): string {
  const log: string[] = [];
  let t = 0;
  try {
    do {
      t += 1;
      try {
        if (t === 2) throw new RangeError("stop");
        log.push("ok" + t);
      } finally {
        log.push("fin" + t);
      }
    } while (log.push("cond" + t) > 0);
  } catch (e) {
    log.push("caught:" + (e as any).constructor.name);
  }
  return log.join(",");
}
console.log("throw_in_body=" + throwsInBody());

// 14) The body's own block scope: a `const` declared there can be re-declared
//     on the next pass with a different value.
const constPerPass: string[] = [];
let p = 0;
do {
  const label = "p" + p;
  constPerPass.push(label);
  p += 1;
} while (p < 3);
console.log("const_per_pass=" + constPerPass.join(","));

// 15) A do-while whose body is a single statement, with the condition reading
//     the mutation that statement made.
let s = 1;
do s *= 3;
while (s < 50);
console.log("single_statement_body=" + s);

// 16) The condition is coerced to boolean, so a non-empty string keeps looping
//     and an empty one stops.
let text = "abcd";
const trimmed: string[] = [];
do {
  trimmed.push(text);
  text = text.slice(1);
} while (text);
console.log("truthy_condition=" + trimmed.join(",") + "|left=" + JSON.stringify(text));

// 17) A do-while inside a for-of, with continue on the inner loop only.
const outerSeen: string[] = [];
for (const word of ["ab", "c"]) {
  let idx = 0;
  do {
    idx += 1;
    if (idx > word.length) continue;
    outerSeen.push(word + idx);
  } while (idx < word.length);
}
console.log("inside_for_of=" + outerSeen.join(","));

// 18) The body's binding is out of scope in the condition, so a `let` declared
//     in the body cannot be tested — the condition reads the outer name.
let shadowed = "outer";
const shadowSeen: string[] = [];
let guard = 0;
do {
  let shadowed = "inner" + guard;
  shadowSeen.push(shadowed);
  guard += 1;
} while (shadowed === "outer" && guard < 3);
console.log("condition_scope=" + shadowSeen.join(",") + "|outer=" + shadowed);

// 19) An empty body is legal and the condition alone drives the loop.
let e = 0;
do {} while ((e += 1) < 4);
console.log("empty_body=" + e);

// 20) The condition's own side effects happen even on the pass that ends it.
const tests: string[] = [];
let q = 0;
do {
  q += 1;
} while (tests.push("t" + q) && q < 3);
console.log("last_test_runs=" + tests.join(",") + "|q=" + q);

// 21) A do-while as the body of an `if`, with no braces around it.
let d = 0;
if (true) do d += 5; while (d < 12);
console.log("as_if_body=" + d);
