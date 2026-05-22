// Cross-runtime: private fields in classes and subclass separation.
class Box {
  #x = 7;
  getX() { return this.#x; }
}
class SubBox extends Box {
  #x = 20;
  both() { return this.getX() + ":" + this.#x; }
}
const sub = new SubBox();
let err = "";
try { eval("sub.#x"); } catch (e: any) { err = e.name; }
console.log("inner=" + sub.both());
console.log("outer=" + err);
