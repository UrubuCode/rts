// Installing a class member is `[[DefineOwnProperty]]`, which consults nothing
// and writes the own slot. `define_method` was calling `objects::set_property`,
// which is `[[Set]]`: it walks the prototype chain looking for an accessor and
// RUNS one if it finds it.
//
// Three wrong answers came out of that, all checked against node. The second is
// the worst, because the base's setter ran at class-definition time and
// swallowed the method — a side effect the language does not have.
//
// `docs/codegen/object-model.md` names this under "Correctness, off the
// nanosecond list, ships regardless" and prescribes the fix used here: one
// define primitive over `objects::put`.
import { describe, test, expect } from "rts:test";

describe("a class member is defined, not assigned", () => {
  test("a base GETTER does not refuse a derived method", () => {
    class G {
      get name(): string {
        return "the getter";
      }
    }
    class Sub extends G {
      name(): number {
        return 1;
      }
    }
    // A getter with no setter refused the write, which threw.
    expect(typeof new Sub().name).toBe("function");
    expect((new Sub() as any).name()).toBe(1);
  });

  test("a base SETTER does not run at class-definition time", () => {
    let ran = 0;
    class G {
      set name(v: any) {
        ran = 1;
      }
      get name(): string {
        return "the getter";
      }
    }
    class Sub extends G {
      name(): number {
        return 1;
      }
    }
    expect(ran).toBe(0);
    expect(typeof new Sub().name).toBe("function");
  });

  test("the base's accessor still works for a class that does NOT shadow it", () => {
    // The falsifier for the fix rather than the bug: define must not stop the
    // chain from being consulted where the language says it should be.
    let written = "";
    class G {
      set v(x: string) {
        written = x;
      }
      get v(): string {
        return "read";
      }
    }
    class Plain extends G {}
    const p = new Plain();
    expect(p.v).toBe("read");
    p.v = "assigned";
    expect(written).toBe("assigned");
  });

  test("a method is still NOT enumerable", () => {
    // The property `define_method` exists for, and the one a define over `put`
    // could silently drop.
    class C {
      m(): number {
        return 1;
      }
    }
    const keys: string[] = [];
    for (const k in new C()) keys.push(k);
    expect(keys.length).toBe(0);
    const d = Object.getOwnPropertyDescriptor(C.prototype, "m");
    expect(d !== undefined).toBe(true);
    expect((d as any).enumerable).toBe(false);
    expect((d as any).writable).toBe(true);
    expect((d as any).configurable).toBe(true);
  });

  test("an inherited data property does not refuse either", () => {
    class G {}
    (G.prototype as any).shared = 1;
    class Sub extends G {
      shared(): number {
        return 2;
      }
    }
    expect(typeof new Sub().shared).toBe("function");
  });
});
