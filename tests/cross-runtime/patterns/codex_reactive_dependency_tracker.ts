// Cross-runtime: tiny reactive dependency tracker using nested effects.
const deps = new Map<string, Set<Function>>();
let active: Function | null = null;
const state: any = new Proxy({ a: 1, b: 2 }, {
  get(t, k: string) {
    if (active) {
      if (!deps.has(k)) deps.set(k, new Set());
      deps.get(k)!.add(active);
    }
    return t[k];
  },
  set(t, k: string, v) {
    t[k] = v;
    deps.get(k)?.forEach(fn => fn());
    return true;
  }
});

const log: string[] = [];
function effect(fn: Function) {
  active = fn;
  fn();
  active = null;
}
effect(() => log.push("sum=" + (state.a + state.b)));
state.a = 5;
state.b = 7;
console.log(log.join("|"));
