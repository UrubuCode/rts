// Cross-runtime: error handling patterns.

// Error subclassing
class AppError extends Error {
  constructor(message: string, public code: number) {
    super(message);
    this.name = "AppError";
    if (Error.captureStackTrace) Error.captureStackTrace(this, AppError);
  }
}
class NetworkError extends AppError {
  constructor(message: string, public statusCode: number) {
    super(message, statusCode);
    this.name = "NetworkError";
  }
}

const e = new NetworkError("Not Found", 404);
console.log(e.name);
console.log(e.message);
console.log(e.code);
console.log(e.statusCode);
console.log(e instanceof NetworkError);
console.log(e instanceof AppError);
console.log(e instanceof Error);

// try/catch/finally execution order
const order: string[] = [];
function risky(fail: boolean): string {
  try {
    order.push("try");
    if (fail) throw new Error("fail");
    order.push("after throw");
    return "ok";
  } catch (e: any) {
    order.push("catch:" + e.message);
    return "caught";
  } finally {
    order.push("finally");
  }
}
const r1 = risky(false);
console.log(r1);               // ok
console.log(order.join(","));  // try,after throw,finally

order.length = 0;
const r2 = risky(true);
console.log(r2);               // caught
console.log(order.join(","));  // try,catch:fail,finally

// re-throw pattern
function parse(s: string): number {
  try {
    const n = JSON.parse(s);
    if (typeof n !== "number") throw new TypeError("not a number");
    return n;
  } catch (e) {
    if (e instanceof SyntaxError) throw new AppError("invalid json: " + s, 400);
    throw e;
  }
}
try { parse("{bad"); } catch (e: any) { console.log(e.name + ":" + e.message); }
try { parse('"hello"'); } catch (e: any) { console.log(e.name + ":" + e.message); }
console.log(parse("42"));

// AggregateError
try {
  throw new AggregateError([new Error("e1"), new Error("e2")], "multiple errors");
} catch (e: any) {
  console.log(e.message);
  console.log(e.errors.map((x: Error) => x.message).join(","));
}
