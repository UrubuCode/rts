// Cross-runtime: a finally block's abrupt completion REPLACES the in-flight one
// — return over return, throw over throw, break/continue over both — while a
// normal finally completion leaves the pending value untouched.
const log: string[] = [];

function returnOverReturn(): string {
  try {
    return "try";
  } finally {
    return "finally";
  }
}
console.log("return-over-return=" + returnOverReturn());

function returnOverThrow(): string {
  try {
    throw new RangeError("x");
  } finally {
    return "swallowed";
  }
}
console.log("return-over-throw=" + returnOverThrow());

function throwOverThrow(): string {
  try {
    try {
      throw new RangeError("first");
    } finally {
      throw new TypeError("second");
    }
  } catch (e: any) {
    return e.constructor.name;
  }
}
console.log("throw-over-throw=" + throwOverThrow());

function throwOverReturn(): string {
  try {
    (function (): string {
      try {
        return "value";
      } finally {
        throw new EvalError("late");
      }
    })();
    return "no-throw";
  } catch (e: any) {
    return e.constructor.name;
  }
}
console.log("throw-over-return=" + throwOverReturn());

// The return VALUE is evaluated before the finally runs.
function evaluatedFirst(): string {
  let v = "a";
  try {
    return v;
  } finally {
    v = "b";
    log.push("finally-ran");
  }
}
console.log("evaluated-first=" + evaluatedFirst());
console.log("finally-log=" + log.join("|"));

// break out of a finally cancels the pending return of the loop body.
function breakOut(): string {
  const out: string[] = [];
  for (let i = 0; i < 3; i = i + 1) {
    try {
      out.push("body" + i);
      if (i === 1) break;
    } finally {
      out.push("fin" + i);
    }
  }
  return out.join(",");
}
console.log("break=" + breakOut());

function breakInFinally(): string {
  const out: string[] = [];
  outer: for (let i = 0; i < 3; i = i + 1) {
    try {
      out.push("t" + i);
      continue outer;
    } finally {
      out.push("f" + i);
      if (i === 1) break outer;
    }
  }
  return out.join(",");
}
console.log("break-in-finally=" + breakInFinally());

function continueInFinally(): string {
  const out: string[] = [];
  for (let i = 0; i < 3; i = i + 1) {
    try {
      if (i === 1) throw new Error("skip");
      out.push("ok" + i);
    } finally {
      if (i === 1) {
        out.push("swallow");
        continue;
      }
      out.push("fin" + i);
    }
  }
  return out.join(",");
}
console.log("continue-in-finally=" + continueInFinally());

// Nested finally blocks run innermost-first as the stack unwinds.
function nested(): string {
  const out: string[] = [];
  try {
    try {
      try {
        throw new Error("deep");
      } finally {
        out.push("inner");
      }
    } finally {
      out.push("middle");
    }
  } catch (e: any) {
    out.push("caught");
  } finally {
    out.push("outer");
  }
  return out.join(",");
}
console.log("nested=" + nested());

// An optional catch binding, and a catch whose binding shadows an outer name.
const shadow = "outer";
function optionalBinding(): string {
  try {
    throw new Error("ignored");
  } catch {
    return "no-binding:" + shadow;
  }
}
console.log("optional=" + optionalBinding());

function shadowing(): string {
  try {
    throw "thrown";
  } catch (shadow: any) {
    return "inner=" + shadow;
  }
}
console.log("shadowing=" + shadowing() + ",outer=" + shadow);

// The catch parameter can be destructured.
function destructured(): string {
  try {
    throw { code: 7, detail: { tag: "t" } };
  } catch ({ code, detail: { tag } }: any) {
    return code + ":" + tag;
  }
}
console.log("destructured=" + destructured());

// A throw of a non-Error keeps the value verbatim.
function nonError(v: any): string {
  try {
    throw v;
  } catch (e: any) {
    return typeof e + ":" + String(e);
  }
}
console.log("throw-string=" + nonError("s"));
console.log("throw-number=" + nonError(0));
console.log("throw-null=" + nonError(null));
console.log("throw-undefined=" + nonError(undefined));

// finally runs even when the try completes normally, and its value is dropped.
function normalCompletion(): string {
  const out: string[] = [];
  const r = (function (): string {
    try {
      out.push("try");
      return "kept";
    } finally {
      out.push("fin");
      "dropped";
    }
  })();
  return r + "/" + out.join(",");
}
console.log("normal=" + normalCompletion());

// A rethrow inside catch is caught by the enclosing try, after the finally.
function rethrow(): string {
  const out: string[] = [];
  try {
    try {
      throw new TypeError("a");
    } catch (e: any) {
      out.push("catch:" + e.constructor.name);
      throw new RangeError("b");
    } finally {
      out.push("fin");
    }
  } catch (e: any) {
    out.push("outer:" + e.constructor.name);
  }
  return out.join("|");
}
console.log("rethrow=" + rethrow());

// A finally around a labelled block, and a return inside a switch under finally.
function labelledBlock(): string {
  const out: string[] = [];
  block: {
    try {
      out.push("in");
      break block;
    } finally {
      out.push("fin");
    }
  }
  out.push("after");
  return out.join(",");
}
console.log("labelled=" + labelledBlock());

function switchUnderFinally(v: number): string {
  try {
    switch (v) {
      case 1:
        return "one";
      default:
        return "other";
    }
  } finally {
    log.push("switch-fin" + v);
  }
}
console.log("switch-1=" + switchUnderFinally(1));
console.log("switch-2=" + switchUnderFinally(2));
console.log("switch-log=" + log.join("|"));

// try/finally inside a for-of runs the finally on each iteration and on break.
function loopFinally(): string {
  const out: string[] = [];
  for (const v of [1, 2, 3]) {
    try {
      if (v === 3) break;
      out.push("v" + v);
    } finally {
      out.push("f" + v);
    }
  }
  return out.join(",");
}
console.log("loop=" + loopFinally());

// A finally that neither returns nor throws leaves the caught value intact.
function passthrough(): string {
  try {
    throw new URIError("kept");
  } catch (e: any) {
    return e.constructor.name;
  } finally {
    "ignored";
  }
}
console.log("passthrough=" + passthrough());

// The catch parameter is a fresh binding per entry, not shared across calls.
function fresh(v: string): string {
  try {
    throw v;
  } catch (e: any) {
    return e;
  }
}
console.log("fresh=" + fresh("a") + "," + fresh("b"));
