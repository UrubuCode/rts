// Cross-runtime: iterator protocol — custom iterables, iterator helpers.

// Infinite iterator with take
function* naturals(start = 1) {
  let n = start;
  while (true) yield n++;
}
function take<T>(iter: Iterator<T>, n: number): T[] {
  const out: T[] = [];
  for (let i = 0; i < n; i++) {
    const { value, done } = iter.next();
    if (done) break;
    out.push(value);
  }
  return out;
}
console.log(take(naturals(), 5).join(","));       // 1,2,3,4,5
console.log(take(naturals(10), 4).join(","));     // 10,11,12,13

// zip two iterables
function* zip<A, B>(a: Iterable<A>, b: Iterable<B>): Generator<[A, B]> {
  const ia = a[Symbol.iterator](), ib = b[Symbol.iterator]();
  while (true) {
    const ra = ia.next(), rb = ib.next();
    if (ra.done || rb.done) return;
    yield [ra.value, rb.value];
  }
}
const zipped = [...zip([1,2,3], ["a","b","c"])];
zipped.forEach(([n, s]) => console.log(n + s));  // 1a, 2b, 3c

// flatten generator
function* flatten(iter: Iterable<any>, depth = Infinity): Generator<any> {
  for (const item of iter) {
    if (depth > 0 && typeof item[Symbol.iterator] === "function" && typeof item !== "string") {
      yield* flatten(item, depth - 1);
    } else {
      yield item;
    }
  }
}
console.log([...flatten([1,[2,[3,[4]]],5])].join(","));       // 1,2,3,4,5
console.log([...flatten([1,[2,[3]],4], 1)].join(","));        // 1,2,[3],4 → but [3] is array...
// depth=1: only first level flattened
const flat1 = [...flatten([1,[2,[3]],4], 1)];
console.log(flat1.length);   // 4

// Generator with throw
function* safeGen() {
  try {
    const v = yield "first";
    yield "second:" + v;
  } catch (e: any) {
    yield "caught:" + e.message;
  }
}
const g = safeGen();
console.log(g.next().value);          // first
console.log(g.throw(new Error("oops")).value);  // caught:oops

// return from iterator
function* countUp() { yield 1; yield 2; yield 3; }
const iter = countUp();
console.log(iter.next().value);   // 1
console.log(iter.return(99).value); // 99
console.log(iter.next().done);    // true
