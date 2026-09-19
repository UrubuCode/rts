// `JSON.stringify(x)` and `JSON.parse(s)` are called by their entry points when
// the whole program proves `JSON` is the primordial — no global read, no
// property read, no generic call. Nothing here asserts that the substitution
// HAPPENED: every expectation is what the language answers, checked against
// node, and the file passes on a binary from before the change as well.
//
// The same change moved three things under the call, and each has a row:
// a number is written without becoming a string first (`decimal_of`), the
// output starts narrow and widens once (`json/out.rs`), and `parse` moves its
// strings into the heap rather than cloning them.
import { describe, test, expect } from "rts:test";

describe("numbers keep Number::toString's spelling", () => {
  test("integers, fractions and both thresholds", () => {
    expect(JSON.stringify([0, -0, 1, -1, 42, 9007199254740991])).toBe("[0,0,1,-1,42,9007199254740991]");
    expect(JSON.stringify([0.1, 0.1 + 0.2, 1.5, -2.25, 123.456])).toBe("[0.1,0.30000000000000004,1.5,-2.25,123.456]");
    expect(JSON.stringify([1e21, 1e20, 1e-7, 1e-6, 1.5e-10, 1.2345678901234568e20])).toBe(
      "[1e+21,100000000000000000000,1e-7,0.000001,1.5e-10,123456789012345680000]",
    );
    expect(JSON.stringify([5e-324, 1.7976931348623157e308])).toBe("[5e-324,1.7976931348623157e+308]");
  });

  test("what has no JSON spelling is null", () => {
    expect(JSON.stringify([NaN, Infinity, -Infinity])).toBe("[null,null,null]");
  });

  test("String(n) and a template still agree with it", () => {
    expect(String(0.000001234)).toBe("0.000001234");
    expect(`${-1e-7}`).toBe("-1e-7");
    expect(String(2 ** 53)).toBe("9007199254740992");
    expect(String(123456789.125)).toBe("123456789.125");
  });
});

describe("the output widens once, and only when it must", () => {
  test("narrow text with every escape", () => {
    expect(JSON.stringify('a"b\\c\n\r\t\b\f\u0001z')).toBe('"a\\"b\\\\c\\n\\r\\t\\b\\f\\u0001z"');
    expect(JSON.stringify("café")).toBe('"café"');
  });

  test("a wide unit after narrow output keeps what came before", () => {
    const text = JSON.stringify({ plain: "abc", wide: "中文", after: "xyz", n: 1.5 });
    expect(text).toBe('{"plain":"abc","wide":"中文","after":"xyz","n":1.5}');
    expect(text.length).toBe(49);
  });

  test("a lone surrogate is escaped and a pair is not", () => {
    expect(JSON.stringify("\ud800")).toBe('"\\ud800"');
    expect(JSON.stringify("😀")).toBe('"😀"');
    expect(JSON.stringify(["a", "\udc00", "b"])).toBe('["a","\\udc00","b"]');
  });

  test("indentation still goes through", () => {
    expect(JSON.stringify({ a: [1, "中"] }, null, 2)).toBe('{\n  "a": [\n    1,\n    "中"\n  ]\n}');
  });
});

describe("parse builds the same values it always did", () => {
  test("strings, keys and escapes survive the move", () => {
    const parsed = JSON.parse('{"name":"alice","wide":"中","esc":"a\\nb\\u0041","list":["x","y"],"n":-0.5}');
    expect(parsed.name).toBe("alice");
    expect(parsed.wide).toBe("中");
    expect(parsed.esc).toBe("a\nbA");
    expect(parsed.list.join("+")).toBe("x+y");
    expect(parsed.n).toBe(-0.5);
    expect(Object.keys(parsed).join()).toBe("name,wide,esc,list,n");
  });

  test("the argument is converted, not required to be text", () => {
    expect(JSON.parse(5 as unknown as string)).toBe(5);
    expect(JSON.parse([1] as unknown as string)).toBe(1);
    expect(JSON.parse({ toString: () => "[1,2]" } as unknown as string).length).toBe(2);
  });

  test("a round trip of many same-shaped rows", () => {
    const rows = [];
    for (let i = 0; i < 300; i++) rows.push({ id: i, name: "user" + i, score: i * 1.5, tags: ["a", "b"] });
    const back = JSON.parse(JSON.stringify(rows));
    let sum = 0;
    for (const row of back) sum += row.id + row.score + row.tags.length + row.name.length;
    expect(back.length).toBe(300);
    expect(sum).toBe(44850 + 67275 + 600 + 1990);
  });
});

describe("what the call raises is still catchable where it is written", () => {
  test("a cycle", () => {
    const loop: Record<string, unknown> = {};
    loop.self = loop;
    let caught = "";
    try {
      JSON.stringify(loop);
    } catch (error) {
      caught = (error as Error).constructor.name;
    }
    expect(caught).toBe("TypeError");
  });

  test("a bigint, and bad text", () => {
    const raised = (run: () => unknown) => {
      try {
        run();
      } catch (error) {
        return (error as Error).constructor.name;
      }
      return "nothing";
    };
    expect(raised(() => JSON.stringify(1n))).toBe("TypeError");
    expect(raised(() => JSON.parse("{"))).toBe("SyntaxError");
  });

  test("a toJSON that throws, and the statement after the catch still runs", () => {
    const bad = { toJSON() { throw new RangeError("no"); } };
    let seen = "";
    try {
      JSON.stringify([1, bad, 2]);
    } catch (error) {
      seen = (error as Error).message;
    }
    expect(seen).toBe("no");
    expect(JSON.stringify([1, 2])).toBe("[1,2]");
  });
});

