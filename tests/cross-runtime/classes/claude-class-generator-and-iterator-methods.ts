// Cross-runtime: generator methods, static generators, an async method and
// Symbol.iterator declared in a class body — their prototypes, names, tags and
// what each one is an instance of.
class Seq {
  items: number[] = [1, 2, 3];

  *[Symbol.iterator](): Generator<number> {
    for (const it of this.items) yield it;
  }
  *pairs(): Generator<string> {
    let i = 0;
    while (i < this.items.length) {
      yield i + ":" + this.items[i];
      i = i + 1;
    }
  }
  static *count(n: number): Generator<number> {
    let i = 0;
    while (i < n) {
      yield i;
      i = i + 1;
    }
  }
  async load(): Promise<string> {
    return "loaded";
  }
  async *stream(): AsyncGenerator<number> {
    yield 1;
    yield 2;
  }
}

const s = new Seq();
console.log("spread=" + [...s].join(","));
console.log("for-of=" + Array.from(s).join(","));
console.log("pairs=" + [...s.pairs()].join("|"));
console.log("static-count=" + [...Seq.count(4)].join(","));

const g = s.pairs();
console.log("gen-tostring=" + Object.prototype.toString.call(g));
console.log("gen-next=" + JSON.stringify(g.next()));
console.log("gen-return=" + JSON.stringify(g.return("stop" as any)));
console.log("gen-after=" + JSON.stringify(g.next()));

const g2 = s.pairs();
g2.next();
let thrown = "none";
try {
  g2.throw(new RangeError("x"));
} catch (e: any) {
  thrown = e.constructor.name;
}
console.log("gen-throw=" + thrown);

// Method shapes.
const proto: any = Seq.prototype;
console.log("iterator-is-own=" + Object.getOwnPropertySymbols(proto).length);
console.log("iterator-name=" + (proto[Symbol.iterator] as any).name);
console.log("pairs-name=" + proto.pairs.name);
console.log("count-name=" + Seq.count.name);
console.log("load-name=" + proto.load.name);
console.log("stream-name=" + proto.stream.name);

console.log("pairs-ctor=" + proto.pairs.constructor.name);
console.log("load-ctor=" + proto.load.constructor.name);
console.log("stream-ctor=" + proto.stream.constructor.name);
console.log("count-ctor=" + Seq.count.constructor.name);

console.log("pairs-has-prototype=" + Object.prototype.hasOwnProperty.call(proto.pairs, "prototype"));
console.log("load-has-prototype=" + Object.prototype.hasOwnProperty.call(proto.load, "prototype"));

const genProtoDesc: any = Object.getOwnPropertyDescriptor(proto.pairs, "prototype");
console.log("gen-prototype-writable=" + genProtoDesc.writable);
console.log("gen-prototype-enumerable=" + genProtoDesc.enumerable);
console.log("gen-prototype-configurable=" + genProtoDesc.configurable);

// Generator objects inherit from the method's .prototype.
console.log("gen-proto-chain=" + (Object.getPrototypeOf(s.pairs()) === proto.pairs.prototype));
console.log("gen-tag=" + (Object.getPrototypeOf(Object.getPrototypeOf(s.pairs())) as any)[Symbol.toStringTag]);

// Generator methods are not constructors.
try {
  new (proto.pairs as any)();
  console.log("new-gen=no-throw");
} catch (e: any) {
  console.log("new-gen=" + e.constructor.name);
}

// A generator's iterator is itself.
const g3 = s.pairs();
console.log("self-iterable=" + ((g3 as any)[Symbol.iterator]() === g3));

// Async method answers a promise; the async generator answers an async iterator.
const out: string[] = [];
s.load().then((v) => {
  out.push("load=" + v);
  const it = s.stream();
  return it.next().then((r) => {
    out.push("stream1=" + r.value + ":" + r.done);
    return it.next().then((r2) => {
      out.push("stream2=" + r2.value + ":" + r2.done);
      return it.next().then((r3) => {
        out.push("stream3=" + String(r3.value) + ":" + r3.done);
        console.log("async=" + out.join("|"));
      });
    });
  });
});
console.log("sync-tail=reached");
