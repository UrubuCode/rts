import { describe, test, expect } from "rts:test";

// Legacy (`experimentalDecorators`) decorators, and the three orders that are
// observable from inside a decorator. Every expectation here was measured
// against bun 1.4.0 with `{"compilerOptions":{"experimentalDecorators":true}}`
// before a line of engine code was written.
//
// The ES2022 design differs in the FIRST of the three: it runs the static
// member before the instance ones, and the field last —
// `static, method, accessor, field, class` against the textual order below.
// So a program written for the standard design does not see different
// arguments, it sees a different order, which is what makes the two
// distinguishable without reading a `tsconfig`.

const log: string[] = [];

function C(n: string) {
  return (...a: any[]) => {
    log.push("class:" + n + "|args=" + a.length + "|a0=" + typeof a[0]);
  };
}
function M(n: string) {
  return (...a: any[]) => {
    log.push(
      "method:" + n +
      "|args=" + a.length +
      "|proto=" + (a[0] === K.prototype) +
      "|ctor=" + (a[0] === K) +
      "|key=" + String(a[1]) +
      "|desc=" + (a[2] && typeof a[2].value),
    );
  };
}
function P(n: string) {
  return (...a: any[]) => {
    log.push(
      "field:" + n +
      "|args=" + a.length +
      "|proto=" + (a[0] === K.prototype) +
      "|key=" + String(a[1]) +
      "|a2=" + typeof a[2],
    );
  };
}
function A(n: string) {
  return (...a: any[]) => {
    log.push(
      "accessor:" + n +
      "|args=" + a.length +
      "|key=" + String(a[1]) +
      "|get=" + (a[2] && typeof a[2].get),
    );
  };
}
function Par(n: string) {
  return (...a: any[]) => {
    log.push(
      "param:" + n +
      "|args=" + a.length +
      "|proto=" + (a[0] === K.prototype) +
      "|key=" + String(a[1]) +
      "|idx=" + a[2],
    );
  };
}

@C("K")
class K {
  @P("field") field = 1;
  @M("method") method(@Par("p0") x?: number, @Par("p1") y?: number) {
    void x;
    void y;
    return 2;
  }
  @A("get") get value() {
    return 3;
  }
  @M("static") static stat() {
    return 4;
  }
}

const k = new K();

describe("claude-decorator-order", () => {
  test("the class still works, and the decorators did not replace it", () => {
    expect(k.field).toBe(1);
    expect(k.method()).toBe(2);
    expect(k.value).toBe(3);
    expect(K.stat()).toBe(4);
    expect(typeof K).toBe("function");
  });

  test("members run in textual order, then the class", () => {
    const names = log.map((line) => line.split("|")[0]);
    expect(names.join(",")).toBe(
      "field:field,param:p1,param:p0,method:method,accessor:get,method:static,class:K",
    );
  });

  test("a field decorator is called with (prototype, key, undefined)", () => {
    expect(log[0]).toBe("field:field|args=3|proto=true|key=field|a2=undefined");
  });

  test("parameter decorators run before the method's own, highest index first", () => {
    expect(log[1]).toBe("param:p1|args=3|proto=true|key=method|idx=1");
    expect(log[2]).toBe("param:p0|args=3|proto=true|key=method|idx=0");
  });

  test("a method decorator is called with (prototype, key, descriptor)", () => {
    expect(log[3]).toBe(
      "method:method|args=3|proto=true|ctor=false|key=method|desc=function",
    );
  });

  test("an accessor decorator receives a descriptor carrying the getter", () => {
    expect(log[4]).toBe("accessor:get|args=3|key=value|get=function");
  });

  test("a static member's target is the constructor, not the prototype", () => {
    expect(log[5]).toBe(
      "method:static|args=3|proto=false|ctor=true|key=stat|desc=function",
    );
  });

  test("a class decorator receives one argument, the constructor", () => {
    expect(log[6]).toBe("class:K|args=1|a0=function");
  });
});
