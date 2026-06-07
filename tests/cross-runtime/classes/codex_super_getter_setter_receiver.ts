// Cross-runtime: super getter/setter use derived receiver.
class Base {
  get value() {
    return (this as any)._v + ":base-get";
  }
  set value(v: string) {
    (this as any)._v = v + ":base-set";
  }
}

class Derived extends Base {
  _v = "init";
  write(v: string) {
    super.value = v;
    return super.value;
  }
}

const d = new Derived();
console.log(d.write("x"));
console.log(d._v);
