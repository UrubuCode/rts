// Cross-runtime: a return in generator finally overrides an injected throw.
function* guarded() {
  try {
    yield "ready";
  } finally {
    return "override";
  }
}
const it = guarded();
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.throw(new Error("boom"))));
console.log(JSON.stringify(it.next()));

