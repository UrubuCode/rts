// Cross-runtime: `this` is in TDZ inside a derived constructor until super()
// runs, a second super() call is a ReferenceError, and returning an object
// from a derived constructor replaces the bound `this`.
const log: string[] = [];

class Base {
  tag: string = "base";
  constructor() {
    log.push("base-ctor");
  }
}

class TdzProbe extends Base {
  constructor() {
    try {
      // Reading `this` before super() must throw ReferenceError.
      log.push("read=" + (this as any).tag);
    } catch (e: any) {
      log.push("read-throws=" + e.constructor.name);
    }
    super();
    log.push("after-super=" + this.tag);
  }
}

new TdzProbe();
console.log("tdz=" + log.join("|"));

class DoubleSuper extends Base {
  constructor() {
    super();
    super();
  }
}

try {
  new DoubleSuper();
  console.log("double=no-throw");
} catch (e: any) {
  console.log("double=" + e.constructor.name);
  console.log("double-is-ref=" + (e instanceof ReferenceError));
}

class NoSuper extends Base {
  constructor() {
    // Falling off the end without super() also leaves `this` uninitialised.
    log.push("nosuper-body");
  }
}

try {
  new NoSuper();
  console.log("nosuper=no-throw");
} catch (e: any) {
  console.log("nosuper=" + e.constructor.name);
}

class ReturnObject extends Base {
  constructor() {
    super();
    this.tag = "ignored";
    return { tag: "replacement", extra: 1 } as any;
  }
}

const ro: any = new ReturnObject();
console.log("ret-tag=" + ro.tag);
console.log("ret-extra=" + ro.extra);
console.log("ret-instanceof=" + (ro instanceof ReturnObject));

class ReturnPrimitive extends Base {
  constructor() {
    super();
    // A non-undefined primitive return from a derived constructor is a TypeError.
    return 42 as any;
  }
}

try {
  new ReturnPrimitive();
  console.log("prim=no-throw");
} catch (e: any) {
  console.log("prim=" + e.constructor.name);
  console.log("prim-is-type=" + (e instanceof TypeError));
}

class ReturnUndefined extends Base {
  constructor() {
    super();
    // Explicit undefined is allowed and keeps `this`.
    return undefined;
  }
}

const ru = new ReturnUndefined();
console.log("undef-tag=" + ru.tag);
console.log("undef-instanceof=" + (ru instanceof ReturnUndefined));

class ReturnUndefinedNoSuper extends Base {
  constructor() {
    // Returning an object skips the need for super() entirely.
    return { tag: "bypass" } as any;
  }
}

const bp: any = new ReturnUndefinedNoSuper();
console.log("bypass-tag=" + bp.tag);
console.log("bypass-instanceof=" + (bp instanceof ReturnUndefinedNoSuper));

class ReturnNullNoSuper extends Base {
  constructor() {
    // null is not an object, so `this` is still uninitialised here.
    return null as any;
  }
}

try {
  new ReturnNullNoSuper();
  console.log("retnull=no-throw");
} catch (e: any) {
  console.log("retnull=" + e.constructor.name);
}

class FieldsAfterSuper extends Base {
  own: string = "own:" + this.tag;
  constructor() {
    super();
    log.push("fields-ctor=" + this.own);
  }
}

const fa = new FieldsAfterSuper();
console.log("field-own=" + fa.own);
console.log("field-order=" + Object.keys(fa).join(","));
console.log("log-tail=" + log[log.length - 1]);

// A derived constructor that only forwards its arguments is exactly what an
// omitted constructor does, including the argument count.
class Explicit extends Base {
  constructor(...args: any[]) {
    super(...args);
  }
}
class Implicit extends Base {}
console.log("explicit-len=" + Explicit.length);
console.log("implicit-len=" + Implicit.length);
console.log("implicit-tag=" + new Implicit().tag);

// super() may be called from a nested arrow, but not from a nested function.
class ArrowSuper extends Base {
  constructor() {
    const go = () => {
      super();
    };
    go();
    this.tag = "arrow";
  }
}
console.log("arrow-super=" + new ArrowSuper().tag);

// Calling a method before super() is still a `this` access, so it throws.
class EarlyMethod extends Base {
  constructor() {
    let outcome = "no-throw";
    try {
      (this as any).describe();
    } catch (e: any) {
      outcome = e.constructor.name;
    }
    super();
    this.tag = outcome;
  }
  describe(): string {
    return "d";
  }
}
console.log("early-method=" + new EarlyMethod().tag);

// `new.target` is readable before super(); it is not part of `this`.
class TargetBeforeSuper extends Base {
  constructor() {
    const nt: any = new.target;
    super();
    this.tag = nt.name;
  }
}
console.log("newtarget-before-super=" + new TargetBeforeSuper().tag);
console.log("base-ctor-count=" + log.filter((x) => x === "base-ctor").length);
