import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Enum em comparações e em parâmetro de fn.

enum Status {
    Pending,
    Active,
    Closed,
}

function fixture_describe(s: number): string {
    if (s == Status.Pending) { return "wait"; }
    if (s == Status.Active) { return "run"; }
    return "done";
}

print(fixture_describe(Status.Pending));
print(fixture_describe(Status.Active));
print(fixture_describe(Status.Closed));

describe("fixture:enum_compare", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("wait\nrun\ndone\n");
  });
});
