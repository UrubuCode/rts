// Cross-runtime: async iterators, async generators, for-await-of advanced.

// Async generator with error handling
async function* paginate(pages: number) {
  for (let i = 1; i <= pages; i++) {
    await Promise.resolve();  // simulate async fetch
    if (i === 3) throw new Error("page 3 failed");
    yield { page: i, data: "content-" + i };
  }
}

async function readPages() {
  const results: string[] = [];
  try {
    for await (const item of paginate(5)) {
      results.push(item.data);
    }
  } catch (e: any) {
    results.push("err:" + e.message);
  }
  return results;
}

// Async pipeline via async generators
async function* map<T, U>(iter: AsyncIterable<T>, fn: (v: T) => U): AsyncGenerator<U> {
  for await (const v of iter) yield fn(v);
}

async function* filter<T>(iter: AsyncIterable<T>, pred: (v: T) => boolean): AsyncGenerator<T> {
  for await (const v of iter) if (pred(v)) yield v;
}

async function* take<T>(iter: AsyncIterable<T>, n: number): AsyncGenerator<T> {
  let count = 0;
  for await (const v of iter) {
    if (count++ >= n) return;
    yield v;
  }
}

async function* range(start: number, end: number): AsyncGenerator<number> {
  for (let i = start; i <= end; i++) { await Promise.resolve(); yield i; }
}

async function collectAsync<T>(iter: AsyncIterable<T>): Promise<T[]> {
  const out: T[] = [];
  for await (const v of iter) out.push(v);
  return out;
}

// Async generator return value
async function* withReturn(): AsyncGenerator<number, string> {
  yield 1; yield 2;
  return "final";
}

async function main() {
  // paginate with error
  const pages = await readPages();
  console.log(pages.join(","));  // content-1,content-2,err:page 3 failed

  // pipeline
  const pipeline = take(filter(map(range(1, 20), x => x * x), x => x % 2 === 0), 4);
  const result = await collectAsync(pipeline);
  console.log(result.join(","));  // 4,16,36,64

  // async generator return
  const gen = withReturn();
  console.log((await gen.next()).value);  // 1
  console.log((await gen.next()).value);  // 2
  const ret = await gen.next();
  console.log(ret.value);                // final
  console.log(ret.done);                 // true

  // for-await over array of promises
  const promises = [
    Promise.resolve("a"),
    Promise.resolve("b"),
    Promise.resolve("c"),
  ];
  const vals: string[] = [];
  for await (const v of promises) vals.push(v);
  console.log(vals.join(","));  // a,b,c
}

main();
