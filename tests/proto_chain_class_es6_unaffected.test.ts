import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR4) Classes ES6 NAO sao afetadas pelo chain walk — Maps delas
// nao tem __proto__ entao a busca para imediatamente apos own props.

class Box {
  width: number;
  height: number;
  constructor(w: number, h: number) {
    this.width = w;
    this.height = h;
  }
  area(): number {
    return this.width * this.height;
  }
}

const b = new Box(3, 5);
print("w=" + b.width);
print("h=" + b.height);
print("area=" + b.area());

// Cenario misto: classe ES6 + constructor function no mesmo programa
function Tag(): void {}
Tag.prototype.kind = 7 as any;

const t: number = new (Tag as any)();
const k: number = (t as any).kind;
print("tag.kind=" + k);

// Classe ES6 nao "ve" o prototype de Tag mesmo no mesmo programa
print("box.area still=" + b.area());

describe("classes ES6 nao quebram com chain walk (#264 PR4)", () => {
  test("classe ES6 funciona normalmente; constructor fn ao lado", () =>
    expect(__rtsCapturedOutput).toBe(
      "w=3\n" +
      "h=5\n" +
      "area=15\n" +
      "tag.kind=7\n" +
      "box.area still=15\n"
    ));
});
