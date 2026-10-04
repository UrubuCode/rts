import { describe, test, expect } from "rts:test";

// The half that makes a decorator useful rather than an observer: what it
// RETURNS. Measured against bun 1.4.0 with `experimentalDecorators`.
//
// Both directions matter and this engine had both wrong. A returned value
// replaces the target; a returned `undefined` leaves the target alone. The
// second was what made `@Observer class A {}` answer `typeof A ===
// "undefined"` — the assignment was unconditional, so an observer destroyed
// what it observed.

const log: string[] = [];

function Observe(target: any) {
  log.push("observed " + typeof target);
}

@Observe
class Kept {
  x = 1;
}

function Replace(_target: any) {
  return class Other {
    tag() {
      return "replaced";
    }
  };
}

@Replace
class Original {
  tag() {
    return "original";
  }
}

function Wrap(_target: any, _key: string, descriptor: any) {
  const inner = descriptor.value;
  return {
    ...descriptor,
    value: function (this: any, ...args: any[]) {
      return "wrapped(" + inner.apply(this, args) + ")";
    },
  };
}

function LeaveAlone(_target: any, _key: string, _descriptor: any) {
  log.push("left alone");
}

class Service {
  @Wrap wrapped() {
    return "body";
  }
  @LeaveAlone untouched() {
    return "body";
  }
}

// A factory is the ordinary spelling, and it is two calls: the factory runs
// for its arguments, and the function it returns is the decorator. The engine
// used to evaluate the first and discard the second, so no decorator written
// this way ever ran.
const factoryLog: string[] = [];
function Entity(name: string) {
  factoryLog.push("factory:" + name);
  return (target: any) => {
    factoryLog.push("applied:" + name + "|" + typeof target);
  };
}

@Entity("user")
class User {
  y = 2;
}

// Several decorators on one target run bottom-up, the one nearest the
// declaration first.
const multi: string[] = [];
function tag(n: string) {
  return (target: any) => {
    multi.push("class:" + n);
    return target;
  };
}
function mtag(n: string) {
  return (_t: any, _k: string, d: any) => {
    multi.push("method:" + n);
    return d;
  };
}

@tag("outer")
@tag("middle")
@tag("inner")
class Multi {
  @mtag("mouter") @mtag("minner") m() {}
}
void Multi;

describe("claude-decorator-replaces", () => {
  test("a decorator that returns nothing leaves the class bound", () => {
    expect(typeof Kept).toBe("function");
    expect(new Kept().x).toBe(1);
    expect(log[0]).toBe("observed function");
  });

  test("a class decorator that returns a value replaces the class", () => {
    expect(new (Original as any)().tag()).toBe("replaced");
  });

  test("a method decorator that returns a descriptor replaces the method", () => {
    expect(new Service().wrapped()).toBe("wrapped(body)");
  });

  test("a method decorator that returns nothing keeps the method", () => {
    expect(new Service().untouched()).toBe("body");
    expect(log.indexOf("left alone") >= 0).toBe(true);
  });

  test("a factory runs AND the function it returns is applied", () => {
    expect(factoryLog.join(",")).toBe("factory:user,applied:user|function");
    expect(typeof User).toBe("function");
    expect(new User().y).toBe(2);
  });

  test("several decorators on one target run bottom-up", () => {
    expect(multi.join(",")).toBe(
      "method:minner,method:mouter,class:inner,class:middle,class:outer",
    );
  });
});
