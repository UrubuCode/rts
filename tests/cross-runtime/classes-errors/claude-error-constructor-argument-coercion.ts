// Cross-runtime: how the Error constructors treat their message argument —
// undefined leaves no own message, everything else is ToString'd — and that
// calling them without `new` builds the same object.
function own(e: any): string {
  return Object.prototype.hasOwnProperty.call(e, "message") + ":" + JSON.stringify(e.message);
}

console.log("undefined=" + own(new Error(undefined)));
console.log("none=" + own(new Error()));
console.log("empty-string=" + own(new Error("")));
console.log("null=" + own(new Error(null as any)));
console.log("zero=" + own(new Error(0 as any)));
console.log("number=" + own(new Error(123 as any)));
console.log("false=" + own(new Error(false as any)));
console.log("nan=" + own(new Error(NaN as any)));
console.log("array=" + own(new Error([1, 2] as any)));
console.log("plain-object=" + own(new Error({} as any)));
console.log("bigint=" + own(new Error(10n as any)));

// ToString runs through Symbol.toPrimitive / toString.
const custom: any = {
  toString(): string {
    return "custom-message";
  },
};
console.log("custom-tostring=" + own(new Error(custom)));

const viaPrimitive: any = {
  [Symbol.toPrimitive](hint: string): string {
    return "prim:" + hint;
  },
};
console.log("toprimitive=" + own(new Error(viaPrimitive)));

// A Symbol message is a TypeError from the coercion.
try {
  new Error(Symbol("s") as any);
  console.log("symbol=no-throw");
} catch (e: any) {
  console.log("symbol=" + e.constructor.name);
}

// A throwing toString propagates out of the constructor.
try {
  new Error({
    toString(): string {
      throw new RangeError("nope");
    },
  } as any);
  console.log("throwing-tostring=no-throw");
} catch (e: any) {
  console.log("throwing-tostring=" + e.constructor.name);
}

// Called without new: same shape, own prototype, fresh object each time.
const called: any = (Error as any)("called");
console.log("called-instanceof=" + (called instanceof Error));
console.log("called-message=" + called.message);
console.log("called-proto=" + (Object.getPrototypeOf(called) === Error.prototype));
console.log("called-distinct=" + ((Error as any)("a") !== (Error as any)("a")));
console.log("called-type=" + (TypeError as any)("t").constructor.name);
console.log("called-range=" + (RangeError as any)("r").constructor.name);
console.log("called-uri=" + (URIError as any)("u").constructor.name);
console.log("called-eval=" + (EvalError as any)("e").constructor.name);
console.log("called-syntax=" + (SyntaxError as any)("s").constructor.name);

// Extra arguments beyond message/options are ignored.
console.log("extra-args=" + own((Error as any)("m", { cause: 1 }, "ignored", 5)));

// Reflect.construct with a foreign newTarget takes its prototype.
class Marker extends Error {}
const foreign: any = Reflect.construct(Error, ["msg"], Marker);
console.log("reflect-proto=" + (Object.getPrototypeOf(foreign) === Marker.prototype));
console.log("reflect-instanceof=" + (foreign instanceof Marker) + ":" + (foreign instanceof Error));
console.log("reflect-message=" + foreign.message);
console.log("reflect-name=" + foreign.name);

// A subclass forwarding no message leaves no own message either.
class Silent extends Error {
  constructor() {
    super();
  }
}
console.log("silent=" + own(new Silent()));
console.log("silent-tostring=" + new Silent().toString());

// Passing the message explicitly as undefined through a subclass.
class Forwarder extends Error {
  constructor(m?: string) {
    super(m);
  }
}
console.log("forward-undefined=" + own(new Forwarder()));
console.log("forward-value=" + own(new Forwarder("v")));

// The constructors are ordinary functions with a prototype and a name.
console.log("error-name=" + Error.name);
console.log("error-proto-of=" + (Object.getPrototypeOf(Error) === Function.prototype));
console.log("type-proto-of=" + (Object.getPrototypeOf(TypeError) === Error));
console.log("error-prototype-desc=" + JSON.stringify(Object.keys(Object.getOwnPropertyDescriptor(Error, "prototype") as any)));
const pd: any = Object.getOwnPropertyDescriptor(Error, "prototype");
console.log("error-prototype-flags=w" + pd.writable + ",e" + pd.enumerable + ",c" + pd.configurable);
console.log("agg-called=" + (AggregateError as any)([], "m").constructor.name);
