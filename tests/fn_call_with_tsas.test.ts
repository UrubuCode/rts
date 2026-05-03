import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264) (Animal as any).call(this, ...) em fn body — antes
// "unsupported call expression form".

function Init(name: string): void {
  collections.map_set(this as any, "tag", 7);
}

function Wrapper(name: string): void {
  (Init as any).call(this, name);
  collections.map_set(this as any, "wrapped", 99);
}

const w: number = new (Wrapper as any)("foo");
const t: number = collections.map_get(w, "tag");
const wr: number = collections.map_get(w, "wrapped");
print("tag=" + t);
print("wrapped=" + wr);

describe("fn.call com TsAs em fn body (#264)", () => {
  test("(Init as any).call(this, ...) propaga this slot", () =>
    expect(__rtsCapturedOutput).toBe(
      "tag=7\n" +
      "wrapped=99\n"
    ));
});
