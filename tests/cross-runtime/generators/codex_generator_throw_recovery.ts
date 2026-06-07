// Cross-runtime: throw into generator can be caught and continue.
function* gen() {
  try {
    yield "start";
  } catch (e: any) {
    yield "caught:" + e.message;
  }
  return "done";
}

const it = gen();
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.throw(new Error("boom"))));
console.log(JSON.stringify(it.next()));
