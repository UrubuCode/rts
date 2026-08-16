// Cross-runtime: labels crossing a `switch` boundary.
// `break` inside a switch belongs to the switch; a LABELLED break/continue
// reaches past it to the labelled loop or block.

const trace: string[] = [];

outer: for (let i = 0; i < 3; i++) {
  inner: for (let j = 0; j < 3; j++) {
    switch (i * 10 + j) {
      case 1:
        trace.push("continue_outer@" + i + j);
        continue outer;
      case 11:
        trace.push("break_inner@" + i + j);
        break inner;
      case 21:
        trace.push("break_outer@" + i + j);
        break outer;
      default:
        trace.push("v" + i + j);
        break; // this one belongs to the switch
    }
    trace.push("after_switch@" + i + j);
  }
  trace.push("end_i@" + i);
}
console.log("nested=" + trace.join(","));

// An unlabelled `break` inside a switch inside a loop only leaves the switch.
const plain: string[] = [];
for (let i = 0; i < 3; i++) {
  switch (i) {
    case 1:
      plain.push("hit" + i);
      break;
    default:
      plain.push("def" + i);
  }
  plain.push("tail" + i);
}
console.log("plain_break=" + plain.join(","));

// `continue` targeting the loop that CONTAINS the switch, unlabelled.
const cont: string[] = [];
for (let i = 0; i < 4; i++) {
  switch (i % 2) {
    case 0:
      continue;
    default:
      cont.push("odd" + i);
  }
  cont.push("tail" + i);
}
console.log("unlabelled_continue=" + cont.join(","));

// A label on a plain block: `break label` jumps to just after the block.
const blockTrace: string[] = [];
block: {
  blockTrace.push("in");
  if (blockTrace.length === 1) break block;
  blockTrace.push("unreachable");
}
blockTrace.push("out");
console.log("labelled_block=" + blockTrace.join(","));

// A labelled block wrapping a switch.
const wrapped: string[] = [];
wrap: {
  switch ("x") {
    case "x":
      wrapped.push("case_x");
      break wrap;
    default:
      wrapped.push("default");
  }
  wrapped.push("after_switch");
}
wrapped.push("after_wrap");
console.log("break_out_of_block=" + wrapped.join(","));

// A label on a `while`, continued from inside a switch inside a nested block.
const wh: string[] = [];
let n = 0;
loop: while (n < 5) {
  n++;
  {
    switch (n % 3) {
      case 0:
        continue loop;
      case 1:
        wh.push("one:" + n);
        break;
      default:
        wh.push("two:" + n);
    }
  }
  wh.push("bottom:" + n);
}
console.log("while_label=" + wh.join(","));

// A label on a `do-while`.
const dw: string[] = [];
let m = 0;
dloop: do {
  m++;
  if (m === 2) continue dloop;
  if (m === 4) break dloop;
  dw.push("m" + m);
} while (m < 10);
console.log("do_while_label=" + dw.join(",") + " m=" + m);

// A labelled block nested inside a labelled loop: each label reaches its own
// statement.
const two: string[] = [];
loops: for (let i = 0; i < 4; i++) {
  guard: {
    if (i === 1) break guard;
    if (i === 3) break loops;
    two.push("kept" + i);
  }
  two.push("bottom" + i);
}
console.log("nested_labels=" + two.join(","));

// `continue label` from a `for-of` inside a switch inside a `for-in`.
const combo: string[] = [];
keys: for (const k in { a: 0, b: 0, c: 0 }) {
  for (const v of [1, 2, 3]) {
    switch (k + v) {
      case "a2":
        combo.push("skip_a");
        continue keys;
      case "b1":
        combo.push("skip_b1");
        continue;
      case "c3":
        combo.push("stop");
        break keys;
      default:
        combo.push(k + v);
    }
  }
  combo.push("done:" + k);
}
console.log("for_of_in_switch=" + combo.join(","));

// `try`/`finally` still runs when a labelled break leaves the loop.
const fin: string[] = [];
esc: for (let i = 0; i < 3; i++) {
  try {
    switch (i) {
      case 1:
        fin.push("break@" + i);
        break esc;
      default:
        fin.push("body" + i);
    }
  } finally {
    fin.push("finally" + i);
  }
}
console.log("finally_on_labelled_break=" + fin.join(","));

// A labelled `continue` from inside a `try` runs the `finally` and continues.
const fin2: string[] = [];
cloop: for (let i = 0; i < 3; i++) {
  try {
    if (i === 1) continue cloop;
    fin2.push("kept" + i);
  } finally {
    fin2.push("f" + i);
  }
  fin2.push("tail" + i);
}
console.log("finally_on_labelled_continue=" + fin2.join(","));

// The label itself is not a binding: an ordinary variable may share its name.
const outerName = "variable";
outerName2: for (let i = 0; i < 1; i++) {
  break outerName2;
}
console.log("label_not_binding=" + outerName);

// A labelled loop inside a function: `return` from a case leaves everything.
function findPair(target: number): string {
  search: for (let i = 1; i <= 4; i++) {
    for (let j = 1; j <= 4; j++) {
      switch (i * j) {
        case 0:
          continue search;
        default:
          if (i * j === target) return "found " + i + "x" + j;
          if (i * j > target) continue search;
      }
    }
  }
  return "none";
}
console.log("find6=" + findPair(6));
console.log("find9=" + findPair(9));
console.log("find7=" + findPair(7));
console.log("find16=" + findPair(16));

// A labelled block used as an early-exit guard chain.
function classify(n: number): string {
  let out = "?";
  check: {
    if (n < 0) { out = "negative"; break check; }
    if (n === 0) { out = "zero"; break check; }
    switch (n % 2) {
      case 0:
        out = "even";
        break check;
      default:
        out = "odd";
    }
    out = out + "+checked";
  }
  return out;
}
console.log("classify=" + [-3, 0, 4, 5].map(classify).join(","));

// `switch` on a value produced by a function, with fallthrough into a labelled
// break, showing which cases ran.
function pick(x: number): string {
  const hits: string[] = [];
  sw: switch (x) {
    case 0:
      hits.push("zero");
    // falls through
    case 1:
      hits.push("one");
      if (x === 0) break sw;
    // falls through
    case 2:
      hits.push("two");
      break;
    default:
      hits.push("other");
  }
  return hits.join(">");
}
console.log("fall0=" + pick(0));
console.log("fall1=" + pick(1));
console.log("fall2=" + pick(2));
console.log("fall9=" + pick(9));
