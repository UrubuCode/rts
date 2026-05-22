// Cross-runtime: advanced patterns with private fields.

// Observable with private subscriber list
class Observable<T> {
  #subscribers = new Set<(v: T) => void>();
  #lastValue: T | undefined;
  #hasValue = false;

  subscribe(fn: (v: T) => void) {
    this.#subscribers.add(fn);
    if (this.#hasValue) fn(this.#lastValue!);
    return () => this.#subscribers.delete(fn);  // unsubscribe
  }

  next(value: T) {
    this.#lastValue = value;
    this.#hasValue = true;
    this.#subscribers.forEach(fn => fn(value));
  }

  get subscriberCount() { return this.#subscribers.size; }
}

const obs = new Observable<number>();
const log: number[] = [];
const unsub = obs.subscribe(v => log.push(v));
obs.next(1); obs.next(2); obs.next(3);
unsub();
obs.next(4);  // not received
console.log(log.join(","));        // 1,2,3
console.log(obs.subscriberCount);  // 0

// Late subscriber gets last value immediately
const log2: number[] = [];
obs.subscribe(v => log2.push(v));  // gets 4 immediately (last value)
console.log(log2.join(","));       // 4

// Immutable value object with private backing
class Vec2 {
  #x: number;
  #y: number;

  constructor(x: number, y: number) { this.#x = x; this.#y = y; }

  get x() { return this.#x; }
  get y() { return this.#y; }

  add(other: Vec2) { return new Vec2(this.#x + other.#x, this.#y + other.#y); }
  scale(s: number) { return new Vec2(this.#x * s, this.#y * s); }
  dot(other: Vec2)  { return this.#x * other.#x + this.#y * other.#y; }
  get magnitude() { return Math.sqrt(this.#x ** 2 + this.#y ** 2); }
  toString() { return "Vec2(" + this.#x + "," + this.#y + ")"; }
}

const v1 = new Vec2(3, 4);
const v2 = new Vec2(1, 2);
console.log(v1.magnitude);           // 5
console.log(v1.add(v2).toString());  // Vec2(4,6)
console.log(v1.scale(2).toString()); // Vec2(6,8)
console.log(v1.dot(v2));             // 11

// State machine with private state
type State = "idle" | "running" | "paused" | "stopped";
class StateMachine {
  #state: State = "idle";
  #transitions: Map<State, State[]>;

  constructor() {
    this.#transitions = new Map([
      ["idle",    ["running"]],
      ["running", ["paused", "stopped"]],
      ["paused",  ["running", "stopped"]],
      ["stopped", []],
    ]);
  }

  #canTransition(to: State) {
    return (this.#transitions.get(this.#state) ?? []).includes(to);
  }

  transition(to: State): boolean {
    if (!this.#canTransition(to)) return false;
    this.#state = to;
    return true;
  }

  get state() { return this.#state; }
}

const sm = new StateMachine();
console.log(sm.state);                 // idle
console.log(sm.transition("running")); // true
console.log(sm.state);                 // running
console.log(sm.transition("idle"));    // false (not allowed)
console.log(sm.transition("paused"));  // true
console.log(sm.transition("stopped")); // true
console.log(sm.transition("running")); // false (terminal)
console.log(sm.state);                 // stopped
