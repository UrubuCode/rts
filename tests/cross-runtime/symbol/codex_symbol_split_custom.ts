// Cross-runtime: custom Symbol.split hook.
const splitter: any = {
  [Symbol.split](s: string, limit?: number) {
    return [s.length, limit ?? "none", s.slice(0, 2)];
  }
};

console.log("abcdef".split(splitter as any).join(","));
console.log("abcdef".split(splitter as any, 3).join(","));
