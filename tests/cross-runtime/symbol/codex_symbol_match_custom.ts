// Cross-runtime: custom Symbol.match and Symbol.replace hooks.
const matcher: any = {
  [Symbol.match](s: string) {
    return ["M:" + s.length];
  },
  [Symbol.replace](s: string, r: string) {
    return s.toUpperCase() + ":" + r;
  }
};

console.log("abc".match(matcher)!.join(","));
console.log("abc".replace(matcher, "x"));
