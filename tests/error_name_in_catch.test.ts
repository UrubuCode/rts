import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

function boom() {
  throw new Error("zap");
}

try {
  boom();
} catch (e: any) {
  print("name=" + e.name);
  print("msg=" + e.message);
}

// Mesmo padrao com TypeError
function typeBoom() {
  throw new TypeError("bad type");
}

try {
  typeBoom();
} catch (e: any) {
  print("te_name=" + e.name);
  print("te_msg=" + e.message);
}

describe("catch (e: any) reads .name/.message from ErrorObj (#745)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "name=Error\n" +
      "msg=zap\n" +
      "te_name=TypeError\n" +
      "te_msg=bad type\n"
    )
  );
});
