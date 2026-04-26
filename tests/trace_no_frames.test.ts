import { describe, test, expect } from "rts:test";
import { trace } from "rts";

let __out: string = "";
function print(v: string): void { __out += v + "\n"; }

// With no frames pushed, capture returns 0
const h = trace.capture();
if (h == 0) {
  print("no_frames");
} else {
  print("FAIL: expected 0 when no frames");
  trace.free(h);
}

// depth is 0
const d = trace.depth();
if (d == 0) {
  print("depth_ok");
} else {
  print("FAIL: expected depth 0");
}

describe("fixture:trace_no_frames", () => {
  test("empty stack behavior", () => {
    expect(__out).toBe("no_frames\ndepth_ok\n");
  });
});
