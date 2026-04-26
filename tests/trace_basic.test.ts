import { describe, test, expect } from "rts:test";
import { trace, gc } from "rts";

let __out: string = "";
function print(v: string): void { __out += v + "\n"; }

// trace.capture() with no frames pushed returns 0 (empty stack)
const h0 = trace.capture();
if (h0 == 0) {
  print("capture_empty_ok");
} else {
  print("FAIL: expected 0 from empty capture");
  trace.free(h0);
}

// depth() on empty stack
const d0 = trace.depth();
if (d0 == 0) {
  print("depth_zero_ok");
} else {
  print("FAIL: depth should be 0");
}

// Push a frame and capture
trace.push_frame("main.ts", "myFn", 10, 5);

const d1 = trace.depth();
if (d1 == 1) {
  print("depth_one_ok");
} else {
  print("FAIL: depth should be 1");
}

const h1 = trace.capture();
if (h1 != 0) {
  print("capture_with_frame_ok");
  const s = gc.string_ptr(h1);
  // We can't compare exact content but the handle is valid
  trace.free(h1);
} else {
  print("FAIL: capture should return handle with frame pushed");
}

// Pop the frame
trace.pop_frame();

const d2 = trace.depth();
if (d2 == 0) {
  print("depth_back_to_zero_ok");
} else {
  print("FAIL: depth should be 0 after pop");
}

describe("fixture:trace_basic", () => {
  test("trace operations", () => {
    expect(__out).toBe(
      "capture_empty_ok\ndepth_zero_ok\ndepth_one_ok\ncapture_with_frame_ok\ndepth_back_to_zero_ok\n"
    );
  });
});
