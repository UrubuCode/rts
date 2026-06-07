// Cross-runtime: generator finally runs on return().
function* gen() {
  try {
    yield 1;
    yield 2;
  } finally {
    yield 99;
  }
}

const it = gen();
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.return(7)));
console.log(JSON.stringify(it.next()));
