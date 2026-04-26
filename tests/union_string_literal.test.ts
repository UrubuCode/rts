import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// String literal union inline (sem type alias).

function settings(m: "fast" | "slow" | "auto"): string {
  if (m == "fast") return "high-perf";
  if (m == "slow") return "low-power";
  return "balanced";
}

print(settings("fast"));
print(settings("slow"));
print(settings("auto"));

describe("fixture:union_string_literal", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("high-perf\nlow-power\nbalanced\n");
  });
});
