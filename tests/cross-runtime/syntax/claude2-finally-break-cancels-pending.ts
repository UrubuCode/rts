// Cross-runtime: an abrupt completion started INSIDE a `finally` — `break`,
// `continue` or `return` — cancels whatever was already in flight. A pending
// exception simply disappears, and so does a pending return value.

function tag(e: any): string {
  return e && typeof e === "object" && "tag" in e ? String(e.tag) : String(e);
}

// 1) `break` in a finally swallows the exception raised in the try.
function breakSwallows(): string {
  const seen: string[] = [];
  for (let i = 0; i < 3; i++) {
    try {
      seen.push("try" + i);
      throw { tag: "boom" + i };
    } finally {
      seen.push("fin" + i);
      break;
    }
  }
  seen.push("after");
  return seen.join(",");
}
console.log("break_swallows=" + breakSwallows());

// 2) `continue` in a finally swallows every iteration's exception.
function continueSwallows(): string {
  const seen: string[] = [];
  for (let i = 0; i < 3; i++) {
    try {
      throw { tag: "x" + i };
    } finally {
      seen.push("fin" + i);
      continue;
    }
  }
  seen.push("loop-ended-normally");
  return seen.join(",");
}
console.log("continue_swallows=" + continueSwallows());

// 3) `return` in a finally swallows the exception and supplies the value.
function returnSwallows(): string {
  try {
    throw { tag: "never-seen" };
  } finally {
    return "from-finally";
  }
}
console.log("return_swallows=" + returnSwallows());

// 4) `return` in a finally also replaces an earlier return's VALUE, and the
//    earlier expression was still evaluated.
const evaluated: string[] = [];
function returnReplacesReturn(): string {
  try {
    evaluated.push("try-expr");
    return "try-value";
  } finally {
    evaluated.push("finally-expr");
    return "finally-value";
  }
}
console.log("return_replaces=" + returnReplacesReturn());
console.log("return_replaces_order=" + evaluated.join(","));

// 5) A LABELLED break from inside a finally leaves the outer loop and still
//    discards the exception.
function labelledBreak(): string {
  const seen: string[] = [];
  outer: for (let i = 0; i < 3; i++) {
    for (let j = 0; j < 3; j++) {
      try {
        if (i === 1 && j === 1) throw { tag: "deep" };
        seen.push(i + "" + j);
      } finally {
        if (i === 1 && j === 1) {
          seen.push("fin");
          break outer;
        }
      }
    }
  }
  seen.push("end");
  return seen.join(",");
}
console.log("labelled_break=" + labelledBreak());

// 6) `break` out of a labelled BLOCK from inside a finally.
function labelledBlockBreak(): string {
  const seen: string[] = [];
  block: {
    try {
      seen.push("in-block");
      throw { tag: "blocked" };
    } finally {
      seen.push("fin");
      break block;
    }
  }
  seen.push("after-block");
  return seen.join(",");
}
console.log("labelled_block_break=" + labelledBlockBreak());

// 7) The exception is only cancelled by the finally that completes abruptly —
//    an inner quiet finally lets it through to the outer one that breaks.
function onlyTheAbruptOneCancels(): string {
  const seen: string[] = [];
  for (let i = 0; i < 2; i++) {
    try {
      try {
        throw { tag: "inner" + i };
      } finally {
        seen.push("quiet" + i);
      }
    } finally {
      seen.push("abrupt" + i);
      continue;
    }
  }
  seen.push("survived");
  return seen.join(",");
}
console.log("only_abrupt_cancels=" + onlyTheAbruptOneCancels());

// 8) A finally that breaks while a RETURN is pending: the function keeps going
//    past the loop and returns something else entirely.
function breakOverReturn(): string {
  for (let i = 0; i < 3; i++) {
    try {
      return "early" + i;
    } finally {
      break;
    }
  }
  return "fell-through";
}
console.log("break_over_return=" + breakOverReturn());

// 9) `continue` in a finally over a pending return: every iteration runs.
function continueOverReturn(): string {
  const seen: string[] = [];
  for (let i = 0; i < 3; i++) {
    try {
      seen.push("want-return" + i);
      return "never";
    } finally {
      continue;
    }
  }
  return seen.join(",") + "|fell-through";
}
console.log("continue_over_return=" + continueOverReturn());

