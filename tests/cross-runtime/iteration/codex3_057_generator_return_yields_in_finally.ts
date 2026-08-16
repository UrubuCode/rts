// Cross-runtime: generator return enters finally, which may yield before completion.
function* values() {
  try {
    yield "body";
  } finally {
    yield "cleanup";
  }
}
const it = values();
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.return("done")));
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.next()));

