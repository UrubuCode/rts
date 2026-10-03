import { describe, test, expect } from "rts:test";

// Dependency injection, which is the shape a decorator is usually written for:
// a constructor parameter decorator records a type against a position, a class
// decorator registers the class, and a container builds it later. Nothing in
// this file exercises a feature the two fixtures beside it do not — it exists
// because the FAILURE mode is a silent one. A registry that is never written to
// answers nothing, the program carries on, and no assertion about a result can
// tell that apart from a registry that was written to and read back empty.
//
// Measured against bun 1.4.0 with `experimentalDecorators`. Two details from
// that measurement are pinned below and neither is guessable:
//
// - a CONSTRUCTOR parameter decorator is handed the constructor and `undefined`
//   as its key, where a METHOD parameter decorator is handed the prototype and
//   the method's name;
// - the constructor's parameter decorators run with the CLASS group, after
//   every member decorator, and in descending index order.

const order: string[] = [];
const deps = new Map<string, string[]>();
const registry: string[] = [];

function Inject(token: string) {
  return (target: any, key: any, index: number) => {
    order.push("inject:" + token + "|key=" + String(key) + "|idx=" + index);
    const name = typeof target === "function" ? target.name : "?";
    const list = deps.get(name) ?? [];
    while (list.length <= index) list.push("");
    list[index] = token;
    deps.set(name, list);
  };
}

function Service(name: string) {
  return (target: any) => {
    order.push("service:" + name);
    registry.push(name + "<-" + (deps.get(target.name) ?? []).join("+"));
  };
}

function Watch(_t: any, key: string, d: any) {
  order.push("watch:" + key);
  return d;
}

@Service("Report")
class Report {
  constructor(
    @Inject("Clock") readonly clock?: string,
    @Inject("Logger") readonly logger?: string,
  ) {}

  @Watch
  run(@Inject("Scope") scope?: string) {
    return "run:" + String(scope);
  }
}

describe("claude-decorator-inject", () => {
  test("the class is still constructible and its members intact", () => {
    expect(typeof Report).toBe("function");
    expect(new Report("c", "l").run("s")).toBe("run:s");
  });

  test("the registry was actually written", () => {
    expect(registry.join(",")).toBe("Report<-Clock+Logger");
  });

  test("the constructor's parameters run last, in descending index order", () => {
    expect(order.join(",")).toBe(
      "inject:Scope|key=run|idx=0," +
        "watch:run," +
        "inject:Logger|key=undefined|idx=1," +
        "inject:Clock|key=undefined|idx=0," +
        "service:Report",
    );
  });
});
