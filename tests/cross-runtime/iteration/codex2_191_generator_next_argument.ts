// Cross-runtime: next arguments become the values of suspended yield expressions.
function* exchange() {
  const a = yield "first";
  const b = yield a + ":second";
  return b + ":done";
}
const it = exchange();
console.log(JSON.stringify(it.next("ignored")));
console.log(JSON.stringify(it.next("A")));
console.log(JSON.stringify(it.next("B")));

