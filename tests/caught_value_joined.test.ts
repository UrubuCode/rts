import { describe, test, expect } from "rts:test";

// What a `catch` receives is whatever was thrown, which nothing in the function
// decides. Joined with a value the function does know -- `e && typeof e === "object"`
// -- it stays unknown; typed as nothing, it answered the known side, and the
// function was compiled as if the caught value were a boolean.

function tag(e: any): string {
  return e && typeof e === "object" && "tag" in e ? String(e.tag) : String(e);
}

function caughtThenJoined(thrown: any): string {
  try {
    throw thrown;
  } catch (e) {
    return tag(e) + "|" + (e && typeof e === "object");
  }
}

function finallyReplaces(): string {
  const trace: string[] = [];
  try {
    try {
      throw { tag: "a" };
    } catch (e) {
      trace.push("catch:" + tag(e));
      throw { tag: "from-catch" };
    } finally {
      trace.push("finally");
      throw { tag: "wins" };
    }
  } catch (e) {
    return tag(e) + "|" + trace.join(",");
  }
}

describe("a caught value joined with a known one", () => {
  test("objects and primitives", () => {
    expect(caughtThenJoined({ tag: "t" })).toBe("t|true");
    expect(caughtThenJoined(0)).toBe("0|0");
    expect(caughtThenJoined("s")).toBe("s|false");
  });
  test("a finally that replaces the exception", () => {
    expect(finallyReplaces()).toBe("wins|catch:a,finally");
  });
});
