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

describe("a class FIELD is defined too, and stays enumerable", () => {
  // `emit/class.rs` synthesised `this.k = e` into the constructor's prologue as
  // an ordinary assignment, and a static field took `emit_write` directly — both
  // `[[Set]]`, both walking the prototype chain. The marker that says "this is a
  // field" already travelled with the value; nothing was asking it.
  //
  // The fix is `property::emit_define`, which differs from `emit_write` on the
  // SLOW PATH ONLY. The fast path is a cached store into an own slot the site
  // already resolved for this layout, and an own slot is what a define writes —
  // so where the cache hits the two agree by construction. Routing the whole
  // write through the entry point instead measured `alloc class instance`
  // 89 -> 182 ns; changing only what a miss calls costs nothing (78.8 -> 76.9).
  test("an instance field over a base getter", () => {
    class G {
      get k(): number {
        return 1;
      }
    }
    class Sub extends G {
      k = 5;
    }
    expect(new Sub().k).toBe(5);
  });

  test("a STATIC field over a base static getter", () => {
    class G {
      static get k(): number {
        return 1;
      }
    }
    class Sub extends G {
      static k = 5;
    }
    expect(Sub.k).toBe(5);
  });

  test("a base setter does not run at construction", () => {
    let ran = 0;
    class G {
      set v(x: any) {
        ran = 1;
      }
      get v(): string {
        return "g";
      }
    }
    class Sub extends G {
      v = 1;
    }
    expect(new Sub().v).toBe(1);
    expect(ran).toBe(0);
  });

  test("and a field is still ENUMERABLE, where a method is not", () => {
    // The attribute that makes this a second operation rather than a flag, and
    // the one a define over `put` could silently drop in the other direction.
    class C {
      x = 1;
      y = 2;
      m(): number {
        return 0;
      }
    }
    const keys: string[] = [];
    for (const k in new C()) keys.push(k);
    expect(keys.sort().join(",")).toBe("x,y");
    const d = Object.getOwnPropertyDescriptor(new C(), "x") as any;
    expect(d.enumerable).toBe(true);
    expect(d.writable).toBe(true);
    expect(d.configurable).toBe(true);
    expect(d.value).toBe(1);
  });

  test("an ordinary property write still performs [[Set]]", () => {
    // The falsifier for the fix: only a FIELD is defined. An assignment the
    // program writes must still find a setter on the chain and run it.
    let written = "";
    class G {
      set w(x: string) {
        written = x;
      }
    }
    const g = new G();
    g.w = "ran";
    expect(written).toBe("ran");
  });
});
