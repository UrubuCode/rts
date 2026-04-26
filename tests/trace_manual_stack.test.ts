import { describe, test, expect } from "rts:test";
import { trace, gc } from "rts";

let __out: string = "";
function print(v: string): void { __out += v + "\n"; }

// Simulate: outer() calls inner()
trace.push_frame("app.ts", "outer", 20, 3);
trace.push_frame("app.ts", "inner", 42, 7);

if (trace.depth() == 2) {
  print("depth_2_ok");
}

const h = trace.capture();
if (h != 0) {
  const text = gc.string_ptr(h);
  // stack should mention both functions (most recent first)
  print("captured_2_frames");
  trace.free(h);
}

// Pop inner, check depth is 1
trace.pop_frame();
if (trace.depth() == 1) {
  print("depth_1_after_pop");
}

// Pop outer, back to 0
trace.pop_frame();
if (trace.depth() == 0) {
  print("depth_0_after_all_pops");
}

describe("fixture:trace_manual_stack", () => {
  test("push/pop multi-frame stack", () => {
    expect(__out).toBe(
      "depth_2_ok\ncaptured_2_frames\ndepth_1_after_pop\ndepth_0_after_all_pops\n"
    );
  });
});
