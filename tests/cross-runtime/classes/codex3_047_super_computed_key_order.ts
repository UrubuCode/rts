// Cross-runtime: a computed super key evaluates before call arguments and only once.
const seen: string[] = [];
class Base {
  run(value: number) { seen.push("method:" + value); return value * 2; }
}
class Child extends Base {
  test() {
    const key = () => { seen.push("key"); return "run"; };
    const arg = () => { seen.push("arg"); return 4; };
    return super[key()](arg());
  }
}
console.log(new Child().test());
console.log(seen.join(","));
