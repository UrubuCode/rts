// Cross-runtime: a `throw` inside `finally` REPLACES whatever completion was in
// flight — a pending exception, a pending return, or a pending break — and when
// finallys nest the outermost throw is the one that escapes.

const trace: string[] = [];
function tag(e: any): string {
  return e && typeof e === "object" && "tag" in e ? String(e.tag) : String(e);
}

// 1) The try's exception is discarded; the finally's is what the caller sees.
function replaceThrow(): void {
  try {
    trace.push("try");
    throw { tag: "from-try" };
  } finally {
    trace.push("finally");
    throw { tag: "from-finally" };
  }
}
try {
  replaceThrow();
  console.log("replace_throw=NOT-REACHED");
} catch (e) {
  console.log("replace_throw=" + tag(e));
}
console.log("replace_throw_trace=" + trace.join(","));

// 2) A pending RETURN is discarded the same way — the value never surfaces.
function replaceReturn(): string {
  try {
    return "returned";
  } finally {
    throw { tag: "over-return" };
  }
}
try {
  console.log("replace_return=NOT-REACHED:" + replaceReturn());
} catch (e) {
  console.log("replace_return=" + tag(e));
}

// 3) A throw raised by the CATCH is replaced too — the catch has already run.
function replaceCatchThrow(): void {
  try {
    throw { tag: "a" };
  } catch (e) {
    trace.push("catch:" + tag(e));
    throw { tag: "from-catch" };
  } finally {
    trace.push("finally2");
    throw { tag: "wins" };
  }
}
trace.length = 0;
try {
  replaceCatchThrow();
} catch (e) {
  console.log("replace_catch=" + tag(e));
}
console.log("replace_catch_trace=" + trace.join(","));

// 4) Nested finallys: the inner one throws, the outer one throws over it.
function nestedBoth(): void {
  try {
    try {
      throw { tag: "innermost" };
    } finally {
      trace.push("inner-finally");
      throw { tag: "inner-replacement" };
    }
  } finally {
    trace.push("outer-finally");
    throw { tag: "outer-replacement" };
  }
}
trace.length = 0;
try {
  nestedBoth();
} catch (e) {
  console.log("nested_both=" + tag(e));
}
console.log("nested_both_trace=" + trace.join(","));

// 5) An inner finally that throws is caught by an OUTER catch, and the original
//    exception is gone by then.
function innerThrowOuterCatch(): string {
  try {
    try {
      throw { tag: "original" };
    } finally {
      throw { tag: "substituted" };
    }
  } catch (e) {
    return tag(e);
  }
}
console.log("inner_throw_outer_catch=" + innerThrowOuterCatch());

// 6) A finally that throws while a BREAK is pending: the break never happens.
function breakThenThrow(): string {
  const seen: string[] = [];
  try {
    for (let i = 0; i < 3; i++) {
      try {
        seen.push("i" + i);
        if (i === 1) break;
      } finally {
        seen.push("f" + i);
        if (i === 1) throw { tag: "over-break" };
      }
    }
    seen.push("after-loop");
  } catch (e) {
    seen.push("caught:" + tag(e));
  }
  return seen.join(",");
}
console.log("break_then_throw=" + breakThenThrow());

// 7) A finally that throws nothing leaves the in-flight exception intact.
function passthrough(): void {
  try {
    throw { tag: "kept" };
  } finally {
    trace.push("quiet-finally");
  }
}
trace.length = 0;
try {
  passthrough();
} catch (e) {
  console.log("passthrough=" + tag(e));
}
console.log("passthrough_trace=" + trace.join(","));

// 8) The finally still runs when nothing is in flight, and its throw is the
//    only completion there is.
function throwFromNothing(): string {
  try {
    return "plain";
  } finally {
    if (trace.length >= 0) throw { tag: "unprovoked" };
  }
}
try {
  console.log("from_nothing=NOT-REACHED:" + throwFromNothing());
} catch (e) {
  console.log("from_nothing=" + tag(e));
}

// 9) Three levels: only the last finally's throw survives, and every level ran.
function threeLevels(): void {
  try {
    try {
      try {
        throw { tag: "L0" };
      } finally {
        trace.push("L0f");
        throw { tag: "L1" };
      }
    } finally {
      trace.push("L1f");
      throw { tag: "L2" };
    }
  } finally {
    trace.push("L2f");
    throw { tag: "L3" };
  }
}
trace.length = 0;
try {
  threeLevels();
} catch (e) {
  console.log("three_levels=" + tag(e));
}
console.log("three_levels_trace=" + trace.join(","));

// 10) A native error replaced by a finally: the caller sees the replacement's
//     constructor, never the original's.
function nativeReplaced(): void {
  try {
    (null as any).boom;
  } finally {
    throw new RangeError("replacement");
  }
}
try {
  nativeReplaced();
} catch (e) {
  console.log("native_replaced=" + (e as any).constructor.name);
  console.log("native_replaced_is_range=" + (e instanceof RangeError));
  console.log("native_replaced_is_type=" + (e instanceof TypeError));
}

// 11) A replacing throw inside a loop ends the loop: later iterations never run.
function loopStopsAtReplacement(): string {
  const seen: string[] = [];
  try {
    for (let i = 0; i < 4; i++) {
      try {
        seen.push("i" + i);
        if (i === 1) throw { tag: "inner" + i };
      } finally {
        seen.push("f" + i);
        if (i === 1) throw { tag: "replacement" };
      }
    }
  } catch (e) {
    seen.push("caught:" + tag(e));
  }
  return seen.join(",");
}
console.log("loop_stops=" + loopStopsAtReplacement());

// 12) Two sibling try/finally in sequence: the first one's replacement escapes
//     and the second block is never entered.
function siblingBlocks(): string {
  const seen: string[] = [];
  try {
    try {
      seen.push("first-try");
    } finally {
      seen.push("first-finally");
      throw { tag: "from-first" };
    }
    // The second block is unreachable once the first finally throws.
  } catch (e) {
    seen.push("caught:" + tag(e));
  }
  try {
    seen.push("second-try");
  } finally {
    seen.push("second-finally");
  }
  return seen.join(",");
}
console.log("sibling_blocks=" + siblingBlocks());

// 13) The thrown value keeps its identity and its own constructor.
const literalMarker = { tag: "literal" };
function identityKept(): string {
  try {
    try {
      throw new TypeError("original");
    } finally {
      throw literalMarker;
    }
  } catch (e) {
    return String(e === literalMarker) + ":" + (e as any).constructor.name;
  }
}
console.log("identity_kept=" + identityKept());

// 14) A finally attached to a try whose catch RETURNED: the return is pending
//     while the finally runs, and the finally's throw wins over it.
function catchReturnedThenThrow(): string {
  const seen: string[] = [];
  function inner(): string {
    try {
      throw { tag: "start" };
    } catch (e) {
      seen.push("catch-returns");
      return "catch-value";
    } finally {
      seen.push("finally-throws");
      throw { tag: "beats-return" };
    }
  }
  try {
    seen.push("got:" + inner());
  } catch (e) {
    seen.push("caught:" + tag(e));
  }
  return seen.join(",");
}
console.log("catch_returned_then_throw=" + catchReturnedThenThrow());
