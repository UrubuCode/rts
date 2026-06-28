import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264) `(Base as any).call(this, ...)` no corpo de um constructor-function —
// mixin estilo ES5: roda o corpo de Base no `this` atual. Via canônica JS
// `this.x`/`obj.x` (antes usava `collections.map_*(this)`).

function Init(name: string): void {
  this.tag = 7;
}

function Wrapper(name: string): void {
  (Init as any).call(this, name);
  this.wrapped = 99;
}

const w: any = new (Wrapper as any)("foo");
const t: number = w.tag;
const wr: number = w.wrapped;
print("tag=" + t);
print("wrapped=" + wr);

describe("fn.call com TsAs em fn body (#264)", () => {
  test("(Init as any).call(this, ...) propaga this slot", () =>
    expect(__rtsCapturedOutput).toBe(
      "tag=7\n" +
      "wrapped=99\n"
    ));
});
