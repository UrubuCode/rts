// Cross-runtime: every computed class key is evaluated exactly once, in textual
// order, when the class is DEFINED — before any static field, static block or
// instance field initialiser runs — and ToPropertyKey is applied there too.
const log: string[] = [];

function k(name: string): string {
  log.push("key:" + name);
  return name;
}

const objKey = {
  toString(): string {
    log.push("toString");
    return "coerced";
  },
};

const sym = Symbol("s");

class C {
  [k("m1")](): string {
    return "m1";
  }
  [k("f1")]: string = (log.push("init:f1"), "f1");
  static [k("sf1")]: string = (log.push("init:sf1"), "sf1");
  get [k("g1")](): string {
    return "g1";
  }
  static {
    log.push("static-block");
  }
  [objKey as any]: string = (log.push("init:coerced"), "coerced-value");
  [sym](): string {
    return "sym";
  }
  // Same key twice: both expressions run, the last definition wins.
  [k("dup")](): string {
    return "first";
  }
  [k("dup")](): string {
    return "second";
  }
  static [k("sf2")]: string = (log.push("init:sf2"), "sf2");
}

console.log("define-order=" + log.join("|"));
log.length = 0;

const c: any = new C();
console.log("instance-order=" + log.join("|"));

console.log("m1=" + c.m1());
console.log("f1=" + c.f1);
console.log("g1=" + c.g1);
console.log("dup=" + c.dup());
console.log("coerced=" + c.coerced);
console.log("sym=" + c[sym]());
console.log("sf1=" + (C as any).sf1);
console.log("sf2=" + (C as any).sf2);

console.log("proto-names=" + Object.getOwnPropertyNames(C.prototype).join(","));
console.log("proto-symbols=" + Object.getOwnPropertySymbols(C.prototype).length);
console.log("inst-keys=" + Object.keys(c).join(","));

// A second instance does not re-evaluate any key expression.
log.length = 0;
const c2: any = new C();
console.log("second-instance=" + log.join("|"));
console.log("second-f1=" + c2.f1);

// A throwing computed key aborts the class definition; earlier keys already ran.
const log2: string[] = [];
function k2(name: string): string {
  log2.push(name);
  return name;
}
try {
  class Bad {
    [k2("ok")](): number {
      return 1;
    }
    [(() => {
      log2.push("boom");
      throw new RangeError("key");
    })()](): number {
      return 2;
    }
    [k2("never")](): number {
      return 3;
    }
  }
  console.log("bad=no-throw");
} catch (e: any) {
  console.log("bad=" + e.constructor.name);
}
console.log("bad-log=" + log2.join("|"));

// Computed keys in a class EXPRESSION run when the expression is evaluated.
const log3: string[] = [];
function make(tag: string) {
  return class {
    [(log3.push("k:" + tag), tag)](): string {
      return tag;
    }
  };
}
console.log("expr-before=" + log3.join("|"));
const A = make("a");
const B = make("b");
console.log("expr-after=" + log3.join("|"));
console.log("expr-a=" + (new A() as any).a());
console.log("expr-b=" + (new B() as any).b());
