// Cross-runtime: derived class field initialization order around super().
const log: string[] = [];

class Base {
  x = (log.push("base-field"), "base");
  constructor() {
    log.push("base-ctor:" + this.x);
  }
}

class Derived extends Base {
  x = (log.push("derived-field"), "derived");
  y = (log.push("derived-y:" + this.x), this.x + "-y");
  constructor() {
    log.push("before-super");
    super();
    log.push("after-super:" + this.x + ":" + this.y);
  }
}

const d = new Derived();
console.log(d.x + ":" + d.y);
console.log(log.join("|"));
