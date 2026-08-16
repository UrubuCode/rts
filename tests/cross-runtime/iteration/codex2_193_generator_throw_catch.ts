// Cross-runtime: generator throw resumes at the suspended yield and can be caught.
function* guarded() {
  try {
    yield "ready";
  } catch (e: any) {
    yield "caught:" + e.message;
  }
  return "done";
}
const it = guarded();
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.throw(new Error("x"))));
console.log(JSON.stringify(it.next()));

