// Cross-runtime: an exception raised while a class is being DEFINED or an
// instance is being BUILT. A throwing computed key aborts the definition and
// leaves the binding uninitialised; a throwing field initialiser aborts the
// construction and no reference to the half-built instance escapes.
const log: string[] = [];

function probe(fn: () => any): string {
  try {
    const v = fn();
    return "ok:" + String(v);
  } catch (e: any) {
    return e.constructor.name;
  }
}

// A computed key that throws: the keys BEFORE it were already evaluated, the
// ones after it never are, and the class binding is never initialised.
console.log("computed-key=" + probe(() => {
  class Broken {
    [(log.push("k1"), "a")](): number {
      return 1;
    }
    [(() => {
      log.push("k2-throws");
      throw new RangeError("key");
    })()](): number {
      return 2;
    }
    [(log.push("k3"), "c")](): number {
      return 3;
    }
  }
  return typeof Broken;
}));
console.log("computed-log=" + log.join(","));

// The binding stays in the dead zone afterwards, so the name is unusable.
console.log("binding-after=" + probe(() => {
  let outcome = "none";
  try {
    class Broken2 {
      [(() => {
        throw new RangeError("key2");
      })()](): number {
        return 1;
      }
    }
    outcome = "defined:" + typeof Broken2;
  } catch (e: any) {
    outcome = "threw:" + e.constructor.name;
  }
  return outcome;
}));

// A static field initialiser that throws aborts the definition too, after the
// earlier statics have already run.
const statics: string[] = [];
console.log("static-field=" + probe(() => {
  class S {
    static first: string = (statics.push("first"), "1");
    static boom: string = (() => {
      statics.push("boom");
      throw new EvalError("static");
    })();
    static never: string = (statics.push("never"), "3");
  }
  return typeof S;
}));
console.log("static-log=" + statics.join(","));

// An INSTANCE field initialiser that throws: the fields before it were
// installed on a `this` that never escapes, the constructor body never runs,
// and `new` produces nothing.
const fields: string[] = [];
let escaped: any = null;
class Partial {
  a: string = (fields.push("a"), "a-value");
  captured: string = (escaped = this, "captured");
  b: string = (() => {
    fields.push("b-throws");
    throw new URIError("field");
  })();
  c: string = (fields.push("c"), "c-value");

  constructor() {
    fields.push("ctor");
  }
}
console.log("instance-field=" + probe(() => new Partial()));
console.log("instance-log=" + fields.join(","));
console.log("escaped-keys=" + (escaped === null ? "none" : Object.keys(escaped).join(",")));
console.log("escaped-is-partial=" + (escaped instanceof Partial));
console.log("escaped-a=" + (escaped === null ? "none" : String(escaped.a)));
console.log("escaped-b=" + (escaped === null ? "none" : String(escaped.b)));

// In a DERIVED class the base is fully built first, so a derived field that
// throws leaves the base constructor's side effects behind.
const sideEffects: string[] = [];
class Base {
  built: string = "base-built";
  constructor() {
    sideEffects.push("base-ctor");
  }
}
class DerivedBroken extends Base {
  ok: string = (sideEffects.push("derived-ok"), "ok");
  bad: string = (() => {
    sideEffects.push("derived-bad");
    throw new SyntaxError("derived");
  })();
  constructor() {
    super();
    sideEffects.push("derived-ctor");
  }
}
console.log("derived=" + probe(() => new DerivedBroken()));
console.log("derived-log=" + sideEffects.join(","));

// A throwing base constructor stops the derived fields from running at all.
const order2: string[] = [];
class ThrowingBase {
  constructor() {
    order2.push("base");
    throw new EvalError("base-ctor");
  }
}
class OverThrowing extends ThrowingBase {
  field: string = (order2.push("derived-field"), "f");
  constructor() {
    super();
    order2.push("derived-ctor");
  }
}
console.log("throwing-base=" + probe(() => new OverThrowing()));
console.log("throwing-base-log=" + order2.join(","));

// The class binding itself survives when only the CONSTRUCTOR throws — the
// definition succeeded, so the name is usable and a second attempt fails the
// same way.
console.log("class-usable=" + typeof OverThrowing);
console.log("second-attempt=" + probe(() => new OverThrowing()));
console.log("prototype-intact=" + (OverThrowing.prototype.constructor === OverThrowing));
console.log("chain-intact=" + (Object.getPrototypeOf(OverThrowing.prototype) === ThrowingBase.prototype));

// A getter used as a computed key value is coerced through toPrimitive, and a
// throwing toPrimitive aborts the definition just as a throwing key does.
const badKey: any = {
  [Symbol.toPrimitive](): string {
    throw new URIError("toprimitive");
  },
};
console.log("bad-key=" + probe(() => {
  class K {
    [badKey](): number {
      return 1;
    }
  }
  return typeof K;
}));

// A heritage expression that throws leaves no class either.
console.log("bad-heritage=" + probe(() => {
  class H extends (() => {
    throw new RangeError("heritage");
  })() {}
  return typeof H;
}));
console.log("final-log=" + log.join(",") + "/" + fields.join(","));
