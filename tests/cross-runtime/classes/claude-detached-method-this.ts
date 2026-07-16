// Cross-runtime: a class method detached from its receiver loses `this`.
// Class bodies are always strict mode, so `this` is undefined (not globalThis).
class Counter {
  n: number;
  constructor(n: number) {
    this.n = n;
  }
  get(): number {
    return this.n;
  }
  safe(): string {
    return this === undefined ? "no-this" : "has-this:" + this.n;
  }
  arrow: () => string = () => "arrow:" + this.n;
}

const c = new Counter(7);
console.log("attached=" + c.get());
console.log("attached_safe=" + c.safe());

// detached: `this` is undefined -> reading a property throws TypeError
const m = c.get;
try {
  m();
  console.log("detached=no-throw");
} catch (e) {
  console.log("detached_throws=" + (e instanceof TypeError));
}

// a detached method that never touches a property observes undefined `this`
const s = c.safe;
console.log("detached_safe=" + s());

// re-attaching via call/apply/bind
console.log("call=" + m.call(c));
console.log("apply=" + m.apply(c, []));
const bound = m.bind(c);
console.log("bind=" + bound());
console.log("bind_twice=" + bound.bind(new Counter(99))());

// borrowed by a different receiver (duck typing)
console.log("borrow_obj=" + m.call({ n: 42 }));
console.log("borrow_other=" + m.call(new Counter(5)));

// arrow field keeps `this` even when detached
const a = c.arrow;
console.log("arrow_detached=" + a());
console.log("arrow_call_other=" + a.call(new Counter(1)));

// passing a method as a callback loses `this`; the arrow field does not
const nums = [1];
try {
  console.log("cb_method=" + nums.map(c.get).join(","));
} catch (e) {
  console.log("cb_method_throws=" + (e instanceof TypeError));
}
console.log("cb_arrow=" + nums.map(c.arrow).join(","));
console.log("cb_bound=" + nums.map(c.get.bind(c)).join(","));
console.log("cb_wrapped=" + nums.map(() => c.get()).join(","));

// detached through a variable holding the method of an array element
const arr = [new Counter(11), new Counter(22)];
const f0 = arr[0].get;
try {
  f0();
  console.log("elem_detached=no-throw");
} catch (e) {
  console.log("elem_detached_throws=" + (e instanceof TypeError));
}
console.log("elem_bound=" + f0.call(arr[1]));

// object destructuring a method also detaches it
const { get } = c;
try {
  get();
  console.log("destructured=no-throw");
} catch (e) {
  console.log("destructured_throws=" + (e instanceof TypeError));
}
