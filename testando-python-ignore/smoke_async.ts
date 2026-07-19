// TextEncoder/Decoder + Promise + async/await + queueMicrotask (microtask queue in std now)
const enc = new TextEncoder();
const bytes = enc.encode("hi");
console.log("enc-len:" + bytes.length);
const dec = new TextDecoder();
console.log("dec:" + dec.decode(bytes));

let order: string[] = [];
queueMicrotask(() => { order.push("micro"); });
order.push("sync");

async function f(x: number): Promise<number> { return x * 2; }
async function main() {
  const r = await f(21);
  console.log("await:" + r);
  await Promise.resolve(0);
  console.log("order:" + order.join(","));
}
main();

Promise.resolve(7).then((v: number) => { console.log("then:" + v); });
