// Cross-runtime: which callables can be `new`-ed and which raise a TypeError.
// A function declaration and a class can; an arrow, a generator, an async
// function, a method shorthand and an accessor cannot — and neither can the
// built-ins that are documented as call-only.
function probe(fn: () => any): string {
  try {
    const v = fn();
    return "ok:" + (typeof v === "object" ? "object" : String(v));
  } catch (e: any) {
    return e.constructor.name;
  }
}

function ordinary(): void {
  // nothing
}
const arrow = () => 1;
function* gen(): Generator<number, void, undefined> {
  yield 1;
}
async function asyncFn(): Promise<number> {
  return 1;
}
async function* asyncGen(): AsyncGenerator<number, void, undefined> {
  yield 1;
}
class Klass {}

const holder: any = {
  shorthand(): number {
    return 1;
  },
  *genShorthand(): Generator<number, void, undefined> {
    yield 1;
  },
  async asyncShorthand(): Promise<number> {
    return 1;
  },
  get accessor(): number {
    return 1;
  },
  set accessor(v: number) {
    void v;
  },
  asProperty: function (): number {
    return 1;
  },
  asArrow: () => 1,
};
const accessorDesc: any = Object.getOwnPropertyDescriptor(holder, "accessor");

console.log("ordinary=" + probe(() => new (ordinary as any)()));
console.log("class=" + probe(() => new Klass()));
console.log("arrow=" + probe(() => new (arrow as any)()));
console.log("generator=" + probe(() => new (gen as any)()));
console.log("async=" + probe(() => new (asyncFn as any)()));
console.log("async-generator=" + probe(() => new (asyncGen as any)()));
console.log("shorthand=" + probe(() => new (holder.shorthand as any)()));
console.log("generator-shorthand=" + probe(() => new (holder.genShorthand as any)()));
console.log("async-shorthand=" + probe(() => new (holder.asyncShorthand as any)()));
console.log("getter=" + probe(() => new (accessorDesc.get as any)()));
console.log("setter=" + probe(() => new (accessorDesc.set as any)()));
console.log("function-property=" + probe(() => new (holder.asProperty as any)()));
console.log("arrow-property=" + probe(() => new (holder.asArrow as any)()));

// A class METHOD, a class GETTER and a class STATIC method are all
// non-constructible; the class itself and a class FIELD holding a function
// are not the same case.
class WithMembers {
  method(): number {
    return 1;
  }
  static staticMethod(): number {
    return 1;
  }
  get value(): number {
    return 1;
  }
  field: () => number = function (): number {
    return 1;
  };
}
const wm = new WithMembers();
const valueDesc: any = Object.getOwnPropertyDescriptor(WithMembers.prototype, "value");
console.log("class-method=" + probe(() => new (WithMembers.prototype.method as any)()));
console.log("class-static=" + probe(() => new (WithMembers.staticMethod as any)()));
console.log("class-getter=" + probe(() => new (valueDesc.get as any)()));
console.log("class-field-fn=" + probe(() => new (wm.field as any)()));

// `prototype` is the give-away: only a constructible function owns one.
function hasProto(f: any): string {
  return String(Object.prototype.hasOwnProperty.call(f, "prototype"));
}
console.log("proto-ordinary=" + hasProto(ordinary));
console.log("proto-arrow=" + hasProto(arrow));
console.log("proto-generator=" + hasProto(gen));
console.log("proto-async=" + hasProto(asyncFn));
console.log("proto-shorthand=" + hasProto(holder.shorthand));
console.log("proto-class-method=" + hasProto(WithMembers.prototype.method));
console.log("proto-class=" + hasProto(WithMembers));

// Reflect.construct answers the same question without the syntax.
console.log("reflect-arrow=" + probe(() => Reflect.construct(arrow as any, [])));
console.log("reflect-generator=" + probe(() => Reflect.construct(gen as any, [])));
console.log("reflect-class=" + probe(() => Reflect.construct(Klass, [])));

// Built-ins that refuse `new` by specification.
console.log("new-symbol=" + probe(() => new (Symbol as any)("x")));
console.log("new-bigint=" + probe(() => new (BigInt as any)(1)));
console.log("new-math-max=" + probe(() => new (Math.max as any)(1)));
console.log("new-parseint=" + probe(() => new (parseInt as any)("1")));
console.log("new-isnan=" + probe(() => new (isNaN as any)(1)));
console.log("new-json-parse=" + probe(() => new (JSON.parse as any)("1")));
console.log("new-object-keys=" + probe(() => new (Object.keys as any)({})));
console.log("new-reflect-get=" + probe(() => new (Reflect.get as any)({}, "a")));

// Built-ins that accept both.
console.log("new-object=" + probe(() => typeof new Object()));
console.log("new-array=" + probe(() => new Array(2).length));
console.log("new-date-ok=" + probe(() => typeof new Date(0).getTime()));
console.log("new-map=" + probe(() => new Map().size));
console.log("new-promise=" + probe(() => typeof new Promise(() => undefined)));

// Built-ins that refuse being CALLED without new.
console.log("call-map=" + probe(() => (Map as any)()));
console.log("call-set=" + probe(() => (Set as any)()));
console.log("call-promise=" + probe(() => (Promise as any)(() => undefined)));
console.log("call-proxy=" + probe(() => (Proxy as any)({}, {})));
console.log("call-weakmap=" + probe(() => (WeakMap as any)()));
console.log("call-class=" + probe(() => (Klass as any)()));

// Calling a class through Reflect.apply is refused the same way.
console.log("apply-class=" + probe(() => Reflect.apply(Klass as any, undefined, [])));
console.log("bind-then-call=" + probe(() => (Klass as any).bind(null)()));
console.log("bind-then-new=" + probe(() => typeof new ((Klass as any).bind(null))()));
