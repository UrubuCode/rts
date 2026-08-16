// Cross-runtime: derived fields initialize after super returns and before constructor statements continue.
const seen: string[] = [];
class Base {
  base = (seen.push("base-field"), 1);
  constructor() { seen.push("base-ctor"); }
}
class Child extends Base {
  child = (seen.push("child-field:" + this.base), 2);
  constructor() {
    seen.push("before-super");
    super();
    seen.push("after-super:" + this.child);
  }
}
const c = new Child();
console.log(c.base, c.child);
console.log(seen.join("|"));

