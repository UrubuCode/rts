// Cross-runtime: Error subclass name/message/cause without stack exactness.
class MyError extends Error {
  constructor(message: string, cause?: unknown) {
    super(message, { cause });
    this.name = "MyError";
  }
}

const root = new Error("root");
const err = new MyError("top", root);
console.log(err.name);
console.log(err.message);
console.log((err as any).cause === root);
console.log(err instanceof Error);
console.log(typeof err.stack === "string");
