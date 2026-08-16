// Cross-runtime: when a for-of, a spread or a destructuring stops early or
// throws, the iterator's `return` is called exactly once — and when both the
// body and `return` throw, the BODY's error is the one that escapes.
const log: string[] = [];

function makeIterable(name: string, opts: any): any {
  let i = 0;
  return {
    [Symbol.iterator]() {
      log.push(name + ".iterator");
      return {
        next() {
          log.push(name + ".next" + i);
          if (opts.nextThrowsAt === i) {
            throw new RangeError("next-" + name);
          }
          i++;
          return i > 3 ? { value: undefined, done: true } : { value: name + i, done: false };
        },
        return(v: any) {
          log.push(name + ".return(" + String(v) + ")");
          if (opts.returnThrows) {
            throw new EvalError("return-" + name);
          }
          if (opts.returnBadResult) {
            return 1 as any;
          }
          return { value: "closed", done: true };
        },
      };
    },
  };
}

function probe(fn: () => any): string {
  try {
    const v = fn();
    return "ok:" + String(v);
  } catch (e: any) {
    return e.constructor.name;
  }
}

function reset(): void {
  log.length = 0;
}

// break closes the iterator.
reset();
console.log("break=" + probe(() => {
  const out: string[] = [];
  for (const v of makeIterable("b", {})) {
    out.push(v);
    if (out.length === 2) {
      break;
    }
  }
  return out.join(",");
}));
console.log("break-log=" + log.join("|"));

// return out of a for-of closes it too.
reset();
console.log("return=" + probe(() => {
  for (const v of makeIterable("r", {})) {
    return "returned-" + v;
  }
  return "never";
}));
console.log("return-log=" + log.join("|"));

// A throw in the body closes it, and the body's error is what escapes.
reset();
console.log("body-throw=" + probe(() => {
  for (const v of makeIterable("t", {})) {
    throw new URIError("body-" + v);
  }
  return "never";
}));
console.log("body-throw-log=" + log.join("|"));

// If `return` ALSO throws while unwinding a body throw, the body's error wins
// and the one from `return` is dropped.
reset();
console.log("both-throw=" + probe(() => {
  for (const v of makeIterable("x", { returnThrows: true })) {
    throw new URIError("body-" + v);
  }
  return "never";
}));
console.log("both-throw-log=" + log.join("|"));

// On a plain `break`, a throwing `return` is NOT swallowed.
reset();
console.log("break-return-throws=" + probe(() => {
  for (const v of makeIterable("y", { returnThrows: true })) {
    void v;
    break;
  }
  return "finished";
}));
console.log("break-return-throws-log=" + log.join("|"));

// `return` must answer an Object; a primitive result is a TypeError on break.
reset();
console.log("bad-return-result=" + probe(() => {
  for (const v of makeIterable("z", { returnBadResult: true })) {
    void v;
    break;
  }
  return "finished";
}));
console.log("bad-return-result-log=" + log.join("|"));

// An error from `next` itself does NOT trigger `return` — the iterator is
// already considered done.
reset();
console.log("next-throws=" + probe(() => {
  const out: string[] = [];
  for (const v of makeIterable("n", { nextThrowsAt: 2 })) {
    out.push(v);
  }
  return out.join(",");
}));
console.log("next-throws-log=" + log.join("|"));

// Array destructuring that takes fewer elements than are available closes it.
reset();
console.log("destructure-partial=" + probe(() => {
  const [first, second] = makeIterable("d", {});
  return first + "," + second;
}));
console.log("destructure-partial-log=" + log.join("|"));

// A rest element drains it, so there is nothing to close.
reset();
console.log("destructure-rest=" + probe(() => {
  const [head, ...rest] = makeIterable("e", {});
  return head + "+" + rest.join(",");
}));
console.log("destructure-rest-log=" + log.join("|"));

// A throwing default in a destructuring pattern closes the iterator.
reset();
console.log("destructure-default-throws=" + probe(() => {
  const [first, second = (() => {
    throw new SyntaxError("default");
  })()] = makeIterable("f", { nextThrowsAt: 5 });
  return String(first) + String(second);
}));
console.log("destructure-default-log=" + log.join("|"));

// Spread drains the iterator, so `return` is never reached.
reset();
console.log("spread=" + probe(() => [...makeIterable("s", {})].join(",")));
console.log("spread-log=" + log.join("|"));

// Array.from with a throwing mapper closes the iterator.
reset();
console.log("from-mapper-throws=" + probe(() => Array.from(makeIterable("m", {}), () => {
  throw new URIError("mapper");
})));
console.log("from-mapper-log=" + log.join("|"));

// A generator closes through its own finally on break.
const genLog: string[] = [];
function* guarded(): Generator<number, void, undefined> {
  try {
    genLog.push("g-1");
    yield 1;
    genLog.push("g-2");
    yield 2;
  } finally {
    genLog.push("g-finally");
  }
}
for (const v of guarded()) {
  genLog.push("body-" + v);
  break;
}
console.log("generator=" + genLog.join("|"));
