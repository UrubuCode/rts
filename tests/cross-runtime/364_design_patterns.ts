// Cross-runtime: design patterns (Observer, Singleton, Factory, Command).

// Observer / EventEmitter
class EventEmitter {
  private _listeners = new Map<string, Function[]>();
  on(event: string, fn: Function) {
    if (!this._listeners.has(event)) this._listeners.set(event, []);
    this._listeners.get(event)!.push(fn);
    return this;
  }
  off(event: string, fn: Function) {
    const list = this._listeners.get(event) ?? [];
    this._listeners.set(event, list.filter(f => f !== fn));
    return this;
  }
  emit(event: string, ...args: any[]) {
    (this._listeners.get(event) ?? []).forEach(f => f(...args));
    return this;
  }
}
const emitter = new EventEmitter();
const log: string[] = [];
const handler = (v: number) => log.push("got:" + v);
emitter.on("data", handler);
emitter.emit("data", 1).emit("data", 2);
emitter.off("data", handler);
emitter.emit("data", 3);  // not received
console.log(log.join(","));  // got:1,got:2

// Singleton
class Config {
  private static _inst: Config | null = null;
  private _data: Record<string, string> = {};
  private constructor() {}
  static getInstance() { return this._inst ?? (this._inst = new Config()); }
  set(k: string, v: string) { this._data[k] = v; }
  get(k: string) { return this._data[k]; }
}
const c1 = Config.getInstance();
const c2 = Config.getInstance();
c1.set("env", "prod");
console.log(c2.get("env"));       // prod — same instance
console.log(c1 === c2);           // true

// Factory
interface Shape { area(): number; perimeter(): number; }
class Circle implements Shape {
  constructor(private r: number) {}
  area() { return Math.PI * this.r * this.r; }
  perimeter() { return 2 * Math.PI * this.r; }
}
class Rect implements Shape {
  constructor(private w: number, private h: number) {}
  area() { return this.w * this.h; }
  perimeter() { return 2 * (this.w + this.h); }
}
function createShape(type: "circle" | "rect", ...args: number[]): Shape {
  if (type === "circle") return new Circle(args[0]);
  return new Rect(args[0], args[1]);
}
const circle = createShape("circle", 5);
const rect = createShape("rect", 3, 4);
console.log(Math.round(circle.area() * 100) / 100);   // 78.54
console.log(rect.area());                               // 12
console.log(rect.perimeter());                          // 14

// Command pattern
interface Command { execute(): void; undo(): void; }
class Counter {
  value = 0;
  history: Command[] = [];
  run(cmd: Command) { cmd.execute(); this.history.push(cmd); }
  undo() { this.history.pop()?.undo(); }
}
const counter = new Counter();
counter.run({ execute() { counter.value += 5; }, undo() { counter.value -= 5; } });
counter.run({ execute() { counter.value *= 2; }, undo() { counter.value /= 2; } });
console.log(counter.value);  // 10
counter.undo();
console.log(counter.value);  // 5
