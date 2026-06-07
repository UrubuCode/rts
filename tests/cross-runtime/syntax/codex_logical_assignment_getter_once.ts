// Cross-runtime: logical assignment evaluates property reference once.
const log: string[] = [];
const obj: any = {
  _x: 0,
  get x() { log.push("get:" + this._x); return this._x; },
  set x(v) { log.push("set:" + v); this._x = v; }
};

obj.x ||= 5;
obj.x &&= 7;
obj.x ??= 9;
console.log(obj._x);
console.log(log.join("|"));
