// Cross-runtime: yield* delegation in generators.

function* inner(start: number, count: number) {
  for (let i = 0; i < count; i++) yield start + i;
}

function* outer() {
  yield 0;
  yield* inner(10, 3);  // delegates to inner
  yield 20;
  yield* [30, 31, 32]; // delegates to any iterable
  yield* "abc";         // string is iterable
}
console.log([...outer()].join(","));  // 0,10,11,12,20,30,31,32,a,b,c

// yield* return value (return value of delegated generator)
function* counter() {
  yield 1; yield 2;
  return "done";
}
function* delegator() {
  const result = yield* counter();  // captures return value
  yield "result:" + result;
}
console.log([...delegator()].join(","));  // 1,2,result:done

// recursive yield* (tree traversal)
type Tree = { val: number; children: Tree[] };
function* traverse(node: Tree): Generator<number> {
  yield node.val;
  for (const child of node.children) yield* traverse(child);
}
const tree: Tree = {
  val: 1,
  children: [
    { val: 2, children: [{ val: 4, children: [] }, { val: 5, children: [] }] },
    { val: 3, children: [{ val: 6, children: [] }] },
  ]
};
console.log([...traverse(tree)].join(","));  // 1,2,4,5,3,6

// yield* with infinite generator + take
function* naturals() { let n = 1; while (true) yield n++; }
function* take<T>(gen: Generator<T>, n: number) {
  for (let i = 0; i < n; i++) {
    const { value, done } = gen.next();
    if (done) return;
    yield value;
  }
}
function* pipeline() {
  yield* take(naturals(), 5);
}
console.log([...pipeline()].join(","));  // 1,2,3,4,5

// yield* in async generator
async function* asyncOuter() {
  yield* [1, 2, 3];
  yield* (async function*() { yield 4; yield 5; })();
}
(async () => {
  const vals: number[] = [];
  for await (const v of asyncOuter()) vals.push(v);
  console.log(vals.join(","));  // 1,2,3,4,5
})();
