// Cross-runtime: destructuring closes iterator when default initializer throws.
const log: string[] = [];
const iterable = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        log.push("next" + i);
        return { value: i++ === 0 ? undefined : 2, done: false };
      },
      return() {
        log.push("return");
        return { done: true };
      }
    };
  }
};

try {
  const [x = (() => { throw new Error("default"); })(), y] = iterable as any;
  console.log(x, y);
} catch (e: any) {
  console.log(e.message);
}
console.log(log.join(","));
