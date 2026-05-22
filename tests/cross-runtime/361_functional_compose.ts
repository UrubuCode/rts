// Cross-runtime: functional composition patterns (pipe, compose, transducer).

// pipe: left-to-right
const pipe = (...fns: ((x: any) => any)[]) => (x: any) => fns.reduce((v, f) => f(v), x);

// compose: right-to-left
const compose = (...fns: ((x: any) => any)[]) => (x: any) => fns.reduceRight((v, f) => f(v), x);

const double = (x: number) => x * 2;
const addOne = (x: number) => x + 1;
const square = (x: number) => x * x;
const toStr  = (x: number) => "val:" + x;

const pipeline = pipe(double, addOne, square, toStr);
console.log(pipeline(3));   // (3*2+1)^2 = 49 → "val:49"

const composed = compose(toStr, square, addOne, double);
console.log(composed(3));   // same order, same result

// partial application
const partial = (fn: Function, ...pre: any[]) => (...rest: any[]) => fn(...pre, ...rest);
const mul = (a: number, b: number) => a * b;
const triple = partial(mul, 3);
console.log(triple(7));   // 21
console.log(triple(10));  // 30

// flip: swap first two args
const flip = (fn: Function) => (a: any, b: any, ...rest: any[]) => fn(b, a, ...rest);
const sub = (a: number, b: number) => a - b;
const flippedSub = flip(sub);
console.log(sub(10, 3));         // 7
console.log(flippedSub(10, 3));  // -7

// transduce
type Reducer<A, B> = (acc: A, val: B) => A;
function transduce<A, B>(
  xf: (r: Reducer<A, B>) => Reducer<A, B>,
  reducer: Reducer<A, B>,
  init: A,
  coll: B[]
): A {
  return coll.reduce(xf(reducer), init);
}
const mapXf = (fn: (x: number) => number) =>
  (next: Reducer<number[], number>) =>
    (acc: number[], val: number) => next(acc, fn(val));
const filterXf = (pred: (x: number) => boolean) =>
  (next: Reducer<number[], number>) =>
    (acc: number[], val: number) => pred(val) ? next(acc, val) : acc;

const xform = (r: Reducer<number[], number>) =>
  filterXf((x: number) => x % 2 === 0)(mapXf((x: number) => x * x)(r));

const result = transduce(xform, (acc, v) => (acc.push(v), acc), [] as number[], [1,2,3,4,5,6]);
console.log(result.join(","));  // 4,16,36
