// Cross-runtime: optional call keeps the correct this binding.
const obj: any = {
  x: 5,
  getSelf() {
    return this;
  },
  child: {
    x: 9,
    m() {
      return this.x;
    }
  }
};

console.log(obj.getSelf?.().x);
console.log(obj.child?.m?.());
const fn = obj.child?.m;
console.log(fn?.call({ x: 12 }));
console.log((null as any)?.m?.());
