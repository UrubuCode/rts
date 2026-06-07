// Cross-runtime: small event emitter with once listeners mutating during emit.
class Emitter {
  listeners: Function[] = [];
  on(fn: Function) { this.listeners.push(fn); }
  once(fn: Function) {
    const wrap = (...args: any[]) => {
      this.listeners = this.listeners.filter(x => x !== wrap);
      fn(...args);
    };
    this.on(wrap);
  }
  emit(v: string) {
    for (const fn of [...this.listeners]) fn(v);
  }
}

const log: string[] = [];
const e = new Emitter();
e.on((v: string) => log.push("a" + v));
e.once((v: string) => { log.push("once" + v); e.on((x: string) => log.push("late" + x)); });
e.emit("1");
e.emit("2");
console.log(log.join(","));
