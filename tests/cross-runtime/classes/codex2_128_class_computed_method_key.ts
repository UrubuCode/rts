// Cross-runtime: computed class method names evaluate once at definition time.
let calls = 0;
const key = () => { calls++; return "run"; };
class Worker {
  [key()]() { return 7; }
}
const w: any = new Worker();
console.log(w.run(), calls);
console.log(Object.keys(Worker.prototype).join(","));

