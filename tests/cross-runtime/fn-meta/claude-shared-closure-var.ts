let shared = 10;
function reader(): number {
  return shared;
}
function writer(v: number): void {
  shared = v;
}
console.log(reader());
writer(20);
console.log(reader());

function makePair() {
  let v = 0;
  const get = () => v;
  const set = (x: number) => {
    v = x;
  };
  return { get, set };
}
const p = makePair();
console.log(p.get());
p.set(99);
console.log(p.get());
writer(reader() + p.get());
console.log(reader());