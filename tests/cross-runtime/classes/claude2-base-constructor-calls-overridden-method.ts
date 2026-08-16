// Cross-runtime: a base constructor that calls an overridden method runs the
// DERIVED override with the derived fields not yet installed — across three
// levels, every layer's fields land after its own super() returns.
const log: string[] = [];

class L1 {
  a: string = mark("L1.field.a", "a1");
  b: string = mark("L1.field.b", "b1");

  constructor() {
    log.push("L1.ctor.enter a=" + this.a);
    log.push("L1.ctor.describe=" + this.describe());
    log.push("L1.ctor.exit");
  }

  describe(): string {
    return "L1(" + this.a + "," + this.b + ")";
  }
}

class L2 extends L1 {
  c: string = mark("L2.field.c", "c2");
  d: string = mark("L2.field.d", "a-was:" + this.a);

  constructor() {
    log.push("L2.ctor.before-super");
    super();
    log.push("L2.ctor.after-super c=" + this.c + " d=" + this.d);
  }

  describe(): string {
    // Reached from L1's constructor, before c and d exist.
    return "L2[" + String(this.c) + "|" + super.describe() + "]";
  }
}

class L3 extends L2 {
  e: string = mark("L3.field.e", "e3");

  constructor() {
    log.push("L3.ctor.before-super");
    super();
    log.push("L3.ctor.after-super e=" + this.e);
  }

  describe(): string {
    return "L3{" + String(this.e) + "+" + super.describe() + "}";
  }
}

function mark(tag: string, value: string): string {
  log.push(tag);
  return value;
}

const inst = new L3();
console.log("final-describe=" + inst.describe());
console.log("a=" + inst.a);
console.log("c=" + inst.c);
console.log("e=" + inst.e);
console.log("keys=" + Object.keys(inst).join(","));
for (let i = 0; i < log.length; i++) {
  console.log("s" + (i < 10 ? "0" : "") + i + "=" + log[i]);
}
console.log("log-len=" + log.length);

// The same shape with an ACCESSOR the base constructor writes through: the
// derived setter runs, and the field declared in the derived class then
// [[Define]]s over whatever the setter stored.
const acc: string[] = [];
class P {
  constructor() {
    (this as any).value = "written-by-base";
    acc.push("base-wrote:" + String((this as any).value));
  }
}
class Q extends P {
  stored: string = "field-init";
  set value(v: string) {
    acc.push("setter:" + v);
    this.stored = "via-setter:" + v;
  }
  get value(): string {
    return "getter(" + this.stored + ")";
  }
}
const q = new Q();
console.log("acc=" + acc.join("|"));
console.log("q-stored=" + q.stored);
console.log("q-value=" + q.value);
console.log("q-keys=" + Object.keys(q).join(","));
console.log("q-own-value=" + Object.prototype.hasOwnProperty.call(q, "value"));

// A base constructor that reads a derived FIELD gets undefined, and a base
// that reads a derived static gets it immediately — statics exist before any
// instance does.
const st: string[] = [];
class R {
  constructor() {
    st.push("field=" + String((this as any).later));
    st.push("static=" + String((this.constructor as any).ready));
  }
}
class S extends R {
  later: string = "installed-after";
  static ready: string = "static-ready";
}
const s = new S();
console.log("st=" + st.join("|"));
console.log("s-later=" + s.later);
console.log("s-ctor=" + (s.constructor === S));
