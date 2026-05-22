// Cross-runtime: proxy-based property access obfuscation pattern.
// js-confuser wraps objects in Proxy to intercept all property reads.

function _makeGuard(target: any, secret: number) {
  const _map: Record<string, string> = {
    "\x67\x65\x74": "get",
    "\x73\x65\x74": "set",
    "\x6c\x6f\x67": "log",
  };
  return new Proxy(target, {
    get(t, prop) {
      const resolved = _map[prop as string] ?? prop;
      const val = t[resolved];
      if (typeof val === "function") return val.bind(t);
      return typeof val === "number" ? val ^ secret : val;
    },
    set(t, prop, val) {
      const resolved = _map[prop as string] ?? prop;
      t[resolved] = typeof val === "number" ? val ^ secret : val;
      return true;
    }
  });
}

const obj: any = { get: 0, set: 0, log: "test" };
const guarded = _makeGuard(obj, 0xFF);
guarded["\x73\x65\x74"] = 0x42 ^ 0xFF;  // stores 0x42^0xFF^0xFF = 0x42 = 66
console.log(guarded["\x67\x65\x74"]);    // reads back XOR'd: 66^0xFF = 0x42^0xFF^0xFF = 66... wait
// actually: store: val^0xFF → obj.set = (0x42^0xFF)^0xFF = 0x42 = 66
// read: obj.get=0... let's just test log
console.log(guarded["\x6c\x6f\x67"]);    // "test"

// function call interception via Proxy
const _calls: string[] = [];
function _intercept(fn: (...a: any[]) => any, name: string) {
  return new Proxy(fn, {
    apply(target, thisArg, args) {
      _calls.push(name + "(" + args.join(",") + ")");
      return target.apply(thisArg, args);
    }
  });
}
const add = _intercept((a: number, b: number) => a + b, "add");
const mul = _intercept((a: number, b: number) => a * b, "mul");
console.log(add(2, 3));
console.log(mul(4, 5));
console.log(_calls.join("|"));
