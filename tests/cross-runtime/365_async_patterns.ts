// Cross-runtime: async/await + Promise patterns.

// Sequential vs parallel
async function sequential(): Promise<number> {
  const a = await Promise.resolve(1);
  const b = await Promise.resolve(2);
  const c = await Promise.resolve(3);
  return a + b + c;
}

async function parallel(): Promise<number> {
  const [a, b, c] = await Promise.all([
    Promise.resolve(10),
    Promise.resolve(20),
    Promise.resolve(30),
  ]);
  return a + b + c;
}

// Promise.allSettled
async function allSettled() {
  const results = await Promise.allSettled([
    Promise.resolve("ok"),
    Promise.reject(new Error("fail")),
    Promise.resolve("also ok"),
  ]);
  return results.map(r => r.status + ":" + (r.status === "fulfilled" ? r.value : (r as any).reason.message));
}

// async error handling
async function withError(): Promise<string> {
  try {
    await Promise.reject(new TypeError("bad input"));
    return "unreachable";
  } catch (e: any) {
    return "caught:" + e.message;
  }
}

// retry pattern
async function retry<T>(fn: () => Promise<T>, times: number): Promise<T> {
  let last: any;
  for (let i = 0; i < times; i++) {
    try { return await fn(); } catch (e) { last = e; }
  }
  throw last;
}

// async generator
async function* asyncRange(start: number, end: number) {
  for (let i = start; i <= end; i++) {
    await Promise.resolve();
    yield i;
  }
}

async function main() {
  console.log(await sequential());    // 6
  console.log(await parallel());      // 60

  const settled = await allSettled();
  console.log(settled.join("|"));     // fulfilled:ok|rejected:fail|fulfilled:also ok

  console.log(await withError());     // caught:bad input

  let attempts = 0;
  const result = await retry(() => {
    attempts++;
    if (attempts < 3) return Promise.reject(new Error("not yet"));
    return Promise.resolve("success");
  }, 5);
  console.log(result);       // success
  console.log(attempts);     // 3

  const vals: number[] = [];
  for await (const v of asyncRange(1, 5)) vals.push(v);
  console.log(vals.join(","));  // 1,2,3,4,5

  // Promise.race
  const raceResult = await Promise.race([
    new Promise(r => setTimeout(() => r("slow"), 50)),
    Promise.resolve("fast"),
  ]);
  console.log(raceResult);  // fast
}

main();
