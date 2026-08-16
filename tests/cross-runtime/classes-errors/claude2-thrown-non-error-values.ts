// Cross-runtime: `throw` accepts ANY value. The catch binding receives the
// identical value, `instanceof Error` is false for all of them, and every layer
// that rethrows hands on the same reference — including through a finally and
// across a promise rejection.
function classify(v: any): string {
  try {
    throw v;
  } catch (e: any) {
    const same = e === v || (typeof v === "number" && Number.isNaN(v) && Number.isNaN(e));
    return typeof e + "/" + String(same) + "/" + (e instanceof Error);
  }
}

console.log("string=" + classify("plain"));
console.log("number=" + classify(42));
console.log("zero=" + classify(0));
console.log("nan=" + classify(NaN));
console.log("boolean=" + classify(false));
console.log("null=" + classify(null));
console.log("undefined=" + classify(undefined));
console.log("bigint=" + classify(10n));
console.log("symbol=" + classify(Symbol("s")));
console.log("object=" + classify({ a: 1 }));
console.log("array=" + classify([1, 2]));
console.log("function=" + classify(function named(): void { }));
console.log("class=" + classify(class Thrown { }));
console.log("frozen=" + classify(Object.freeze({ f: 1 })));
console.log("error=" + classify(new TypeError("t")));

// An error-LIKE object is not an Error: only the prototype chain decides.
const errorLike: any = { name: "TypeError", message: "looks-real", stack: "fake" };
console.log("errorlike-instanceof=" + (errorLike instanceof Error));
console.log("errorlike-tag=" + Object.prototype.toString.call(errorLike));
console.log("errorlike-ctor=" + errorLike.constructor.name);
console.log("errorlike-tostring=" + Error.prototype.toString.call(errorLike));

// Reading `.constructor.name` off a thrown primitive works through the wrapper
// prototype, but null and undefined have none.
function nameOf(v: any): string {
  try {
    throw v;
  } catch (e: any) {
    if (e === null || e === undefined) {
      return "no-constructor";
    }
    return e.constructor.name;
  }
}
console.log("name-string=" + nameOf("s"));
console.log("name-number=" + nameOf(1));
console.log("name-bigint=" + nameOf(1n));
console.log("name-symbol=" + nameOf(Symbol("s")));
console.log("name-null=" + nameOf(null));
console.log("name-undefined=" + nameOf(undefined));
console.log("name-array=" + nameOf([]));

// Identity survives an arbitrary number of rethrowing frames.
const token: any = { id: "token" };
function level3(): void {
  throw token;
}
function level2(): void {
  try {
    level3();
  } catch (e) {
    throw e;
  }
}
function level1(): string {
  try {
    level2();
    return "never";
  } catch (e: any) {
    return String(e === token) + ":" + e.id;
  }
}
console.log("rethrow-identity=" + level1());

// A finally that runs on the way out does not disturb the value in flight.
const trace: string[] = [];
function throughFinally(): string {
  try {
    try {
      throw token;
    } finally {
      trace.push("inner-finally");
    }
  } catch (e: any) {
    trace.push("caught");
    return String(e === token);
  } finally {
    trace.push("outer-finally");
  }
}
console.log("through-finally=" + throughFinally());
console.log("through-finally-trace=" + trace.join(">"));

// An optional catch binding still runs, and a destructuring binding pulls the
// value apart.
function optional(): string {
  try {
    throw token;
  } catch {
    return "no-binding";
  }
}
function destructured(): string {
  try {
    throw { code: 7, detail: "d" };
  } catch ({ code, detail }: any) {
    return code + ":" + detail;
  }
}
function destructuredWithDefault(): string {
  try {
    throw {};
  } catch ({ code = "fallback" }: any) {
    return String(code);
  }
}
console.log("optional-binding=" + optional());
console.log("destructured=" + destructured());
console.log("destructured-default=" + destructuredWithDefault());

// Destructuring a thrown primitive fails inside the catch itself.
function destructuringFails(): string {
  try {
    try {
      throw null;
    } catch ({ code }: any) {
      return "unreachable:" + String(code);
    }
  } catch (e: any) {
    return "secondary:" + e.constructor.name;
  }
}
console.log("destructure-null=" + destructuringFails());

// The catch parameter is its own binding: assigning to it does not leak.
let outerName = "outer";
function shadowing(): string {
  try {
    throw "thrown";
  } catch (outerName: any) {
    outerName = "reassigned";
    return outerName;
  }
}
console.log("shadowing=" + shadowing() + "/" + outerName);

// A rejected promise carries the value with the same indifference.
const rejections: string[] = [];
Promise.reject("a-string")
  .catch((e: any) => {
    rejections.push(typeof e + ":" + String(e === "a-string") + ":" + (e instanceof Error));
    return Promise.reject(token);
  })
  .catch((e: any) => {
    rejections.push(typeof e + ":" + String(e === token));
    return Promise.reject(undefined);
  })
  .catch((e: any) => {
    rejections.push(String(e) + ":" + (e === undefined));
  })
  .then(() => {
    console.log("rejections=" + rejections.join("|"));
    console.log("tail=reached");
  });
console.log("sync-end=true");
