// Cross-runtime: an arrow has no bindings of its own — no `this`, no
// `arguments`, no `new.target`, no `prototype` and no way to be constructed.
// Each of those is inherited from the enclosing function instead, so `call`,
// `apply` and `bind` can supply arguments but never a receiver.

class Host {
  label: string;
  constructor(label: string) {
    this.label = label;
  }

  // Every arrow made here closes over this instance.
  makeReader(): any {
    return (suffix: string): string => this.label + suffix;
  }

  makeNested(): any {
    return (): any => (): string => this.label + "/nested";
  }

  // A method's `arguments` is visible to an arrow declared inside it. The
  // parameter list has a default, so the object is unmapped in every host.
  countsOuter(a: number = 0): any {
    return (): string => arguments.length + ":" + String((arguments as any)[0]);
  }

  newTargetProbe(): any {
    return (): string => String(new.target);
  }
}

const host = new Host("H");
const other: any = { label: "OTHER" };
const reader = host.makeReader();

// 1) `call`, `apply` and `bind` cannot change an arrow's receiver, but the
//    arguments go through.
console.log("plain=" + reader("-plain"));
console.log("call=" + reader.call(other, "-call"));
console.log("apply=" + reader.apply(other, ["-apply"]));
console.log("bind=" + reader.bind(other)("-bind"));
console.log("bind_partial=" + reader.bind(other, "-partial")());

// 2) Binding twice changes nothing about the receiver either.
console.log("double_bind=" + reader.bind(other).bind({ label: "THIRD" })("-double"));

// 3) Nested arrows keep reaching the same `this`.
console.log("nested=" + host.makeNested()()());

// 4) An arrow reads `this` LIVE from the enclosing scope, so a later mutation
//    of the instance is visible.
host.label = "H2";
console.log("live_this=" + reader("-live"));
host.label = "H";

// 5) Two arrows made by the same method are different objects that agree.
const readerB = host.makeReader();
console.log("distinct_objects=" + (reader !== readerB) + "|same_answer=" +
  (reader("-x") === readerB("-x")));

// 6) An arrow has no own `prototype` and cannot be constructed.
console.log("has_prototype=" + Object.prototype.hasOwnProperty.call(reader, "prototype"));
console.log("own_names=" + Object.getOwnPropertyNames(reader).sort().join(","));
function tryNew(fn: any): string {
  try {
    return "made:" + typeof new fn("x");
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
}
console.log("new_arrow=" + tryNew(reader));
console.log("new_bound_arrow=" + tryNew(reader.bind(other)));

// 7) `new.target` inside an arrow is the enclosing function's, so a method's
//    arrow always sees undefined.
console.log("new_target_in_method_arrow=" + host.newTargetProbe()());

// 8) In a constructor, the arrow sees the constructor's own `new.target`.
class Probe {
  reportOwn: any;
  constructor() {
    this.reportOwn = (): string => (new.target === undefined ? "undefined" : new.target.name);
  }
}
class SubProbe extends Probe {}
console.log("new_target_direct=" + new Probe().reportOwn());
console.log("new_target_subclass=" + new SubProbe().reportOwn());
console.log("new_target_reflect=" + Reflect.construct(Probe, [], SubProbe).reportOwn());

// 9) An ordinary function's `new.target` is undefined when called and the
//    function itself when constructed.
function ordinary(): any {
  return { seen: new.target === undefined ? "called" : "constructed:" + new.target.name };
}
console.log("ordinary_called=" + ordinary().seen);
console.log("ordinary_constructed=" + new (ordinary as any)().seen);
console.log("ordinary_via_reflect=" + Reflect.construct(ordinary as any, []).seen);
console.log("ordinary_reflect_target=" + Reflect.construct(ordinary as any, [], Probe).seen);

// 10) An arrow inside a method sees that method's `arguments`.
console.log("arguments_from_arrow=" + host.countsOuter(5, 6 as any)());

// 11) An arrow has no `arguments` of its own even when given some.
function outerWithArguments(a: number = 0): any {
  const inner = (...rest: any[]): string =>
    "outer=" + arguments.length + " inner_rest=" + rest.length;
  return inner;
}
console.log("arrow_rest_vs_arguments=" + outerWithArguments(1, 2 as any)(9, 9, 9));

// 12) An arrow assigned as an object's method does NOT get that object.
const withArrow: any = {
  label: "literal",
  read: reader,
};
console.log("arrow_as_method=" + withArrow.read("-as-method"));

// 13) A normal function in the same position does.
const withMethod: any = {
  label: "literal",
  read(suffix: string): string {
    return this.label + suffix;
  },
};
console.log("method_as_method=" + withMethod.read("-method"));
// Taken out and given a receiver back, it follows whoever is supplied. (An
// extraction with NO receiver is left untested: what `this` becomes then
// depends on the module mode the host chose for this file.)
const takenOut = withMethod.read;
console.log("method_taken_out_rebound=" + takenOut.call({ label: "given" }, "-rebound"));
console.log("method_taken_out_bound=" + takenOut.bind(withMethod)("-bound"));
console.log("arrow_taken_out_rebound=" + reader.call({ label: "given" }, "-ignored"));

// 14) An arrow's `length` and `name` behave like any function's, and `bind`
//     adjusts them the same way.
const twoParams = (a: number, b: number): number => a + b;
console.log("arrow_length=" + twoParams.length + "|bound=" + twoParams.bind(null, 1).length);
console.log("arrow_typeof=" + typeof twoParams + "|instanceof=" + (twoParams instanceof Function));

// 15) An arrow used as a callback needs no binding, which is the point.
class Adder {
  total = 0;
  addAll(values: number[]): number {
    values.forEach((v) => {
      this.total += v;
    });
    return this.total;
  }
  addAllLoose(values: number[]): string {
    try {
      values.forEach(function (v: number): void {
        (this as any).total += v;
      });
      return "ok:" + this.total;
    } catch (e) {
      return "threw:" + (e as any).constructor.name;
    }
  }
}
console.log("arrow_callback=" + new Adder().addAll([1, 2, 3]));
console.log("function_callback_loose=" + new Adder().addAllLoose([1, 2, 3]));

// 16) A default parameter that is an arrow closes over the same `this`.
class Defaults {
  label = "D";
  run(make: any = (): string => this.label + "-default"): string {
    return make();
  }
}
console.log("default_arrow=" + new Defaults().run());
console.log("default_supplied=" + new Defaults().run(() => "supplied"));
