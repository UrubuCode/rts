import { describe, test, expect } from "rts:test";

// A `for`-`of` over a string walks its code points as a list where the string's
// iterator would step exactly that list -- and steps the protocol wherever a
// program changed it. Each case is one way the two could come apart.

function points(s: string): string {
  const out: string[] = [];
  for (const ch of s) out.push(ch);
  return out.join("|");
}

function brokenOut(s: string): string {
  let out = "";
  for (const ch of s) {
    if (ch === "c") break;
    out += ch;
  }
  return out;
}

function wrapperObject(): string {
  const out: string[] = [];
  for (const ch of new String("xy") as any) out.push(ch);
  return out.join("|");
}

function replacedIterator(): string {
  const proto = String.prototype as any;
  const saved = proto[Symbol.iterator];
  proto[Symbol.iterator] = function* () {
    yield "patched";
  };
  try {
    return points("abc");
  } finally {
    proto[Symbol.iterator] = saved;
  }
}

function replacedNext(): string {
  const listProto = Object.getPrototypeOf(("q" as any)[Symbol.iterator]());
  const saved = listProto.next;
  let calls = 0;
  listProto.next = function (this: any) {
    calls++;
    return saved.call(this);
  };
  try {
    return points("ab") + "#" + calls;
  } finally {
    listProto.next = saved;
  }
}

describe("for-of over a string", () => {
  test("code points, a surrogate pair as one", () => {
    expect(points("a😀b")).toBe("a|😀|b");
    expect(points("")).toBe("");
  });
  test("break leaves early", () => {
    expect(brokenOut("abcd")).toBe("ab");
  });
  test("a String object", () => {
    expect(wrapperObject()).toBe("x|y");
  });
  test("a replaced Symbol.iterator is called", () => {
    expect(replacedIterator()).toBe("patched");
    expect(points("ok")).toBe("o|k");
  });
  test("a replaced next is called once per step", () => {
    expect(replacedNext()).toBe("a|b#3");
  });
});
