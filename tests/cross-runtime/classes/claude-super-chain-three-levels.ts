// Cross-runtime: 3-level inheritance where every level calls super.method().
// Focus: call order + accumulated result through the whole chain.
const trace: string[] = [];

class L1 {
  name: string;
  constructor(name: string) {
    trace.push("L1-ctor:" + name);
    this.name = name;
  }
  describe(): string {
    trace.push("L1-describe");
    return "L1(" + this.name + ")";
  }
  tag(): string {
    return "l1";
  }
}

class L2 extends L1 {
  constructor(name: string) {
    trace.push("L2-ctor-before");
    super(name + "-2");
    trace.push("L2-ctor-after:" + this.name);
  }
  describe(): string {
    trace.push("L2-describe");
    return "L2[" + super.describe() + "]";
  }
  tag(): string {
    return super.tag() + ">l2";
  }
}

class L3 extends L2 {
  constructor(name: string) {
    trace.push("L3-ctor-before");
    super(name + "-3");
    trace.push("L3-ctor-after:" + this.name);
  }
  describe(): string {
    trace.push("L3-describe");
    return "L3{" + super.describe() + "}";
  }
  tag(): string {
    return super.tag() + ">l3";
  }
}

const c = new L3("x");
console.log("ctor_trace=" + trace.join("|"));
trace.length = 0;
console.log("describe=" + c.describe());
console.log("describe_trace=" + trace.join("|"));
console.log("tag=" + c.tag());
console.log("name=" + c.name);

// super.method() on a middle-level instance stops at its own chain
const m = new L2("y");
trace.length = 0;
console.log("mid_describe=" + m.describe());
console.log("mid_trace=" + trace.join("|"));
console.log("mid_tag=" + m.tag());

// super binding is lexical (based on the defining class), not on `this`
const borrowed = L2.prototype.describe;
trace.length = 0;
console.log("borrowed=" + borrowed.call(c));
console.log("borrowed_trace=" + trace.join("|"));

// calling a level-1 method directly through the deepest instance
console.log("direct_l1=" + L1.prototype.describe.call(c));
