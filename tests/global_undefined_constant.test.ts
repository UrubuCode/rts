import { describe, test, expect } from "rts:test";

// `undefined` read as a global is a constant -- the global object's property is
// non-writable and non-configurable -- and a binding of that name, which the
// language allows outside strict code's reserved words, is still read as itself.

function missed(): number {
  const m = new Map<string, number>([["a", 1]]);
  let n = 0;
  for (let i = 0; i < 4; i++) if (m.get(i < 2 ? "a" : "zz") === undefined) n++;
  return n;
}

function shadowedHere(): unknown {
  const undefined = 5;
  return undefined;
}

function shadowedAround(): unknown {
  const undefined = "outer";
  function inner(): unknown {
    return undefined;
  }
  return inner();
}

function writeIgnored(): unknown {
  try {
    (globalThis as any).undefined = 7;
  } catch (e) {
    // A module is strict, where the write raises; either way nothing changes.
  }
  return undefined;
}

describe("global undefined", () => {
  test("compares as the value", () => {
    expect(missed()).toBe(2);
  });
  test("a local binding of the name", () => {
    expect(shadowedHere()).toBe(5);
  });
  test("a binding of the name around the function", () => {
    expect(shadowedAround()).toBe("outer");
  });
  test("a write to the global property changes nothing", () => {
    expect(writeIgnored()).toBe(void 0);
  });
});
