import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const parent: any = { x: 10, y: 20 };
const child: any = { z: 30 };

// setPrototypeOf retorna o proprio target (JS spec).
const ret = Object.setPrototypeOf(child, parent);
print("is_self=" + (ret === child));

// child agora ve campos do parent via cadeia __proto__.
print("child_z=" + child.z);
print("child_x=" + child.x);
print("child_y=" + child.y);

// Mudar de prototype
const otherParent: any = { x: 100 };
Object.setPrototypeOf(child, otherParent);
print("child_x2=" + child.x);
print("child_y2=" + child.y);  // undefined-ish

describe("Object.setPrototypeOf (#772)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "is_self=true\n" +
      "child_z=30\n" +
      "child_x=10\n" +
      "child_y=20\n" +
      "child_x2=100\n" +
      "child_y2=null\n"
    )
  );
});
