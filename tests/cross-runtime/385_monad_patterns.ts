// Cross-runtime: Option/Maybe and Result/Either monads.

// Option<T>
class Option<T> {
  #value: T | null;
  private constructor(v: T | null) { this.#value = v; }
  static some<T>(v: T): Option<T> { return new Option(v); }
  static none<T>(): Option<T> { return new Option<T>(null); }
  static of<T>(v: T | null | undefined): Option<T> {
    return v == null ? Option.none() : Option.some(v);
  }
  isSome() { return this.#value !== null; }
  isNone() { return this.#value === null; }
  map<U>(fn: (v: T) => U): Option<U> {
    return this.isSome() ? Option.some(fn(this.#value!)) : Option.none();
  }
  flatMap<U>(fn: (v: T) => Option<U>): Option<U> {
    return this.isSome() ? fn(this.#value!) : Option.none();
  }
  getOrElse(def: T): T { return this.isSome() ? this.#value! : def; }
  toString() { return this.isSome() ? "Some(" + this.#value + ")" : "None"; }
}

const users: Record<number, { name: string; age: number }> = {
  1: { name: "Alice", age: 30 },
  2: { name: "Bob", age: 17 },
};
const getUser = (id: number) => Option.of(users[id]);
const getName = (u: { name: string }) => u.name;
const getAdult = (u: { age: number; name: string }) =>
  u.age >= 18 ? Option.some(u) : Option.none<typeof u>();

console.log(getUser(1).map(getName).toString());              // Some(Alice)
console.log(getUser(99).map(getName).toString());             // None
console.log(getUser(1).flatMap(getAdult).map(getName).toString());  // Some(Alice)
console.log(getUser(2).flatMap(getAdult).map(getName).toString());  // None
console.log(getUser(99).getOrElse({ name: "Guest", age: 0 }).name); // Guest

// Result<T,E>
class Result<T, E> {
  private constructor(private _ok: T | null, private _err: E | null) {}
  static ok<T, E>(v: T): Result<T, E> { return new Result<T, E>(v, null); }
  static err<T, E>(e: E): Result<T, E> { return new Result<T, E>(null, e); }
  isOk()  { return this._err === null; }
  isErr() { return this._err !== null; }
  map<U>(fn: (v: T) => U): Result<U, E> {
    return this.isOk() ? Result.ok(fn(this._ok!)) : Result.err(this._err!);
  }
  flatMap<U>(fn: (v: T) => Result<U, E>): Result<U, E> {
    return this.isOk() ? fn(this._ok!) : Result.err(this._err!);
  }
  mapErr<F>(fn: (e: E) => F): Result<T, F> {
    return this.isErr() ? Result.err(fn(this._err!)) : Result.ok(this._ok!);
  }
  unwrap(): T { if (this.isOk()) return this._ok!; throw new Error("unwrap on Err: " + this._err); }
  unwrapOr(def: T): T { return this.isOk() ? this._ok! : def; }
  toString() { return this.isOk() ? "Ok(" + this._ok + ")" : "Err(" + this._err + ")"; }
}

function parseNum(s: string): Result<number, string> {
  const n = Number(s);
  return isNaN(n) ? Result.err("not a number: " + s) : Result.ok(n);
}
function safeDivide(a: number, b: number): Result<number, string> {
  return b === 0 ? Result.err("division by zero") : Result.ok(a / b);
}

const r1 = parseNum("42").flatMap(n => safeDivide(100, n));
const r2 = parseNum("0").flatMap(n => safeDivide(100, n));
const r3 = parseNum("abc").flatMap(n => safeDivide(100, n));

console.log(r1.toString());              // Ok(2.380952...)
console.log(Math.round(r1.unwrap() * 100) / 100);  // 2.38
console.log(r2.toString());              // Err(division by zero)
console.log(r3.toString());              // Err(not a number: abc)
console.log(r2.unwrapOr(-1));            // -1