// 10) The cancelled exception is really gone: an outer catch never sees it.
function outerCatchSeesNothing(): string {
  let caught = "none";
  try {
    for (let i = 0; i < 1; i++) {
      try {
        throw { tag: "vanishes" };
      } finally {
        break;
      }
    }
  } catch (e) {
    caught = tag(e);
  }
  return caught;
}
console.log("outer_catch_sees=" + outerCatchSeesNothing());

// 11) A while loop, same rule, with the condition re-evaluated after continue.
function whileContinue(): string {
  const seen: string[] = [];
  let n = 0;
  while (n < 3) {
    try {
      n++;
      throw { tag: "w" + n };
    } finally {
      seen.push("f" + n);
      continue;
    }
  }
  return seen.join(",") + "|n=" + n;
}
console.log("while_continue=" + whileContinue());

// 12) do-while: the finally's continue jumps to the CONDITION, not the body.
function doWhileContinue(): string {
  const seen: string[] = [];
  let n = 0;
  do {
    try {
      seen.push("body" + n);
      throw { tag: "d" };
    } finally {
      n++;
      continue;
    }
  } while (n < 3);
  return seen.join(",") + "|n=" + n;
}
console.log("do_while_continue=" + doWhileContinue());

// 13) `for-of` with a finally that breaks: the iteration stops and the pending
//     exception is gone.
function forOfBreak(): string {
  const seen: string[] = [];
  for (const v of ["a", "b", "c"]) {
    try {
      seen.push("saw:" + v);
      if (v === "b") throw { tag: "at-b" };
    } finally {
      if (v === "b") {
        seen.push("fin-b");
        break;
      }
    }
  }
  seen.push("done");
  return seen.join(",");
}
console.log("for_of_break=" + forOfBreak());

// 14) `for-in` with a finally that continues: every key is visited even though
//     each iteration threw.
function forInContinue(): string {
  const seen: string[] = [];
  const src: any = { p: 1, q: 2 };
  for (const k in src) {
    try {
      throw { tag: k };
    } finally {
      seen.push("k:" + k);
      continue;
    }
  }
  return seen.join(",") + "|survived";
}
console.log("for_in_continue=" + forInContinue());

// 15) A finally whose `return` cancels an exception raised in the CATCH.
function returnCancelsCatchThrow(): string {
  try {
    throw { tag: "one" };
  } catch (e) {
    throw { tag: "two" };
  } finally {
    return "finally-return-wins";
  }
}
console.log("return_cancels_catch=" + returnCancelsCatchThrow());

// 16) Only the innermost abrupt completion counts: the outer finally runs
//     afterwards and, being quiet, changes nothing.
function innerBreakOuterQuiet(): string {
  const seen: string[] = [];
  try {
    for (let i = 0; i < 3; i++) {
      try {
        throw { tag: "i" + i };
      } finally {
        seen.push("inner" + i);
        break;
      }
    }
    seen.push("after-loop");
  } finally {
    seen.push("outer-finally");
  }
  return seen.join(",");
}
console.log("inner_break_outer_quiet=" + innerBreakOuterQuiet());

// 17) The value a finally returns is computed inside the finally, so it sees
//     mutations made by the try before it threw.
function finallyReturnSeesMutation(): string {
  let state = "initial";
  try {
    state = "mutated";
    throw { tag: "ignored" };
  } finally {
    return "state=" + state;
  }
}
console.log("finally_return_sees=" + finallyReturnSeesMutation());

// 18) A pending break cancelled by a finally's `return`.
function returnOverBreak(): string {
  for (let i = 0; i < 3; i++) {
    try {
      break;
    } finally {
      return "returned-instead-of-breaking";
    }
  }
  return "loop-ended";
}
console.log("return_over_break=" + returnOverBreak());

// 19) Nothing pending at all: the finally's break is the only completion.
function nothingPending(): string {
  const seen: string[] = [];
  for (let i = 0; i < 3; i++) {
    try {
      seen.push("i" + i);
    } finally {
      if (i === 1) break;
    }
  }
  return seen.join(",");
}
console.log("nothing_pending=" + nothingPending());

// 20) A labelled continue from inside a finally jumps to the outer loop.
function labelledContinue(): string {
  const seen: string[] = [];
  outer: for (let i = 0; i < 3; i++) {
    for (let j = 0; j < 3; j++) {
      try {
        if (j === 1) throw { tag: "skip" };
        seen.push(i + "" + j);
      } finally {
        if (j === 1) continue outer;
      }
    }
    seen.push("inner-done" + i);
  }
  return seen.join(",");
}
console.log("labelled_continue=" + labelledContinue());
