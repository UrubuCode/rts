// Cross-runtime: async methods, static async methods and async generator
// methods on a class — what they return before being awaited, how `this` and
// `super` reach them, and the order a for-await draws values in.
const log: string[] = [];

class Source {
  base: number = 10;

  async one(): Promise<number> {
    log.push("one-enter");
    const v = await this.base;
    log.push("one-exit");
    return v + 1;
  }

  static async build(n: number): Promise<string> {
    log.push("build:" + n);
    return "built-" + (await n);
  }

  async *pairs(): AsyncGenerator<string, string, undefined> {
    log.push("gen-start");
    yield "p" + this.base;
    log.push("gen-mid");
    yield "q" + (await 2);
    log.push("gen-end");
    return "done";
  }

  *plain(): Generator<number, void, undefined> {
    yield this.base;
    yield this.base + 1;
  }

  [Symbol.asyncIterator](): any {
    return this.pairs();
  }
}

class Derived extends Source {
  async one(): Promise<number> {
    log.push("derived-one");
    const fromBase = await super.one();
    return fromBase * 2;
  }
  arrowAsync: () => Promise<string> = async () => "arrow:" + this.base;
}

const s = new Source();

// Shape questions, answered synchronously.
const p = s.one();
console.log("returns-promise=" + (p instanceof Promise));
console.log("returns-thenable=" + (typeof (p as any).then === "function"));
console.log("ctor-name=" + p.constructor.name);

const g: any = s.pairs();
console.log("gen-is-object=" + (typeof g === "object"));
console.log("gen-has-next=" + (typeof g.next === "function"));
console.log("gen-has-async-iterator=" + (typeof g[Symbol.asyncIterator] === "function"));
console.log("gen-self=" + (g[Symbol.asyncIterator]() === g));
console.log("gen-sync-iterator=" + (typeof g[Symbol.iterator]));

// Descriptors: methods of every flavour are non-enumerable, writable and
// configurable, exactly like an ordinary method.
function desc(o: any, k: any): string {
  const d: any = Object.getOwnPropertyDescriptor(o, k);
  return "w" + d.writable + ",e" + d.enumerable + ",c" + d.configurable + ",fn" + (typeof d.value === "function");
}
console.log("desc-async=" + desc(Source.prototype, "one"));
console.log("desc-asyncgen=" + desc(Source.prototype, "pairs"));
console.log("desc-static=" + desc(Source, "build"));
console.log("name-async=" + Source.prototype.one.name);
console.log("name-asyncgen=" + Source.prototype.pairs.name);
console.log("proto-names=" + Object.getOwnPropertyNames(Source.prototype).sort().join(","));
console.log("proto-symbols=" + Object.getOwnPropertySymbols(Source.prototype).length);

// An async method is not a constructor.
let newAsync = "no-throw";
try {
  new (Source.prototype.one as any)();
} catch (e: any) {
  newAsync = e.constructor.name;
}
console.log("new-async=" + newAsync);

// The sync generator method still behaves as one.
console.log("plain=" + Array.from(s.plain()).join(","));

async function run(): Promise<void> {
  console.log("await-one=" + (await p));

  const d = new Derived();
  console.log("derived-one=" + (await d.one()));
  console.log("derived-arrow=" + (await d.arrowAsync()));

  console.log("static=" + (await Source.build(4)));

  const seen: string[] = [];
  for await (const v of s.pairs()) {
    seen.push(v);
  }
  console.log("for-await=" + seen.join("|"));

  // The class itself is async-iterable through the async method key.
  const viaClass: string[] = [];
  for await (const v of s as any) {
    viaClass.push(v);
  }
  console.log("via-class=" + viaClass.join("|"));

  // next() drives the async generator step by step and reports done/value.
  const it: any = s.pairs();
  const a = await it.next();
  const b = await it.next();
  const c = await it.next();
  const e = await it.next();
  console.log("step-a=" + a.value + ":" + a.done);
  console.log("step-b=" + b.value + ":" + b.done);
  console.log("step-c=" + c.value + ":" + c.done);
  console.log("step-e=" + e.value + ":" + e.done);

  // return() closes it early.
  const it2: any = s.pairs();
  console.log("early=" + (await it2.next()).value);
  const closed = await it2.return("stopped");
  console.log("closed=" + closed.value + ":" + closed.done);

  console.log("log=" + log.join(">"));
}

run().then(() => {
  console.log("tail=reached");
});
console.log("sync-end=true");
