// Cross-runtime: computed member names interleaved with static fields.
const log: string[] = [];
function key(name: string) {
  log.push("key:" + name);
  return name;
}

class C {
  static [key("a")] = log.push("static-a");
  static #secret = log.push("private-static");
  [key("m")]() { return C.#secret; }
  static [key("b")] = log.push("static-b");
}

console.log(new C().m());
console.log((C as any).a + ":" + (C as any).b);
console.log(log.join("|"));
