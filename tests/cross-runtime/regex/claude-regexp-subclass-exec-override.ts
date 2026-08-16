// Cross-runtime: Symbol.replace / Symbol.match / Symbol.search all go through
// RegExpExec, which calls the OWN `exec` property if it is callable. So a
// subclass (or a patched instance) that overrides exec redirects every String
// method that takes a regex. Nothing in the corpus subclasses RegExp.

class Counting extends RegExp {
  calls = 0;
  exec(s: string): any {
    this.calls++;
    return super.exec(s);
  }
}

// --- the subclass is a working regex ---
const c0 = new Counting("a", "gi");
console.log("source=" + c0.source);
console.log("flags=" + c0.flags);
console.log("global=" + c0.global);
console.log("ignoreCase=" + c0.ignoreCase);
console.log("instanceof=" + (c0 instanceof RegExp) + ":" + (c0 instanceof Counting));
console.log("tostring=" + String(c0));
console.log("tag=" + Object.prototype.toString.call(c0));

// --- exec is called once per match by replace/match, once total by test/search ---
const r1 = new Counting("a", "g");
console.log("replace=" + "aba".replace(r1 as any, "-") + ":" + r1.calls);
const r2 = new Counting("a", "g");
console.log("match=" + (("aba".match(r2 as any) as any) || []).join(",") + ":" + r2.calls);
const r3 = new Counting("a", "g");
console.log("test=" + r3.test("aba") + ":" + r3.calls);
const r4 = new Counting("b", "");
console.log("search=" + "aba".search(r4 as any) + ":" + r4.calls);
const r5 = new Counting("a", "g");
console.log("matchAll=" + [..."aba".matchAll(r5 as any)].length + ":" + r5.calls);
const r6 = new Counting("a", "g");
console.log("replaceAll=" + "aba".replaceAll(r6 as any, "-") + ":" + r6.calls);

// --- rewriting the result array changes what replace inserts ---
class Loud extends RegExp {
  exec(s: string): any {
    const m = super.exec(s);
    if (m) m[0] = "<" + m[0] + ">";
    return m;
  }
}
console.log("loud-replace=" + "abc".replace(new Loud("b") as any, "[$&]"));
console.log("loud-match=" + ("abc".match(new Loud("b") as any) as any)[0]);
console.log("loud-split=" + "abc".split(new Loud("b") as any).join("|"));

// --- an exec that always returns null makes every method a no-op ---
const dead: any = /x/g;
dead.exec = () => null;
console.log("dead-replace=" + "xxx".replace(dead, "-"));
console.log("dead-match=" + String("xxx".match(dead)));
console.log("dead-test=" + dead.test("xxx"));
console.log("dead-search=" + "xxx".search(dead));

// --- a NON-callable own exec is ignored and the builtin runs ---
const shadowed: any = /b/;
shadowed.exec = 42;
console.log("noncallable=" + "abc".replace(shadowed, "-"));

// --- an exec returning a non-object is a TypeError ---
const bogus: any = /b/;
bogus.exec = () => 7;
try {
  console.log("bogus=" + "abc".replace(bogus, "-"));
} catch (e: any) {
  console.log("bogus!" + e.constructor.name);
}

// --- a hand-built result array works: index and length are read from it ---
const fake: any = /zzz/;
fake.exec = (s: string) => {
  const arr: any = ["bc", "c"];
  arr.index = 1;
  arr.input = s;
  arr.groups = undefined;
  return arr;
};
console.log("fake-replace=" + "abc".replace(fake, "[$&|$1]"));
console.log("fake-match=" + ("abc".match(fake) as any).index);
console.log("fake-search=" + "abc".search(fake));

// --- Symbol.species drives split's internal clone ---
console.log("species-is-RegExp=" + ((RegExp as any)[Symbol.species] === RegExp));
class Sub extends RegExp {}
console.log("split-subclass=" + "a,b,c".split(new Sub(",") as any).join("|"));
console.log("sub-species=" + ((Sub as any)[Symbol.species] === Sub));

// --- the subclass constructor is reached through new.target ---
console.log("ctor-name=" + c0.constructor.name);
console.log("proto-chain=" + (Object.getPrototypeOf(Counting.prototype) === RegExp.prototype));
console.log("own-exec=" + Object.prototype.hasOwnProperty.call(Counting.prototype, "exec"));
console.log("lastIndex-own=" + Object.prototype.hasOwnProperty.call(c0, "lastIndex"));
