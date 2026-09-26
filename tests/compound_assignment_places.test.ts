import { describe, test, expect } from "rts:test";

// A compound or logical assignment reads its target once and writes it at most once:
// the object and a computed key are evaluated a single time, and a logical form that
// short-circuits neither evaluates its value nor runs a setter.

function memberCompound(): string {
  const o: any = { x: 1, s: "a" };
  o.x += 2;
  o.x *= 5;
  o.s += "b";
  o.x **= 2;
  return o.x + ":" + o.s;
}

function keyOnce(): string {
  const o: any = { k0: 10, k1: 20 };
  let calls = 0;
  const key = () => "k" + calls++;
  o[key()] -= 1;
  o[key()] |= 3;
  return [o.k0, o.k1, calls].join();
}

function objectOnce(): string {
  const box: any = { v: 1 };
  let reads = 0;
  const get = () => (reads++, box);
  get().v += 1;
  get().v ||= 9;
  return box.v + ":" + reads;
}

function logicalMembers(): string {
  const log: string[] = [];
  const o: any = {
    _a: 0,
    get a() { log.push("get"); return this._a; },
    set a(v: any) { log.push("set " + v); this._a = v; },
  };
  o.a ||= 5;
  o.a ||= 7;
  o.a &&= 8;
  o.b ??= 1;
  o.b ??= (log.push("unreached"), 2);
  return log.join(",") + "|" + o._a + "|" + o.b;
}

function logicalNames(): string {
  let a: any = null, b: any = 0, c: any = 1;
  a ??= "A";
  b ||= "B";
  c &&= "C";
  let f: any;
  f ??= function () {};
  return [a, b, c, f.name].join();
}

describe("compound assignment to a place", () => {
  test("a member", () => {
    expect(memberCompound()).toBe("225:ab");
  });
  test("a computed key once", () => {
    expect(keyOnce()).toBe("9,23,2");
  });
  test("the object once", () => {
    expect(objectOnce()).toBe("2:2");
  });
  test("logical, through an accessor", () => {
    expect(logicalMembers()).toBe("get,set 5,get,get,set 8|8|1");
  });
  test("logical, on names", () => {
    expect(logicalNames()).toBe("A,B,C,f");
  });
});
