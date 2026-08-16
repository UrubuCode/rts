// Cross-runtime: generator return executes finally before completing.
const seen: string[] = [];
function* values() {
  try {
    yield 1;
    yield 2;
  } finally {
    seen.push("finally");
  }
}
const it = values();
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.return(9)));
console.log(seen.join(","));
console.log(JSON.stringify(it.next()));

