// Cross-runtime: Proxy get/set receiver semantics with prototype accessors.
const proto: any = {
  get value() {
    return this._v + ":get";
  },
  set value(v: string) {
    this._v = v + ":set";
  }
};

const target = Object.create(proto);
target._v = "base";

const proxy = new Proxy(target, {
  get(t, k, r) {
    return Reflect.get(t, k, r);
  },
  set(t, k, v, r) {
    return Reflect.set(t, k, v, r);
  }
});

proxy.value = "new";
console.log(target._v);
console.log(proxy.value);
console.log(Object.prototype.hasOwnProperty.call(target, "_v"));
