import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (#1071) Getter via Object.defineProperty(C.prototype, "x", {get(){return this._f}})
// onde _f eh bool deve tipar a leitura como bool (true/false, nao 1/0).
class Widget {
  private _visible = true;
  private _enabled = false;
}
Object.defineProperty(Widget.prototype, "visible", {
  get(this: any) { return this._visible; },
});
Object.defineProperty(Widget.prototype, "enabled", {
  get(this: any) { return this._enabled; },
});
const w: any = new Widget();
print("vis=" + w.visible);
print("en=" + w.enabled);

describe("defineProperty getter bool (#1071)", () => {
  test("getter bool tipado como true/false", () =>
    expect(out).toBe("vis=true\nen=false\n"));
});
