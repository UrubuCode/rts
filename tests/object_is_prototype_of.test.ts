import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const parent: any = { x: 1 };
const child: any = Object.create(parent);

print("parent_of_child=" + parent.isPrototypeOf(child));
print("child_of_parent=" + child.isPrototypeOf(parent));

// Nested chain: parent <- mid <- child
const mid: any = Object.create(parent);
const grandchild: any = Object.create(mid);
print("parent_of_grandchild=" + parent.isPrototypeOf(grandchild));
print("mid_of_grandchild=" + mid.isPrototypeOf(grandchild));

// Unrelated objects
const other: any = { y: 2 };
print("other_of_child=" + other.isPrototypeOf(child));

describe("Object.prototype.isPrototypeOf (#772)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "parent_of_child=true\n" +
      "child_of_parent=false\n" +
      "parent_of_grandchild=true\n" +
      "mid_of_grandchild=true\n" +
      "other_of_child=false\n"
    )
  );
});
