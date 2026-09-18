import { describe, test, expect } from "rts:test";

// A derived constructor's `this` is a binding that `super()` initialises, and
// every way of reaching it before then is a ReferenceError (GetThisBinding):
// a written `this`, a `super.x`, and falling off the end of the body — which is
// an implicit `return this`. And a `return` whose value is `undefined` answers
// that same binding, so `return;` after `super()` produces the instance.
//
// Each expectation was read off Node 20 and Bun 1.4 running this same file.

function errorName(make: () => unknown): string {
  try {
    make();
    return "none";
  } catch (e) {
    return (e as Error).constructor.name;
  }
}

class Base {
  a: number = 7;
  label(): string {
    return "base";
  }
}

class NeverCallsSuper extends Base {
  b: number = 13;
  constructor() {}
}

class ReadsThisFirst extends Base {
  constructor() {
    const seen = this;
    super();
  }
}

class ReadsSuperMemberFirst extends Base {
  constructor() {
    const seen = super.label();
    super();
  }
}

class EmptyReturn extends Base {
  constructor() {
    super();
    return;
  }
}

class ReturnsUndefined extends Base {
  constructor() {
    super();
    return undefined;
  }
}

class EmptyReturnWithoutSuper extends Base {
  constructor() {
    return;
  }
}

class ReturnsObjectWithoutSuper extends Base {
  constructor() {
    return { z: 1 } as any;
  }
}

class SuperInArrow extends Base {
  constructor() {
    const call = () => {
      super();
    };
    call();
  }
}

// Falling off the end of a block-bodied ARROW answers `undefined`, also when
// the arrow reads `this` — it borrows the enclosing function's `this`, it is
// not a constructor answering it.
class Method {
  x: number = 0;
  run(): unknown {
    const set = () => {
      this.x = 1;
    };
    return set();
  }
}

class DerivedWithArrow extends Base {
  answered: unknown = "unset";
  constructor() {
    super();
    const set = () => {
      this.a = 2;
    };
    this.answered = set();
  }
}

describe("derived constructor this binding", () => {
  test("a constructor that never calls super() throws ReferenceError", () => {
    expect(errorName(() => new NeverCallsSuper())).toBe("ReferenceError");
  });
  test("reading this before super() throws ReferenceError", () => {
    expect(errorName(() => new ReadsThisFirst())).toBe("ReferenceError");
  });
  test("super.member before super() throws ReferenceError", () => {
    expect(errorName(() => new ReadsSuperMemberFirst())).toBe("ReferenceError");
  });
  test("return; after super() answers the instance", () => {
    const made = new EmptyReturn();
    expect(made instanceof EmptyReturn).toBe(true);
    expect(made.a).toBe(7);
  });
  test("return undefined after super() answers the instance", () => {
    expect(new ReturnsUndefined() instanceof ReturnsUndefined).toBe(true);
  });
  test("return; without super() throws ReferenceError", () => {
    expect(errorName(() => new EmptyReturnWithoutSuper())).toBe("ReferenceError");
  });
  test("returning an object needs no super()", () => {
    expect((new ReturnsObjectWithoutSuper() as any).z).toBe(1);
  });
  test("super() inside an arrow initialises the constructor's this", () => {
    expect(new SuperInArrow() instanceof SuperInArrow).toBe(true);
  });
  test("a block-bodied arrow reading this answers undefined in a method", () => {
    expect(new Method().run()).toBe(undefined);
  });
  test("a block-bodied arrow reading this answers undefined in a derived constructor", () => {
    const made = new DerivedWithArrow();
    expect(made.answered).toBe(undefined);
    expect(made.a).toBe(2);
  });
});
