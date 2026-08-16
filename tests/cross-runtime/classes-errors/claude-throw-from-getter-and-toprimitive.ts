// Cross-runtime: an exception raised inside a getter, a Symbol.toPrimitive, a
// toString/valueOf or an iterator propagates out of the built-in that called it
// — JSON.stringify, string concatenation, spread and Array.from included.
function caught(fn: () => any): string {
  try {
    const v = fn();
    return "ok:" + String(v);
  } catch (e: any) {
    return e.constructor.name;
  }
}

// JSON.stringify walks getters and lets the throw through.
const withGetter: any = {
  a: 1,
  get b(): number {
    throw new RangeError("getter");
  },
};
console.log("stringify-getter=" + caught(() => JSON.stringify(withGetter)));
console.log("stringify-ok=" + caught(() => JSON.stringify({ a: 1 })));

// A throwing toJSON is reached only for the value that has it.
const withToJSON: any = {
  a: 1,
  b: {
    toJSON(): any {
      throw new EvalError("toJSON");
    },
  },
};
console.log("stringify-tojson=" + caught(() => JSON.stringify(withToJSON)));

// A throwing replacer, and a throwing getter behind an array index.
console.log(
  "stringify-replacer=" +
    caught(() =>
      JSON.stringify({ a: 1 }, () => {
        throw new TypeError("replacer");
      }),
    ),
);
const arr: any = [1];
Object.defineProperty(arr, "1", {
  get(): number {
    throw new URIError("index");
  },
  enumerable: true,
  configurable: true,
});
console.log("stringify-index=" + caught(() => JSON.stringify(arr)));

// Symbol.toPrimitive wins over toString/valueOf and its throw escapes.
const prim: any = {
  [Symbol.toPrimitive](hint: string): any {
    throw new RangeError("prim:" + hint);
  },
  toString(): string {
    return "never";
  },
  valueOf(): number {
    return 0;
  },
};
console.log("concat=" + caught(() => "x" + prim));
console.log("template=" + caught(() => `${prim}`));
console.log("plus=" + caught(() => (prim as any) + 1));
console.log("number=" + caught(() => Number(prim)));
console.log("string=" + caught(() => String(prim)));
console.log("compare=" + caught(() => (prim as any) < 1));
console.log("key=" + caught(() => ({} as any)[prim]));

// The hint each operation asks for is observable.
const hints: string[] = [];
const hinted: any = {
  [Symbol.toPrimitive](hint: string): any {
    hints.push(hint);
    return 1;
  },
};
"" + hinted;
`${hinted}`;
Number(hinted);
String(hinted);
(hinted as any) < 1;
console.log("hints=" + hints.join(","));

// Without Symbol.toPrimitive, valueOf is tried first for "default"/"number".
const order: string[] = [];
const ordinary: any = {
  valueOf(): any {
    order.push("valueOf");
    return {};
  },
  toString(): any {
    order.push("toString");
    throw new TypeError("toString");
  },
};
console.log("ordinary=" + caught(() => "" + ordinary));
console.log("order=" + order.join(","));

// Both non-primitive: TypeError from OrdinaryToPrimitive itself.
const neither: any = {
  valueOf(): any {
    return {};
  },
  toString(): any {
    return {};
  },
};
console.log("neither=" + caught(() => "" + neither));

// A throwing Symbol.iterator escapes spread, for-of, Array.from and destructuring.
const badIterable: any = {
  [Symbol.iterator](): any {
    throw new RangeError("iter");
  },
};
console.log("spread=" + caught(() => [...badIterable]));
console.log("array-from=" + caught(() => Array.from(badIterable)));
console.log("destructure=" + caught(() => { const [x] = badIterable; return x; }));
console.log("map-ctor=" + caught(() => new Map(badIterable)));

// A throwing next() likewise.
const badNext: any = {
  [Symbol.iterator](): any {
    return {
      next(): any {
        throw new EvalError("next");
      },
    };
  },
};
console.log("bad-next=" + caught(() => [...badNext]));

// An iterator whose return() throws during an early exit from for-of.
const log: string[] = [];
const badReturn: any = {
  [Symbol.iterator](): any {
    let i = 0;
    return {
      next(): any {
        i = i + 1;
        return { value: i, done: i > 3 };
      },
      return(): any {
        log.push("return-called");
        throw new URIError("return");
      },
    };
  },
};
console.log(
  "bad-return=" +
    caught(() => {
      for (const v of badReturn) {
        if (v === 2) break;
      }
      return "finished";
    }),
);
console.log("return-log=" + log.join(","));

// A getter that throws during Object.assign and structured spread.
const src: any = {
  get x(): number {
    throw new TypeError("assign");
  },
};
console.log("assign=" + caught(() => Object.assign({}, src)));
console.log("obj-spread=" + caught(() => ({ ...src })));
console.log("entries=" + caught(() => Object.entries(src)));
console.log("values=" + caught(() => Object.values(src)));
console.log("keys-safe=" + caught(() => Object.keys(src).join(",")));
