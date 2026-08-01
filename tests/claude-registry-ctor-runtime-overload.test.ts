import { describe, test, expect } from "rts:test";

// A Registry class whose constructor is OVERLOADED (same arity, different
// parameter types) used to require the front to PROVE the argument's type. When
// nothing was proven — a function parameter, a loop variable, an `any` — the
// engine bailed. The overload is now chosen at run time from the PolyValue tag,
// with the arms and their order derived from the registered signatures.
//
// `Date` is the only pure-Registry class with overloaded constructors today, so
// it is what exercises the path; the mechanism names no class. Every expectation
// below was checked against Node.

function fromUnknown(x: any): number {
  return new Date(x).getTime();
}

const fromNumberParam = fromUnknown(1234567);
const fromStringParam = fromUnknown("2020-01-01T00:00:00Z");

// The same site, alternating types across iterations — one compiled site, two
// runtime answers.
const mixed: any[] = [0, "1970-01-02T00:00:00Z", 86400000];
const fromLoop: number[] = [];
for (const v of mixed) {
  fromLoop.push(new Date(v).getTime());
}

// The proven-type paths must be untouched by the new dispatch.
const provenNumber = new Date(1000000).getTime();
const provenString = new Date("2020-01-01T00:00:00Z").getTime();
const provenCalendar = new Date(2020, 0, 1).getFullYear();
const nowIsPositive = new Date().getTime() > 0;

// A string that is not a date still parses to NaN through the dynamic arm,
// exactly as through the proven one.
const unparsable = String(fromUnknown("bogus"));

describe("runtime overload dispatch for a Registry constructor", () => {
  test("an unproven NUMBER argument reaches the numeric constructor", () => {
    expect(fromNumberParam).toBe(1234567);
  });

  test("an unproven STRING argument reaches the parsing constructor", () => {
    expect(fromStringParam).toBe(1577836800000);
  });

  test("one site serves both types across iterations", () => {
    expect(fromLoop[0]).toBe(0);
    expect(fromLoop[1]).toBe(86400000);
    expect(fromLoop[2]).toBe(86400000);
  });

  test("the proven-type paths still resolve statically", () => {
    expect(provenNumber).toBe(1000000);
    expect(provenString).toBe(1577836800000);
    expect(provenCalendar).toBe(2020);
    expect(nowIsPositive).toBe(true);
  });

  test("an unparsable string is NaN, not an error", () => {
    expect(unparsable).toBe("NaN");
  });
});
