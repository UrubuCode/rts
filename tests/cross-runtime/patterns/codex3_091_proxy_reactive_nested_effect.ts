// Cross-runtime: a small reactive graph combines Proxy receiver semantics and nested dependency tracking.
const deps = new Map<string, Set<() => void>>();
let active: (() => void) | null = null;
const state = new Proxy({ price: 2, quantity: 3 }, {
  get(target, key, receiver) {
    if (active) {
      const bucket = deps.get(String(key)) || new Set();
      bucket.add(active);
      deps.set(String(key), bucket);
    }
    return Reflect.get(target, key, receiver);
  },
  set(target, key, value, receiver) {
    const changed = Reflect.set(target, key, value, receiver);
    deps.get(String(key))?.forEach((fn) => fn());
    return changed;
  },
});
const seen: number[] = [];
function effect(fn: () => void) { active = fn; fn(); active = null; }
effect(() => seen.push(state.price * state.quantity));
state.price = 4;
state.quantity = 5;
console.log(seen.join(","));
console.log([...deps.keys()].join(","));

