// Cross-runtime: a false set trap causes strict assignment to throw without mutation.
export {};
const target: any = { x: 1 };
const seen: string[] = [];
const proxy = new Proxy(target, {
  set(t, key, value, receiver) {
    seen.push([String(key), value, receiver === proxy].join(":"));
    return false;
  },
});
let threw = false;
try { proxy.x = 9; } catch (e) { threw = e instanceof TypeError; }
console.log(threw, target.x, proxy.x);
console.log(seen.join("|"));