describe("the argument forms that stay the ordinary call", () => {
  test("none, undefined, a function, and more than one", () => {
    expect((JSON.stringify as () => unknown)()).toBe(undefined);
    expect(JSON.stringify(undefined)).toBe(undefined);
    expect(JSON.stringify(() => 1)).toBe(undefined);
    expect(JSON.stringify({ a: 1, b: 2 }, ["b"])).toBe('{"b":2}');
    expect(JSON.parse("[1,2]", (key, value) => (typeof value === "number" ? value * 2 : value))[1]).toBe(4);
    const parts: [string] = ["[7]"];
    expect(JSON.parse(...parts)[0]).toBe(7);
  });

  test("the argument is evaluated once, and before the call", () => {
    let count = 0;
    const text = JSON.stringify((count++, { count }));
    expect(text).toBe('{"count":1}');
    expect(count).toBe(1);
  });
});

describe("a binding named JSON is the scope's, not the global's", () => {
  test("a parameter", () => {
    function through(JSON: { stringify(value: unknown): string }) {
      return JSON.stringify(1);
    }
    expect(through({ stringify: () => "mine" })).toBe("mine");
  });

  test("a block-scoped const", () => {
    {
      const JSON = { parse: (text: string) => text + "!" };
      expect(JSON.parse("x")).toBe("x!");
    }
    expect(JSON.parse("1")).toBe(1);
  });
});

describe("what is remembered from one row is not assumed of the next", () => {
  test("a hidden member on the second of two same-shaped rows", () => {
    const first = { a: 1, b: 2 };
    const second = { a: 3, b: 4 };
    Object.defineProperty(second, "b", { enumerable: false });
    const third = { a: 5, b: 6 };
    expect(JSON.stringify([first, second, third])).toBe('[{"a":1,"b":2},{"a":3},{"a":5,"b":6}]');
    expect(JSON.stringify([second, first])).toBe('[{"a":3},{"a":1,"b":2}]');
  });

  test("rows whose keys differ, reorder, repeat and shrink", () => {
    const text = '[{"a":1,"b":2,"c":3},{"a":4,"c":5,"b":6},{"a":7},{"a":8,"a":9,"b":10},{"x":{"a":1},"a":2},{"x":{"b":1},"a":3}]';
    const rows = JSON.parse(text);
    expect(Object.keys(rows[1]).join()).toBe("a,c,b");
    expect(rows[1].c + rows[1].b).toBe(11);
    expect(Object.keys(rows[2]).join()).toBe("a");
    expect(rows[3].a).toBe(9);
    expect(rows[3].b).toBe(10);
    expect(rows[4].x.a + rows[4].a).toBe(3);
    expect(rows[5].x.b + rows[5].a).toBe(4);
    expect(JSON.stringify(rows)).toBe('[{"a":1,"b":2,"c":3},{"a":4,"c":5,"b":6},{"a":7},{"a":9,"b":10},{"x":{"a":1},"a":2},{"x":{"b":1},"a":3}]');
  });
});

describe("a primitive written in one borrow is still read when the language reads it", () => {
  test("an element's toJSON that shrinks the array behind it", () => {
    const list: unknown[] = [];
    list.push({ toJSON() { list.length = 2; return "cut"; } }, 1, 2, 3);
    expect(JSON.stringify(list)).toBe('["cut",1,null,null]');
  });

  test("an own undefined element is null", () => {
    expect(JSON.stringify([1, undefined, 3])).toBe("[1,null,3]");
    expect(JSON.stringify([1, , 3])).toBe("[1,null,3]");
  });

  test("a function replacer still sees every primitive, in order", () => {
    const keys: string[] = [];
    const text = JSON.stringify({ a: 1, b: ["x", 2], c: "s" }, (key, value) => {
      keys.push(key);
      return typeof value === "number" ? value + 1 : value;
    });
    expect(text).toBe('{"a":2,"b":["x",3],"c":"s"}');
    expect(keys.join()).toBe(",a,b,0,1,c");
  });

  test("a member getter runs once per serialisation", () => {
    let runs = 0;
    const row = { a: 1, get b() { runs++; return 2; }, c: 3 };
    expect(JSON.stringify([row, row])).toBe('[{"a":1,"b":2,"c":3},{"a":1,"b":2,"c":3}]');
    expect(runs).toBe(2);
  });

  test("wrappers and their hooks are objects, not primitives", () => {
    const boxed = new Number(5) as Number & { toJSON?: () => string };
    expect(JSON.stringify([boxed, new String("s"), new Boolean(false)])).toBe('[5,"s",false]');
    boxed.toJSON = () => "hooked";
    expect(JSON.stringify({ boxed })).toBe('{"boxed":"hooked"}');
  });
});
