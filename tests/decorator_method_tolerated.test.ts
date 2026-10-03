import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Method, property and parameter decorators. This fixture's name and its
// comment both said they were "tolerated syntactically" and "not executed at
// runtime (MVP limitation)", and that was true until 2026-10-03 — which is why
// the file asserted only that the program still compiled and printed "ok".
//
// They run now, so the fixture asks what they DID rather than that nothing
// happened. The name is kept because it is the fixture's identity in six months
// of history; what it tolerates is now the full legacy contract.
//
// The signatures were `(target: i64, …)`. That annotation was safe only while
// nothing was ever passed: a decorator receives the prototype or the
// constructor, which is an object, and claiming `i64` for it is a claim about a
// value the program never saw.

function noop(target: any, key: string, desc: any): any {
  print("method " + key + " desc=" + typeof desc);
  return desc;
}
function obs(target: any, key: string): void {
  print("field " + key);
}
function inj(target: any, key: string, idx: number): void {
  print("param " + key + "#" + idx);
}

class Service {
  @obs
  state: number = 0;

  @noop
  do(@inj dep: number): void {
    this.state = dep;
    print("ok");
  }
}

const s = new Service();
s.do(7);

describe("fixture:decorator_method_tolerated", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "field state\nparam do#0\nmethod do desc=object\nok\n",
    );
  });

  test("the method a decorator returned its own descriptor for still runs", () => {
    expect(s.state).toBe(7);
  });
});
