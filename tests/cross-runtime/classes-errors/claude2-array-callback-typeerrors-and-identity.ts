// Cross-runtime: what the Array higher-order methods do when their callback is
// missing, is not callable, or throws. Every non-callable argument is a
// TypeError raised BEFORE any element is visited, and a thrown error comes out
// as the identical object with the iteration abandoned mid-way.
function probe(fn: () => any): string {
  try {
    const v = fn();
    return "ok:" + String(v);
  } catch (e: any) {
    return e.constructor.name;
  }
}

const nums = [1, 2, 3, 4];

// Missing callback.
console.log("map-missing=" + probe(() => (nums as any).map()));
console.log("filter-missing=" + probe(() => (nums as any).filter()));
console.log("foreach-missing=" + probe(() => (nums as any).forEach()));
console.log("some-missing=" + probe(() => (nums as any).some()));
console.log("every-missing=" + probe(() => (nums as any).every()));
console.log("find-missing=" + probe(() => (nums as any).find()));
console.log("findlast-missing=" + probe(() => (nums as any).findLast()));
console.log("flatmap-missing=" + probe(() => (nums as any).flatMap()));
console.log("reduce-missing=" + probe(() => (nums as any).reduce()));

// Non-callable callback of assorted kinds.
console.log("map-number=" + probe(() => (nums as any).map(5)));
console.log("map-string=" + probe(() => (nums as any).map("x")));
console.log("map-null=" + probe(() => (nums as any).map(null)));
console.log("map-object=" + probe(() => (nums as any).map({})));
console.log("sort-number=" + probe(() => nums.slice().sort(5 as any)));
console.log("sort-object=" + probe(() => nums.slice().sort({} as any)));
console.log("sort-null-ok=" + probe(() => [3, 1, 2].sort(null as any).join(",")));
console.log("sort-undefined-ok=" + probe(() => [3, 1, 2].sort(undefined).join(",")));

// The check happens before the first visit: nothing is touched.
const touched: string[] = [];
const watched: any = [1, 2];
Object.defineProperty(watched, "0", {
  get(): number {
    touched.push("read0");
    return 1;
  },
  configurable: true,
  enumerable: true,
});
console.log("early-check=" + probe(() => watched.map(5)));
console.log("early-touched=" + JSON.stringify(touched.join(",")));
console.log("late-run=" + probe(() => watched.map((v: any) => v).join(",")));
console.log("late-touched=" + touched.join(","));

// reduce on an empty array with no initial value is a TypeError; with one, it
// is simply the initial value.
console.log("reduce-empty=" + probe(() => ([] as number[]).reduce((a, b) => a + b)));
console.log("reduce-empty-init=" + probe(() => ([] as number[]).reduce((a, b) => a + b, 7)));
console.log("reduce-holes=" + probe(() => ([, , ] as any[]).reduce((a: any, b: any) => a + b)));
console.log("reduceright-empty=" + probe(() => ([] as number[]).reduceRight((a, b) => a + b)));
console.log("reduce-single=" + probe(() => [9].reduce((a, b) => a + b)));

// A throwing callback surfaces the identical object and stops the walk.
const marker = new RangeError("mine");
const visited: number[] = [];
let identity = "none";
try {
  nums.map((v) => {
    visited.push(v);
    if (v === 3) {
      throw marker;
    }
    return v;
  });
} catch (e: any) {
  identity = String(e === marker) + ":" + e.constructor.name + ":" + e.message;
}
console.log("throwing-identity=" + identity);
console.log("throwing-visited=" + visited.join(","));

// The same for forEach, sort and reduce.
console.log("foreach-throws=" + probe(() => nums.forEach(() => {
  throw new URIError("fe");
})));
console.log("sort-comparator-throws=" + probe(() => nums.slice().sort(() => {
  throw new EvalError("cmp");
})));
console.log("reduce-throws=" + probe(() => nums.reduce(() => {
  throw new SyntaxError("rd");
}, 0)));
console.log("flatmap-throws=" + probe(() => nums.flatMap(() => {
  throw new URIError("fm");
})));

// A comparator that answers inconsistently is legal — no error, just an
// unspecified order, so only the length is asserted.
console.log("bad-comparator-len=" + probe(() => [3, 1, 2].sort(() => 0).length));

// Array.from and Array.of with a non-callable mapper.
console.log("from-bad-mapper=" + probe(() => Array.from([1, 2], 5 as any)));
console.log("from-mapper-throws=" + probe(() => Array.from([1, 2], () => {
  throw new TypeError("fm2");
})));
console.log("from-non-iterable-ok=" + probe(() => Array.from({ length: 2 } as any).length));
console.log("from-null=" + probe(() => Array.from(null as any)));
console.log("from-number=" + probe(() => Array.from(5 as any).length));

// A brand check: these methods are generic over array-likes but not over
// null/undefined receivers.
console.log("map-on-arraylike=" + probe(() => Array.prototype.map.call({ length: 2, 0: "a", 1: "b" } as any, (v: any) => v).join(",")));
console.log("map-on-string=" + probe(() => Array.prototype.map.call("ab" as any, (v: any) => v + "!").join(",")));
console.log("map-on-null=" + probe(() => Array.prototype.map.call(null as any, (v: any) => v)));
console.log("map-on-undefined=" + probe(() => Array.prototype.map.call(undefined as any, (v: any) => v)));
console.log("foreach-on-number=" + probe(() => Array.prototype.forEach.call(5 as any, () => undefined)));

// Writing through a frozen array from inside a callback: class bodies are
// strict, so the probe runs inside one to make the outcome strictness-free.
class Writer {
  static intoFrozen(): string {
    const frozen = Object.freeze([1, 2]);
    try {
      (frozen as any)[0] = 9;
      return "no-throw:" + frozen[0];
    } catch (e: any) {
      return e.constructor.name;
    }
  }
  static push(): string {
    const frozen = Object.freeze([1, 2]);
    try {
      (frozen as any).push(3);
      return "no-throw";
    } catch (e: any) {
      return e.constructor.name;
    }
  }
}
console.log("frozen-write=" + Writer.intoFrozen());
console.log("frozen-push=" + Writer.push());
